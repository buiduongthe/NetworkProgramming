"""Bài mở rộng MR6.3 – Các thủ tục dùng chung cho server và client hội thoại video.

Khung hình (frame) là ma trận điểm ảnh; mỗi điểm RGB 3 byte. Khung 160×120:
  - kích thước thô: 160 × 120 × 3 = 57 600 byte; ở 10 khung/giây (fps) cần
    57 600 × 8 × 10 ≈ 4,6 Mbit/s – quá lớn, nên phải nén trước khi gửi;
  - ở đây nén bằng zlib (không mất dữ liệu, thư viện chuẩn); khung tổng hợp có "hạt nhiễu" giống
    cảm biến camera nên nén còn khoảng 7 KB -> 10 fps × 7 KB × 8 ≈ 560 kbit/s;
  - giảm độ phân giải 1/2 mỗi chiều -> số điểm ảnh còn 1/4, dung lượng giảm tương ứng.
Một khung nén (vài KB) lớn hơn một gói UDP an toàn, nên được chia mảnh (fragmentation):
mỗi datagram ≤ 1200 byte (mức mà QUIC – RFC 9000 – coi là an toàn trên Internet, dưới MTU 1500
của Ethernet trừ header IP/UDP và các đường hầm VPN), gồm header 20 byte + tối đa 1180 byte dữ liệu.
Bên nhận ghép đủ mảnh mới giải nén được; mất một mảnh là hỏng cả khung (khung bị bỏ).
"""
import json
import math
import random
import struct
import time
import zlib

RONG, CAO = 160, 120                 # độ phân giải gốc của camera
MAX_DATAGRAM = 1200
HEADER = struct.Struct("!BBHIHHHHI")  # 20 byte
DU_LIEU_TOI_DA = MAX_DATAGRAM - HEADER.size   # 1180 byte
PHIEN_BAN = 1
LOAI_MANH, LOAI_GIU = 1, 2           # mảnh khung hình; gói đăng ký/giữ kết nối
MANH_TOI_DA = 256                    # tối đa 256 mảnh/khung (≈ 300 KB): chặn khung giả quá lớn
MA_SERVER = 0


# ---------------------------------------------------------------- Chia mảnh và ghép khung
def chia_manh(ma, so_khung, rong, cao, du_lieu):
    """Header: phiên bản, loại, mã nguồn, số khung, chỉ số mảnh, tổng số mảnh, rộng, cao,
    nhãn thời gian (ms). Trả về danh sách datagram."""
    tong = max(1, math.ceil(len(du_lieu) / DU_LIEU_TOI_DA))
    ts = int(time.monotonic() * 1000) & 0xFFFFFFFF
    return [HEADER.pack(PHIEN_BAN, LOAI_MANH, ma, so_khung, i, tong, rong, cao, ts)
            + du_lieu[i * DU_LIEU_TOI_DA:(i + 1) * DU_LIEU_TOI_DA] for i in range(tong)]


def goi_giu(ma):
    return HEADER.pack(PHIEN_BAN, LOAI_GIU, ma, 0, 0, 0, 0, 0, 0)


def tach_header(goi):
    """Trả về (loại, mã, số khung, chỉ số, tổng, rộng, cao, payload) hoặc None nếu sai dạng."""
    if len(goi) < HEADER.size:
        return None
    pb, loai, ma, so, i, tong, w, h, _ = HEADER.unpack_from(goi)
    if pb != PHIEN_BAN or (loai == LOAI_MANH and (tong == 0 or tong > MANH_TOI_DA or i >= tong)):
        return None
    return loai, ma, so, i, tong, w, h, goi[HEADER.size:]


class GhepKhung:
    """Ghép các mảnh thành khung cho một luồng (một nguồn gửi).

    Mảnh có thể đến lệch thứ tự hoặc mất. Giữ các khung đang ghép dở; khi một khung đủ mảnh:
    trả về khung đó và bỏ mọi khung cũ hơn còn dở (không chờ nữa – video trực tiếp chỉ cần khung
    mới nhất). Khung dở quá `cho` giây cũng bị bỏ. Đếm: mảnh nhận, khung đủ, khung hỏng (thiếu mảnh),
    mảnh mất, khung mất hẳn (không nhận được mảnh nào), mảnh đến trễ.
    """

    def __init__(self, cho=0.5):
        self.cho = cho
        self.dang = {}                 # số khung -> {"tong", "manh": {chỉ số: bytes}, "t", "w", "h"}
        self.cuoi = None               # số khung đã xong/bỏ gần nhất
        self.manh_nhan = self.khung_du = self.khung_hong = 0
        self.manh_mat = self.khung_mat_han = self.tre = 0

    def _bo(self, so):
        k = self.dang.pop(so)
        self.khung_hong += 1
        self.manh_mat += k["tong"] - len(k["manh"])

    def them(self, so, i, tong, w, h, payload, bay_gio):
        self.manh_nhan += 1
        if self.cuoi is not None and so <= self.cuoi:
            self.tre += 1                                   # thuộc khung đã xong hoặc đã bỏ
            return None
        k = self.dang.setdefault(so, {"tong": tong, "manh": {}, "t": bay_gio, "w": w, "h": h})
        k["manh"][i] = payload
        for s in [s for s, d in self.dang.items() if bay_gio - d["t"] > self.cho and s != so]:
            self._bo(s)                                     # chờ quá lâu
        if len(k["manh"]) < k["tong"]:
            return None
        du_lieu = b"".join(k["manh"][j] for j in range(k["tong"]))
        del self.dang[so]
        cu = [s for s in self.dang if s < so]
        for s in cu:
            self._bo(s)                                     # khung cũ còn dở: bỏ
        if self.cuoi is not None:
            self.khung_mat_han += max(0, so - self.cuoi - 1 - len(cu))
        self.cuoi = so
        self.khung_du += 1
        return w, h, du_lieu


# ---------------------------------------------------------------- Ảnh RGB thô (bytearray)
# Phông chữ bitmap 5×7: mỗi ký tự 7 hàng, mỗi hàng 5 bit (bit cao = điểm bên trái), viết hex.
PHONG = {
    "A": "0E11111F111111", "B": "1E11111E11111E", "C": "0E11101010110E", "D": "1E11111111111E",
    "E": "1F10101E10101F", "F": "1F10101E101010", "G": "0E11101711110F",
    "H": "1111111F111111", "I": "0E04040404040E", "J": "0702020202120C",
    "K": "11121418141211", "L": "1010101010101F", "M": "111B1515111111", "N": "11111915131111",
    "O": "0E11111111110E", "P": "1E11111E101010", "Q": "0E11111115120D", "R": "1E11111E141211",
    "S": "0F10100E01011E", "T": "1F040404040404", "U": "1111111111110E", "V": "11111111110A04",
    "W": "1111111515150A", "X": "11110A040A1111", "Y": "1111110A040404", "Z": "1F01020408101F",
    "0": "0E11131519110E", "1": "040C040404040E", "2": "0E11010204081F", "3": "1F02040201110E",
    "4": "02060A121F0202", "5": "1F101E0101110E", "6": "0608101E11110E", "7": "1F010204080808",
    "8": "0E11110E11110E", "9": "0E11110F01020C", "#": "0A0A1F0A1F0A0A", "/": "01010204081010",
    ":": "000C0C000C0C00", ";": "000C0C000C0408", ".": "00000000000C0C", "-": "0000001F000000",
    "(": "02040808080402", ")": "08040202020408", "=": "00001F001F0000", ",": "000000000C0408",
    "?": "0E110102040004",
    " ": "00000000000000",
}


def to_mau(anh, w, x, y, rw, rh, mau):
    """Tô hình chữ nhật (x, y, rw, rh) bằng màu (r, g, b) – gán từng hàng bằng phép cắt lát."""
    hang = bytes(mau) * rw
    for yy in range(y, y + rh):
        anh[(yy * w + x) * 3:(yy * w + x + rw) * 3] = hang


def ve_chu(anh, w, h, x, y, chu, co=1, mau=(255, 255, 255)):
    """Vẽ chuỗi chữ (chỉ chữ in hoa không dấu, số, vài ký hiệu) bằng phông 5×7 phóng `co` lần."""
    for c in chu.upper():
        ma = PHONG.get(c, PHONG["?"])
        for hang in range(7):
            bits = int(ma[2 * hang:2 * hang + 2], 16)
            for cot in range(5):
                if bits & (0x10 >> cot):
                    px, py = x + cot * co, y + hang * co
                    if px + co <= w and py + co <= h:
                        to_mau(anh, w, px, py, co, co, mau)
        x += 6 * co


def thu_nho(anh, w, h, d):
    """Giảm độ phân giải d lần mỗi chiều: lấy 1 điểm trong mỗi khối d×d (lấy mẫu thưa)."""
    if d == 1:
        return bytes(anh), w, h
    w2, h2 = w // d, h // d
    ra = bytearray(w2 * h2 * 3)
    for y in range(h2):
        hang = anh[y * d * w * 3:(y * d + 1) * w * 3]     # cứ d hàng lấy 1
        o = y * w2 * 3
        for c in range(3):                                # mỗi kênh màu: cứ d điểm lấy 1
            ra[o + c:o + w2 * 3:3] = hang[c:w2 * d * 3:3 * d]
    return bytes(ra), w2, h2


def phong_to(anh, w, h, k):
    """Phóng to k lần bằng lặp điểm gần nhất (nearest neighbour): ảnh độ phân giải thấp thành ô vuông."""
    if k == 1:
        return bytes(anh)
    ra = []
    for y in range(h):
        hang = anh[y * w * 3:(y + 1) * w * 3]
        rong = b"".join(hang[i:i + 3] * k for i in range(0, len(hang), 3))
        ra.extend([rong] * k)
    return b"".join(ra)


# ---------------------------------------------------------------- Camera tổng hợp
BANG_MAU = [(230, 120, 60), (70, 170, 90), (70, 120, 220), (200, 80, 180), (200, 190, 60)]


class CameraTongHop:
    """Khung hình giả lập: nền sóng chéo màu riêng của từng client, có hạt nhiễu thưa như cảm
    biến thật, trượt ngang theo thời gian (chuyển động); vẽ tên client và số khung."""

    def __init__(self, ten, ma):
        self.ten = ten
        mau = BANG_MAU[(ma - 1) % len(BANG_MAU)]
        r = random.Random(ma)
        nen = bytearray(RONG * CAO * 3)
        for y in range(CAO):
            for x in range(RONG):
                k = 0.35 + 0.65 * (0.5 + 0.5 * math.sin(2 * math.pi * (x / 40 + y / 60)))
                hat = r.randrange(24) if r.random() < 0.04 else 0       # hạt nhiễu
                i = (y * RONG + x) * 3
                for c in range(3):
                    nen[i + c] = min(255, int(mau[c] * k) // 16 * 16 + hat)
        self.nen = bytes(nen)

    def lay_khung(self, so):
        dich = (so * 2) % RONG * 3                          # trượt 2 điểm ảnh mỗi khung
        anh = bytearray(b"".join(self.nen[y * RONG * 3:(y + 1) * RONG * 3][dich:]
                                 + self.nen[y * RONG * 3:(y + 1) * RONG * 3][:dich]
                                 for y in range(CAO)))
        to_mau(anh, RONG, 6, 6, 6 * 4 * len(self.ten) + 6, 7 * 4 + 6, (20, 20, 20))
        ve_chu(anh, RONG, CAO, 9, 9, self.ten, 4)
        nhan = f"#{so:04d}"
        to_mau(anh, RONG, 6, CAO - 34, 6 * 2 * len(nhan) + 4, 7 * 2 + 4, (20, 20, 20))
        ve_chu(anh, RONG, CAO, 8, CAO - 32, nhan, 2)
        return anh


def giai_nen(du_lieu, toi_da=4 << 20):
    """Giải nén zlib có giới hạn kích thước đầu ra (chống "bom nén" từ client xấu)."""
    d = zlib.decompressobj()
    ra = d.decompress(du_lieu, toi_da)
    if d.unconsumed_tail:
        raise zlib.error("dữ liệu giải nén quá lớn")
    return ra


# ---------------------------------------------------------------- PNG viết tay (zlib + struct)
def ghi_png(duong_dan, anh, w, h):
    """PNG = chữ ký 8 byte + các khối (độ dài, kiểu, dữ liệu, CRC-32): IHDR, IDAT, IEND.
    IDAT là zlib của các hàng ảnh, mỗi hàng bắt đầu bằng 1 byte bộ lọc (0 = không lọc)."""
    def khoi(kieu, du_lieu):
        return (struct.pack("!I", len(du_lieu)) + kieu + du_lieu
                + struct.pack("!I", zlib.crc32(kieu + du_lieu) & 0xFFFFFFFF))
    hang = b"".join(b"\x00" + bytes(anh[y * w * 3:(y + 1) * w * 3]) for y in range(h))
    ihdr = struct.pack("!IIBBBBB", w, h, 8, 2, 0, 0, 0)     # 8 bit/kênh, kiểu màu 2 = RGB
    with open(duong_dan, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + khoi(b"IHDR", ihdr) + khoi(b"IDAT", zlib.compress(hang, 9))
                + khoi(b"IEND", b""))


# ---------------------------------------------------------------- Kênh điều khiển JSON
def gui_json(sock, obj):
    """Mỗi thông điệp điều khiển là một dòng JSON kết thúc bằng '\\n'."""
    sock.sendall((json.dumps(obj, ensure_ascii=False) + "\n").encode("utf-8"))
