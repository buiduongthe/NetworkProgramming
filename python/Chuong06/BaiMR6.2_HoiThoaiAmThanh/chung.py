"""Bài mở rộng MR6.2 – Các thủ tục dùng chung cho server và client hội thoại âm thanh.

Âm thanh số PCM (Pulse-Code Modulation) 16 bit, mono, tần số lấy mẫu 16000 Hz:
  - lấy mẫu: mỗi giây đo biên độ sóng âm 16000 lần; mỗi mẫu là số nguyên 16 bit có dấu
    (-32768 … 32767). Theo định lý Nyquist, 16 kHz biểu diễn được âm tới 8 kHz – đủ cho giọng nói;
  - tốc độ bit: 16000 mẫu/s × 16 bit = 256000 bit/s = 256 kbit/s (32000 byte/s);
  - khung 20 ms: 16000 × 0,020 = 320 mẫu = 640 byte -> 50 gói mỗi giây;
  - mỗi gói thêm 12 byte RTP + 8 byte UDP + 20 byte IPv4 = 40 byte phụ phí
    -> 50 × 40 × 8 = 16 kbit/s, tổng ≈ 272 kbit/s mỗi chiều cho mỗi client.

Mức âm đo bằng dBFS (decibel so với toàn thang – full scale): 0 dBFS là mức RMS lớn nhất
(32768), mức âm hơn là nhỏ hơn. Ví dụ sóng sin biên độ 0,25 toàn thang có RMS 0,25/√2 -> -15,1 dBFS;
nhân biên độ với 2 -> +6,02 dB, chia 2 -> -6,02 dB.
"""
import array
import json
import math
import struct
import sys
import wave

TAN_SO_MAU = 16000                               # mẫu/giây
KHUNG_MS = 20                                    # độ dài một khung (gói) âm thanh
MAU_MOI_KHUNG = TAN_SO_MAU * KHUNG_MS // 1000    # 320 mẫu
BYTE_MOI_KHUNG = MAU_MOI_KHUNG * 2               # 640 byte
PT_AM_THANH = 96          # payload type động (RFC 3551) dùng cho L16/16000/mono
PT_GIU_KET_NOI = 127      # gói rỗng: đăng ký địa chỉ UDP, giữ kết nối khi mic tắt
SSRC_SERVER = 0           # mã nguồn của luồng trộn do server gửi


# ---------------------------------------------------------------- Gói RTP (RFC 3550)
def dong_goi_rtp(pt, seq, ts, ssrc, payload=b""):
    """Header RTP cố định 12 byte: V=2|P|X|CC (1 byte), M|PT (1 byte), số thứ tự (16 bit),
    nhãn thời gian (32 bit, đơn vị: mẫu), SSRC (32 bit, ở đây là mã client)."""
    return struct.pack("!BBHII", 0x80, pt & 0x7F, seq & 0xFFFF, ts & 0xFFFFFFFF, ssrc) + payload


def tach_rtp(goi):
    """Trả về (pt, seq, ts, ssrc, payload) hoặc None nếu gói sai dạng."""
    if len(goi) < 12:
        return None
    b0, b1, seq, ts, ssrc = struct.unpack("!BBHII", goi[:12])
    if b0 >> 6 != 2:                             # phiên bản RTP phải là 2
        return None
    return b1 & 0x7F, seq, ts, ssrc, goi[12:]


# ---------------------------------------------------------------- PCM <-> byte
def pcm_sang_byte(mau):
    """RTP truyền L16 theo thứ tự byte mạng (big-endian) – RFC 3551 mục 4.5.11."""
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


def cat_ngon(x):
    """Cắt ngọn (clipping): giá trị vượt phạm vi 16 bit bị giữ ở biên."""
    return 32767 if x > 32767 else (-32768 if x < -32768 else x)


# ---------------------------------------------------------------- Mức âm
def muc_dbfs(mau):
    """Mức RMS (căn bậc hai của trung bình bình phương) tính theo dBFS."""
    if not mau:
        return -math.inf
    rms = math.sqrt(sum(x * x for x in mau) / len(mau))
    return 20 * math.log10(rms / 32768) if rms > 0 else -math.inf


def thanh_muc(db, rong=16):
    """Thanh đo mức dạng chữ: -60 dBFS (trống) … 0 dBFS (đầy)."""
    n = 0 if db == -math.inf else max(0, min(rong, round((db + 60) / 60 * rong)))
    return "|" + "#" * n + "-" * (rong - n) + "|"


def chu_db(db, rong=5):
    return "---".rjust(rong) if db < -90 else f"{db:{rong}.1f}"


# ---------------------------------------------------------------- Bộ đệm jitter
class BoDemJitter:
    """Bộ đệm chống rung trễ (jitter buffer) cho một luồng RTP.

    Gói UDP đến không đều (jitter), có thể mất hoặc đảo thứ tự. Bộ đệm giữ vài khung
    (do_sau) trước khi bắt đầu phát, rồi cứ mỗi 20 ms lấy đúng khung có số thứ tự kế tiếp:
      - có khung      -> phát;
      - thiếu khung nhưng đã có khung sau -> coi như mất, phát khoảng lặng (che lỗi đơn giản);
      - bộ đệm rỗng   -> cạn (người gửi ngừng gửi hoặc mạng nghẽn), chờ nạp lại do_sau khung;
      - khung đến sau khi đã qua lượt phát -> trễ, bỏ.
    Đồng thời thống kê theo RFC 3550: số gói mất (từ khoảng trống số thứ tự) và jitter
    (trung bình trượt 1/16 của chênh lệch thời gian truyền giữa hai gói liên tiếp).
    """

    def __init__(self, do_sau=3, toi_da=50):
        self.do_sau, self.toi_da = do_sau, toi_da
        self.khung = {}                  # số thứ tự mở rộng -> payload
        self.ke_tiep = None              # số thứ tự sẽ phát tiếp
        self.dang_phat = False
        self.seq_lon_nhat = None         # số thứ tự mở rộng (không quay vòng) lớn nhất đã nhận
        self.seq_dau = None
        self.nhan = self.tre = self.thieu = self.can = self.tran = 0
        self.jitter = 0.0                # đơn vị: mẫu (1 ms = 16 mẫu)
        self._truyen_truoc = None

    def _mo_rong(self, seq):
        """Số thứ tự 16 bit quay vòng sau 65535; đổi thành số nguyên tăng mãi."""
        if self.seq_lon_nhat is None:
            return seq
        lech = (seq - self.seq_lon_nhat) & 0xFFFF
        return self.seq_lon_nhat + lech if lech < 0x8000 else self.seq_lon_nhat - (0x10000 - lech)

    def nhan_goi(self, seq, ts, payload, luc_den):
        s = self._mo_rong(seq)
        if self.seq_dau is None:
            self.seq_dau = s
        if self.seq_lon_nhat is None or s > self.seq_lon_nhat:
            self.seq_lon_nhat = s
        self.nhan += 1
        truyen = luc_den * TAN_SO_MAU - ts         # thời gian truyền (lệch hằng số không quan trọng)
        if self._truyen_truoc is not None:
            d = abs(truyen - self._truyen_truoc)
            if d < TAN_SO_MAU:                     # bỏ qua bước nhảy lớn (nhãn thời gian quay vòng)
                self.jitter += (d - self.jitter) / 16
        self._truyen_truoc = truyen
        if self.dang_phat and s < self.ke_tiep:
            self.tre += 1                          # đến quá muộn, đã qua lượt phát
            return
        self.khung[s] = payload
        if len(self.khung) > self.toi_da:          # tràn: bỏ khung cũ nhất
            del self.khung[min(self.khung)]
            self.tran += 1

    def lay(self):
        """Gọi mỗi 20 ms; trả về payload của khung cần phát hoặc None (phát khoảng lặng)."""
        if not self.dang_phat:
            if len(self.khung) < self.do_sau:
                return None
            self.dang_phat, self.ke_tiep = True, min(self.khung)
        if self.ke_tiep in self.khung:
            p = self.khung.pop(self.ke_tiep)
            self.ke_tiep += 1
            return p
        if not self.khung:                         # cạn bộ đệm: chờ nạp lại
            self.dang_phat = False
            self.can += 1
            return None
        self.thieu += 1                            # mất khung này, đã có khung sau
        self.ke_tiep += 1
        return None

    def xoa(self):
        self.khung.clear()
        self.dang_phat = False

    @property
    def mat(self):
        """Số gói mất theo RFC 3550: số gói mong đợi (từ số thứ tự) trừ số gói đã nhận."""
        if self.seq_dau is None:
            return 0
        return max(0, self.seq_lon_nhat - self.seq_dau + 1 - self.nhan)

    @property
    def jitter_ms(self):
        return self.jitter / (TAN_SO_MAU / 1000)


# ---------------------------------------------------------------- Phân tích tần số
def goertzel_bien_do(mau, tan_so, fs=TAN_SO_MAU):
    """Biên độ của thành phần tần số `tan_so` trong một khối mẫu, bằng thuật toán Goertzel.

    Goertzel tính một điểm của biến đổi Fourier rời rạc bằng một bộ lọc cộng hưởng bậc hai:
        s[n] = x[n] + 2cos(ω)·s[n-1] - s[n-2],  ω = 2π·f/fs
    sau N mẫu: |X|² = s1² + s2² - 2cos(ω)·s1·s2. Tín hiệu được nhân trước với cửa sổ Hann để
    giảm rò phổ từ các âm khác; biên độ sin A ≈ 2|X| / Σw.
    """
    n = len(mau)
    w = 2 * math.pi * tan_so / fs
    he_so = 2 * math.cos(w)
    s1 = s2 = tong_w = 0.0
    k = 2 * math.pi / (n - 1)
    for i, x in enumerate(mau):
        cs = 0.5 - 0.5 * math.cos(k * i)             # cửa sổ Hann
        tong_w += cs
        s1, s2 = x * cs + he_so * s1 - s2, s1
    return 2 * math.sqrt(max(s1 * s1 + s2 * s2 - he_so * s1 * s2, 0.0)) / tong_w


def muc_am_dbfs(mau, tan_so, khoi=1600):
    """Mức RMS (dBFS) của một âm trong đoạn dài: chia khối 0,1 s (độ phân giải 10 Hz), đo biên độ
    từng khối rồi lấy trung bình công suất. Đo theo khối ngắn để không bị sai khi pha của âm bị
    nhảy giữa đoạn (bộ đệm jitter cạn rồi nạp lại làm âm đến chậm thêm vài khung)."""
    ds = [goertzel_bien_do(mau[i:i + khoi], tan_so) for i in range(0, len(mau) - khoi + 1, khoi)]
    if not ds:
        return -math.inf
    rms = math.sqrt(sum(a * a for a in ds) / len(ds)) / math.sqrt(2)   # biên độ sin -> RMS
    return 20 * math.log10(rms / 32768) if rms > 0.5 else -math.inf


# ---------------------------------------------------------------- Tệp WAV
def ghi_wav(duong_dan, mau):
    """WAV (RIFF) lưu mẫu theo little-endian, khác thứ tự byte mạng của RTP."""
    with wave.open(duong_dan, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(TAN_SO_MAU)
        a = array.array("h", mau)
        if sys.byteorder == "big":
            a.byteswap()
        w.writeframes(a.tobytes())


def doc_wav(duong_dan):
    """Đọc WAV PCM 16 bit mono 16000 Hz; trả về array('h')."""
    with wave.open(duong_dan, "rb") as w:
        if (w.getnchannels(), w.getsampwidth(), w.getframerate()) != (1, 2, TAN_SO_MAU):
            raise ValueError("cần WAV PCM 16 bit, mono, 16000 Hz")
        a = array.array("h")
        a.frombytes(w.readframes(w.getnframes()))
    if sys.byteorder == "big":
        a.byteswap()
    return a


# ---------------------------------------------------------------- Kênh điều khiển JSON
def gui_json(sock, obj):
    """Mỗi thông điệp điều khiển là một dòng JSON kết thúc bằng '\\n' (định khung theo dòng)."""
    sock.sendall((json.dumps(obj, ensure_ascii=False) + "\n").encode("utf-8"))
