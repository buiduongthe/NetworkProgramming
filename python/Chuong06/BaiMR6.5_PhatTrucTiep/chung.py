"""Bài mở rộng MR6.5 – Các thủ tục dùng chung cho máy chủ, máy phát và người xem (phát trực tiếp 1→n).

Tự chứa (không nhập mã của bài khác): chép và mở rộng phần cần thiết của MR6.2 (âm thanh RTP, bộ đệm
jitter) và MR6.3 (header video, chia mảnh, ghép khung, phông chữ 5×7, PNG viết tay).

Hai loại datagram trên CÙNG một socket UDP, phân biệt bằng byte đầu tiên:
  - 0x80: gói RTP âm thanh (RFC 3550): V=2, P=0, X=0, CC=0 -> byte đầu luôn là 0x80;
  - 0x01: gói video của bài (phiên bản 1): loại 2 = giữ kết nối, 3 = mảnh khung I, 4 = mảnh khung P.

Nén liên khung (inter-frame) đơn giản – ý tưởng của mọi codec video (H.264, VP8, AV1):
  - khung khóa I (intra): nén một mình  ->  I = zlib(rgb thô);
  - khung sai khác P (predicted): chỉ mã hóa phần THAY ĐỔI so với khung ngay trước
        P = zlib(XOR từng byte(khung hiện tại, khung trước))
    cảnh tĩnh -> XOR gần như toàn 0 -> nén còn vài trăm byte. Giải mã P cần đúng khung trước:
    mất một khung là hỏng cả chuỗi tới khung I kế tiếp (nhóm khung – GOP, Group Of Pictures).
"""
import array
import json
import math
import random
import struct
import sys
import time
import zlib

# ---------------------------------------------------------------- Âm thanh (như MR6.2)
TAN_SO_MAU = 16000                               # mẫu/giây
KHUNG_MS = 20                                    # một gói âm thanh = 20 ms
MAU_MOI_KHUNG = TAN_SO_MAU * KHUNG_MS // 1000    # 320 mẫu
BYTE_MOI_KHUNG = MAU_MOI_KHUNG * 2               # 640 byte (L16)
PT_AM_THANH = 96                                 # payload type động (RFC 3551) cho L16/16000/mono
RTP = struct.Struct("!BBHII")                    # 12 byte: V|P|X|CC, M|PT, seq, ts, ssrc
DAU_RTP = 0x80

# ---------------------------------------------------------------- Video (như MR6.3)
RONG, CAO = 160, 120
FPS_MAC_DINH, GOP_MAC_DINH = 10, 20
MAX_DATAGRAM = 1200
HEADER = struct.Struct("!BBHIHHHHI")             # 20 byte
DU_LIEU_TOI_DA = MAX_DATAGRAM - HEADER.size      # 1180 byte
PHIEN_BAN = 1                                    # cũng là byte đầu của gói video (0x01)
LOAI_GIU, LOAI_I, LOAI_P = 2, 3, 4
MANH_TOI_DA = 256

# ---------------------------------------------------------------- Giới hạn, thời gian chờ
GIOI_HAN_DONG = 64 * 1024        # một dòng JSON điều khiển tối đa 64 KB
BO_DEM_GOP_TOI_DA = 4 << 20      # bộ đệm GOP ở server tối đa 4 MB
PLI_SERVER_S = 0.5               # server chuyển xin_khung_khoa tối đa 1 lần / 500 ms
PLI_XEM_S = 1.0                  # người xem gửi xin_khung_khoa tối đa 1 lần / 1 s
CHO_THAM_CHIEU_S = 0.2           # chờ khung thiếu trước khi coi là mất tham chiếu
CHO_MANH_S = 0.5                 # chờ đủ mảnh của một khung
HANG_HIEN_THI = 30               # hàng khung chờ hiển thị tối đa 30 khung
DO_SAU_MAC_DINH = 5              # bộ đệm jitter 5 khung = 100 ms


# ================================================================ RTP âm thanh
def dong_goi_rtp(seq, ts, ssrc, payload=b""):
    """Header RTP 12 byte: 0x80 (V=2), PT=96 (M=0), số thứ tự, nhãn thời gian (mẫu), SSRC."""
    return RTP.pack(DAU_RTP, PT_AM_THANH, seq & 0xFFFF, ts & 0xFFFFFFFF, ssrc & 0xFFFFFFFF) + payload


def tach_rtp(goi):
    """Trả về (pt, seq, ts, ssrc, payload) hoặc None nếu sai dạng."""
    if len(goi) < RTP.size:
        return None
    b0, b1, seq, ts, ssrc = RTP.unpack_from(goi)
    if b0 >> 6 != 2:
        return None
    return b1 & 0x7F, seq, ts, ssrc, goi[RTP.size:]


def pcm_sang_byte(mau):
    """L16 truyền theo thứ tự byte mạng (big-endian) – RFC 3551 mục 4.5.11."""
    a = array.array("h", mau)
    if sys.byteorder == "little":
        a.byteswap()
    return a.tobytes()


def byte_sang_pcm(b):
    a = array.array("h")
    a.frombytes(b)
    if sys.byteorder == "little":
        a.byteswap()
    return a


def muc_dbfs(mau):
    """Mức RMS theo dBFS (0 dBFS = toàn thang 32768)."""
    if not mau:
        return -math.inf
    rms = math.sqrt(sum(x * x for x in mau) / len(mau))
    return 20 * math.log10(rms / 32768) if rms > 0 else -math.inf


class BoDemJitter:
    """Bộ đệm chống rung trễ (jitter buffer) cho luồng RTP – như MR6.2, thêm nhãn thời gian.

    Giữ `do_sau` khung trước khi bắt đầu phát, rồi cứ 20 ms lấy đúng khung có số thứ tự kế tiếp:
      - có khung  -> trả về (ts, payload);
      - thiếu khung nhưng đã có khung sau -> mất: trả về (ts ước lượng, None) – phát im lặng,
        đồng hồ phát vẫn tiến 20 ms;
      - rỗng -> cạn: trả về None (đồng hồ phát dừng), chờ nạp lại `do_sau` khung.
    """

    def __init__(self, do_sau=DO_SAU_MAC_DINH, toi_da=60):
        self.do_sau, self.toi_da = do_sau, toi_da
        self.khung = {}                  # số thứ tự mở rộng -> (ts, payload)
        self.ke_tiep = None
        self.dang_phat = False
        self.seq_lon_nhat = self.seq_dau = None
        self.ts_cuoi = 0
        self.nhan = self.tre = self.thieu = self.can = self.tran = 0

    def _mo_rong(self, seq):
        """Số thứ tự 16 bit quay vòng -> số nguyên tăng mãi."""
        if self.seq_lon_nhat is None:
            return seq
        lech = (seq - self.seq_lon_nhat) & 0xFFFF
        return self.seq_lon_nhat + lech if lech < 0x8000 else self.seq_lon_nhat - (0x10000 - lech)

    def nhan_goi(self, seq, ts, payload):
        s = self._mo_rong(seq)
        if self.seq_dau is None:
            self.seq_dau = s
        if self.seq_lon_nhat is None or s > self.seq_lon_nhat:
            self.seq_lon_nhat = s
        self.nhan += 1
        if self.dang_phat and s < self.ke_tiep:
            self.tre += 1                              # đến sau lượt phát của nó
            return
        self.khung[s] = (ts, payload)
        if len(self.khung) > self.toi_da:
            del self.khung[min(self.khung)]
            self.tran += 1

    def lay(self):
        """Gọi mỗi 20 ms. Trả về (ts, payload|None) hoặc None (đang chờ nạp)."""
        if not self.dang_phat:
            if len(self.khung) < self.do_sau:
                return None
            self.dang_phat, self.ke_tiep = True, min(self.khung)
        if self.ke_tiep in self.khung:
            ts, p = self.khung.pop(self.ke_tiep)
            self.ke_tiep += 1
            self.ts_cuoi = ts
            return ts, p
        if not self.khung:
            self.dang_phat = False
            self.can += 1
            return None
        self.thieu += 1                                # mất khung này
        self.ke_tiep += 1
        self.ts_cuoi = (self.ts_cuoi + MAU_MOI_KHUNG) & 0xFFFFFFFF
        return self.ts_cuoi, None

    def xoa(self):
        """Bỏ mọi khung đang giữ (khi tắt nhận âm thanh); lần sau nạp lại từ đầu."""
        self.khung.clear()
        self.dang_phat = False

    def dat_do_sau(self, n):
        """Đổi độ sâu khi đang chạy: tăng -> dừng phát để nạp thêm; giảm -> bỏ khung cũ nhất."""
        self.do_sau = n
        if len(self.khung) < n:
            self.dang_phat = False
        while len(self.khung) > n:
            s = min(self.khung)
            del self.khung[s]
            if self.dang_phat:
                self.ke_tiep = s + 1

    @property
    def mat(self):
        """Số gói mất theo RFC 3550: số gói mong đợi trừ số gói đã nhận."""
        if self.seq_dau is None:
            return 0
        return max(0, self.seq_lon_nhat - self.seq_dau + 1 - self.nhan)


# ================================================================ Gói video
def chia_manh(loai, ma, so_khung, rong, cao, ts_ms, du_lieu):
    """Chia dữ liệu một khung (I hoặc P) thành datagram ≤ 1200 byte.
    Header: phiên bản 1, loại, mã nguồn, số khung, chỉ số mảnh, tổng số mảnh, rộng, cao, nhãn ms."""
    tong = max(1, math.ceil(len(du_lieu) / DU_LIEU_TOI_DA))
    if tong > MANH_TOI_DA:
        raise ValueError("khung quá lớn")
    return [HEADER.pack(PHIEN_BAN, loai, ma, so_khung & 0xFFFFFFFF, i, tong, rong, cao, ts_ms & 0xFFFFFFFF)
            + du_lieu[i * DU_LIEU_TOI_DA:(i + 1) * DU_LIEU_TOI_DA] for i in range(tong)]


def goi_giu(ma):
    """Gói giữ kết nối / đăng ký địa chỉ UDP: loại 2, các trường khác 0."""
    return HEADER.pack(PHIEN_BAN, LOAI_GIU, ma, 0, 0, 0, 0, 0, 0)


def tach_header(goi):
    """Trả về (loại, mã, số khung, chỉ số, tổng, rộng, cao, ts, payload) hoặc None nếu sai dạng."""
    if len(goi) < HEADER.size or len(goi) > MAX_DATAGRAM:
        return None
    pb, loai, ma, so, i, tong, w, h, ts = HEADER.unpack_from(goi)
    if pb != PHIEN_BAN or loai not in (LOAI_GIU, LOAI_I, LOAI_P):
        return None
    if loai != LOAI_GIU and (tong == 0 or tong > MANH_TOI_DA or i >= tong or w == 0 or h == 0):
        return None
    return loai, ma, so, i, tong, w, h, ts, goi[HEADER.size:]


class GhepKhung:
    """Ghép mảnh thành khung (I hoặc P). Khác MR6.3: KHÔNG bỏ khung cũ còn dở khi khung mới đủ,
    vì khung P cần mọi khung trước nó – khung dở chỉ bị bỏ khi chờ quá `cho` giây."""

    def __init__(self, cho=CHO_MANH_S):
        self.cho = cho
        self.dang = {}                   # số khung -> {"dau": (loai, tong, w, h, ts), "manh": {}, "t"}
        self.xong = set()                # khung đã đủ hoặc đã bỏ (mảnh đến sau = trễ)
        self.lon_nhat = -1
        self.manh_nhan = self.khung_du = self.khung_hong = self.manh_mat = self.tre = 0

    def don_dep(self, bay_gio, tru=None):
        for s in [s for s, d in self.dang.items() if bay_gio - d["t"] > self.cho and s != tru]:
            d = self.dang.pop(s)
            self.khung_hong += 1
            self.manh_mat += d["dau"][1] - len(d["manh"])
            self.xong.add(s)

    def bo_dang_do(self):
        """Bỏ các khung đang ghép dở mà không tính là hỏng (khi tắt nhận video)."""
        self.xong.update(self.dang)
        self.dang.clear()

    def them(self, loai, so, i, tong, w, h, ts, payload, bay_gio):
        """Trả về (loại, số khung, ts, rộng, cao, dữ liệu) khi khung vừa đủ mảnh, ngược lại None."""
        self.manh_nhan += 1
        if so in self.xong or so < self.lon_nhat - 1000:
            self.tre += 1
            return None
        dau = (loai, tong, w, h, ts)
        k = self.dang.setdefault(so, {"dau": dau, "manh": {}, "t": bay_gio})
        if k["dau"] != dau:
            return None                                  # mâu thuẫn với mảnh trước: bỏ mảnh
        k["manh"][i] = payload
        self.don_dep(bay_gio, tru=so)
        if len(k["manh"]) < tong:
            return None
        del self.dang[so]
        self.xong.add(so)
        self.lon_nhat = max(self.lon_nhat, so)
        if len(self.xong) > 3000:
            self.xong = {s for s in self.xong if s > self.lon_nhat - 1000}
        self.khung_du += 1
        return loai, so, ts, w, h, b"".join(k["manh"][j] for j in range(tong))


# ================================================================ Mã hóa / giải mã I, P
def xor_khung(a, b):
    """XOR từng byte hai khung cùng kích thước (dùng số nguyên lớn của Python cho nhanh)."""
    return (int.from_bytes(a, "little") ^ int.from_bytes(b, "little")).to_bytes(len(a), "little")


def giai_nen(du_lieu, toi_da=4 << 20):
    """Giải nén zlib có giới hạn kích thước đầu ra (chống "bom nén")."""
    d = zlib.decompressobj()
    ra = d.decompress(du_lieu, toi_da)
    if d.unconsumed_tail:
        raise zlib.error("dữ liệu giải nén quá lớn")
    return ra


def ma_hoa_i(anh):
    return zlib.compress(bytes(anh), 6)


def ma_hoa_p(anh, truoc):
    return zlib.compress(xor_khung(bytes(anh), truoc), 6)


class GiaiMaVideo:
    """Giải mã chuỗi I/P theo thứ tự số khung.

    - I: giải nén -> thành khung tham chiếu (bỏ mọi khung cũ hơn đang chờ).
    - P số k: giải được khi tham chiếu là k−1. Nếu khung k đủ mảnh mà tham chiếu < k−1: giữ chờ
      khung thiếu tối đa CHO_THAM_CHIEU_S; quá hạn -> MẤT THAM CHIẾU: đếm đứng hình, bỏ mọi P tới I
      kế tiếp, kiem_tra() trả True để người xem gửi xin_khung_khoa.
    - Chưa từng có tham chiếu (vừa vào xem): chỉ chờ I, không tính đứng hình.
    """

    def __init__(self, cho=CHO_THAM_CHIEU_S):
        self.cho = cho
        self.tham_chieu = None           # (số khung, ảnh, rộng, cao)
        self.mat_tham_chieu = False
        self.luc_mat = None
        self.doi = {}                    # số khung P -> (ts, w, h, dữ liệu, lúc đủ mảnh)
        self.so_i = self.so_p = self.bo_p = self.dung_hinh = self.loi = 0

    def them(self, loai, so, ts, w, h, du_lieu, bay_gio):
        """Trả về danh sách khung vừa giải được [(số, loại, ts, w, h, ảnh)]."""
        ra = []
        if loai == LOAI_I:
            if self.tham_chieu is not None and so <= self.tham_chieu[0]:
                return ra                                 # I cũ hơn tham chiếu hiện tại
            try:
                anh = giai_nen(du_lieu)
            except zlib.error:
                self.loi += 1
                return ra
            if len(anh) != w * h * 3:
                self.loi += 1
                return ra
            self.tham_chieu = (so, anh, w, h)
            self.mat_tham_chieu = False
            self.doi = {s: v for s, v in self.doi.items() if s > so}
            self.so_i += 1
            ra.append((so, LOAI_I, ts, w, h, anh))
            self._giai_tiep(ra)
            return ra
        if self.tham_chieu is None or self.mat_tham_chieu:
            self.bo_p += 1                                # chưa có / mất tham chiếu: chờ I
            return ra
        if so <= self.tham_chieu[0]:
            return ra
        self.doi[so] = (ts, w, h, du_lieu, bay_gio)
        self._giai_tiep(ra)
        return ra

    def _giai_tiep(self, ra):
        while self.tham_chieu is not None and self.tham_chieu[0] + 1 in self.doi:
            so = self.tham_chieu[0] + 1
            ts, w, h, du_lieu, _ = self.doi.pop(so)
            _, truoc, tw, th = self.tham_chieu
            try:
                x = giai_nen(du_lieu)
            except zlib.error:
                x = b""
            if len(x) != len(truoc) or (w, h) != (tw, th):
                self.loi += 1                             # P hỏng: coi như thiếu khung này
                return
            anh = xor_khung(x, truoc)
            self.tham_chieu = (so, anh, w, h)
            self.so_p += 1
            ra.append((so, LOAI_P, ts, w, h, anh))

    def kiem_tra(self, bay_gio):
        """True nếu vừa mất tham chiếu (khung thiếu không tới sau CHO_THAM_CHIEU_S)."""
        if not self.doi:
            return False
        if bay_gio - min(v[4] for v in self.doi.values()) < self.cho:
            return False
        self.mat_tham_chieu = True
        self.luc_mat = bay_gio
        self.dung_hinh += 1
        self.bo_p += len(self.doi)
        self.doi.clear()
        return True

    def lam_moi(self):
        self.__init__(self.cho)


# ================================================================ Ảnh RGB, phông 5×7, PNG
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
    "+": "0004041F040400", "?": "0E110102040004",
    " ": "00000000000000",
}


def to_mau(anh, w, x, y, rw, rh, mau):
    """Tô hình chữ nhật (x, y, rw, rh) bằng màu (r, g, b)."""
    rw = max(0, min(rw, w - x))
    hang = bytes(mau) * rw
    for yy in range(y, y + rh):
        dau = (yy * w + x) * 3
        if dau + rw * 3 > len(anh):
            break
        anh[dau:dau + rw * 3] = hang


def ve_chu(anh, w, h, x, y, chu, co=1, mau=(255, 255, 255)):
    """Vẽ chuỗi (chữ in hoa không dấu, số, vài ký hiệu) bằng phông 5×7 phóng `co` lần."""
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


def phong_to(anh, w, h, k):
    """Phóng to k lần bằng lặp điểm gần nhất."""
    if k == 1:
        return bytes(anh)
    ra = []
    for y in range(h):
        hang = anh[y * w * 3:(y + 1) * w * 3]
        rong = b"".join(hang[i:i + 3] * k for i in range(0, len(hang), 3))
        ra.extend([rong] * k)
    return b"".join(ra)


def ghi_png(duong_dan, anh, w, h):
    """PNG viết tay: chữ ký 8 byte + khối IHDR, IDAT (zlib các hàng, byte lọc 0), IEND; mỗi khối CRC-32."""
    def khoi(kieu, du_lieu):
        return (struct.pack("!I", len(du_lieu)) + kieu + du_lieu
                + struct.pack("!I", zlib.crc32(kieu + du_lieu) & 0xFFFFFFFF))
    hang = b"".join(b"\x00" + bytes(anh[y * w * 3:(y + 1) * w * 3]) for y in range(h))
    ihdr = struct.pack("!IIBBBBB", w, h, 8, 2, 0, 0, 0)
    with open(duong_dan, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + khoi(b"IHDR", ihdr) + khoi(b"IDAT", zlib.compress(hang, 9))
                + khoi(b"IEND", b""))


def ghep_anh(duong_dan, ds, cot=2):
    """Ghép các ảnh (rộng, cao, rgb, nhãn) thành một PNG: `cot` cột, nhãn dưới mỗi ảnh."""
    o_w = max(w for w, _, _, _ in ds) + 16
    cao_hang = [max(h for _, h, _, _ in ds[i:i + cot]) + 30 for i in range(0, len(ds), cot)]
    W, H = min(cot, len(ds)) * o_w + 16, sum(cao_hang) + 8
    anh = bytearray(b"\xff" * (W * H * 3))
    for n, (w, h, du_lieu, nhan) in enumerate(ds):
        x0 = 16 + (n % cot) * o_w
        y0 = 8 + sum(cao_hang[:n // cot])
        for y in range(h):
            anh[((y0 + y) * W + x0) * 3:((y0 + y) * W + x0 + w) * 3] = du_lieu[y * w * 3:(y + 1) * w * 3]
        ve_chu(anh, W, H, x0, y0 + h + 8, nhan[:o_w // 12], 2, (0, 0, 0))
    ghi_png(duong_dan, anh, W, H)
    return W, H


# ================================================================ Nguồn giả lập "bảng clapper"
BANG_MAU = [(230, 120, 60), (70, 170, 90), (70, 120, 220), (200, 80, 180), (200, 190, 60)]


class NguonClapper:
    """Camera giả lập kiểu bảng clapper của nhà quay phim: nền tĩnh (sóng chéo + hạt nhiễu thưa như
    MR6.3), một ô vuông chạy ngang (chuyển động nhỏ), đồng hồ phương tiện T=12.345S và số khung.
    Ở mỗi giây tròn (ts % 1000 < 1000/fps) cả khung CHỚP TRẮNG (giữ mờ vân nền) – cùng lúc tiếng bíp của
    NguonAmClapper: nhìn hình, nghe tiếng là thấy lệch tiếng-hình."""

    def __init__(self, ten, ma=1, fps=FPS_MAC_DINH):
        self.ten, self.fps = ten, fps
        mau = BANG_MAU[(ma - 1) % len(BANG_MAU)]
        r = random.Random(ma)
        nen = bytearray(RONG * CAO * 3)
        for y in range(CAO):
            for x in range(RONG):
                k = 0.35 + 0.65 * (0.5 + 0.5 * math.sin(2 * math.pi * (x / 40 + y / 60)))
                hat = r.randrange(24) if r.random() < 0.04 else 0
                i = (y * RONG + x) * 3
                for c in range(3):
                    nen[i + c] = min(255, int(mau[c] * k) // 16 * 16 + hat)
        self.nen = bytes(nen)
        # Khung chớp gần như trắng nhưng còn vân nền: khung I rơi vào lúc chớp vẫn có kích thước
        # như khung I thường (bảng tra bytes.translate đổi từng byte rất nhanh).
        self.nen_chop = self.nen.translate(bytes(255 - (255 - v) // 4 for v in range(256)))

    def lay_khung(self, so, ts_ms):
        chop = ts_ms % 1000 < 1000 / self.fps
        x = 6 + (ts_ms // 10) % (RONG - 24)               # ô vuông chạy 100 điểm ảnh/giây
        if chop:                                          # chớp trắng: v -> 255 − (255 − v)/4
            anh = bytearray(self.nen_chop)
            mau_chu, nen_chu = (0, 0, 0), (255, 255, 255)
        else:
            anh = bytearray(self.nen)
            mau_chu, nen_chu = (255, 255, 255), (20, 20, 20)
            to_mau(anh, RONG, x, 52, 12, 12, (255, 230, 0))
        dong_ho = f"T={ts_ms // 1000}.{ts_ms % 1000:03d}"
        to_mau(anh, RONG, 4, 4, 6 * 2 * len(dong_ho) + 4, 18, nen_chu)
        ve_chu(anh, RONG, CAO, 6, 6, dong_ho, 2, mau_chu)
        nhan = f"#{so:04d}"
        to_mau(anh, RONG, 4, CAO - 22, 6 * 2 * len(nhan) + 4, 18, nen_chu)
        ve_chu(anh, RONG, CAO, 6, CAO - 20, nhan, 2, mau_chu)
        to_mau(anh, RONG, RONG - 30, CAO - 15, 27, 11, (200, 0, 0))
        ve_chu(anh, RONG, CAO, RONG - 28, CAO - 13, "LIVE", 1)
        ve_chu(anh, RONG, CAO, RONG - 6 * 2 * len(self.ten[:3]) - 4, 6, self.ten[:3], 2, mau_chu)
        return anh


class NguonAmClapper:
    """Micro giả lập: âm 440 Hz biên độ thấp liên tục + tiếng "bíp" 1000 Hz dài 100 ms ở mỗi giây
    tròn của đồng hồ phương tiện (mẫu thứ n: n % 16000 < 1600). Mẫu đầu tiên = lúc bấm Phát."""

    def __init__(self, tan_so=440.0, bien_do=0.08, tan_so_bip=1000.0, bien_do_bip=0.4):
        self.n = 0
        self.w, self.a = 2 * math.pi * tan_so / TAN_SO_MAU, bien_do * 32767
        self.wb, self.ab = 2 * math.pi * tan_so_bip / TAN_SO_MAU, bien_do_bip * 32767

    def lay_khung(self):
        ra = []
        for n in range(self.n, self.n + MAU_MOI_KHUNG):
            x = self.a * math.sin(self.w * n)
            if n % TAN_SO_MAU < TAN_SO_MAU // 10:
                x += self.ab * math.sin(self.wb * n)
            ra.append(round(x))
        self.n += MAU_MOI_KHUNG
        return ra


# ================================================================ Kênh điều khiển JSON theo dòng
def gui_json(sock, obj):
    """Mỗi thông điệp là một dòng JSON UTF-8 kết thúc '\\n'."""
    sock.sendall((json.dumps(obj, ensure_ascii=False) + "\n").encode("utf-8"))


def doc_dong(tep):
    """Đọc một dòng JSON từ tệp nhị phân của socket (sock.makefile("rb")).
    Trả về dict, hoặc None khi kết nối đóng; dòng > 64 KB hoặc sai dạng -> ValueError."""
    dong = tep.readline(GIOI_HAN_DONG + 1)
    if not dong:
        return None
    if len(dong) > GIOI_HAN_DONG or not dong.endswith(b"\n"):
        raise ValueError("dòng điều khiển quá dài hoặc bị cắt")
    tin = json.loads(dong.decode("utf-8"))
    if not isinstance(tin, dict):
        raise ValueError("thông điệp không phải đối tượng JSON")
    return tin


def ten_hop_le(ten):
    return isinstance(ten, str) and 1 <= len(ten) <= 16 and ten.strip() == ten and ten.isprintable()


def doc_lenh_stdin(t0, xu_ly, ket_thuc=None):
    """Đọc lệnh từ stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ t0 (đồng hồ đơn điệu)."""
    for dong in sys.stdin:
        dong = dong.strip()
        if dong.startswith("@"):
            moc, _, dong = dong.partition(" ")
            time.sleep(max(0.0, float(moc[1:]) - (time.monotonic() - t0)))
            dong = dong.strip()
        if dong:
            xu_ly(dong)
    if ket_thuc:
        ket_thuc()
