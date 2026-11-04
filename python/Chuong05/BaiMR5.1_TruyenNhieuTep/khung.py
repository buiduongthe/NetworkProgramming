"""Bài mở rộng MR5.1 – Khung giao thức dùng chung: định khung JSON, thùng token, băm SHA-256.

Giao thức (một kết nối TCP cho mỗi lượt truyền):
  - Một khung điều khiển JSON có tiền tố độ dài 4 byte (big-endian) mở đầu.
  - Sau đó là luồng byte nhị phân của tệp (từ offset tới hết) để hỗ trợ tải tiếp (resume).
Chép gọn ý tưởng định khung từ Mã giả 5.1 (Bài 5.1) và băm SHA-256 từ Bài 3.9/MR7.1.
"""
import hashlib
import json
import struct
import threading
import time

CHUNK = 32 * 1024                 # cỡ mỗi lần đọc/ghi
KHONG_GIOI_HAN = 1 << 40          # "không giới hạn" tốc độ (1 TiB/s) – dùng cho mặc định


def gui_khung(sock, obj):
    """Gửi một khung điều khiển JSON: 4 byte độ dài + nội dung UTF-8."""
    data = json.dumps(obj, ensure_ascii=False).encode("utf-8")
    sock.sendall(struct.pack("!I", len(data)) + data)


def doc_du(sock, n):
    """Đọc đúng n byte (hoặc báo lỗi nếu kết nối đóng giữa chừng)."""
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("Kết nối đóng khi đang đọc")
        buf += chunk
    return buf


def nhan_khung(sock):
    """Nhận một khung điều khiển JSON."""
    (n,) = struct.unpack("!I", doc_du(sock, 4))
    return json.loads(doc_du(sock, n).decode("utf-8"))


def sha256_tep(path, offset=0, length=None):
    """Băm SHA-256 của tệp (hoặc một phần từ offset)."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        f.seek(offset)
        con = length
        while True:
            want = CHUNK if con is None else min(CHUNK, con)
            if want <= 0:
                break
            b = f.read(want)
            if not b:
                break
            h.update(b)
            if con is not None:
                con -= len(b)
    return h.hexdigest()


class ThungToken:
    """Thùng token giới hạn tốc độ (byte/giây), đổ đầy liên tục; có thể đổi tốc độ lúc chạy.

    Dùng chung cho mức phiên (session) và mức từng tệp. `tieu_thu(n)` chặn tới khi đủ n token.
    """

    def __init__(self, rate):
        self.rate = float(rate)
        self.suc_chua = float(rate)           # cho phép bùng một giây
        self.token = min(float(rate), CHUNK * 4)   # bùng khởi động nhỏ -> tốc độ đo sát giới hạn
        self.moc = time.monotonic()
        self.khoa = threading.Lock()

    def _do_day(self):
        now = time.monotonic()
        self.token = min(self.suc_chua, self.token + (now - self.moc) * self.rate)
        self.moc = now

    def dat_rate(self, rate):
        with self.khoa:
            self._do_day()
            self.rate = float(rate)
            self.suc_chua = float(rate)               # sức chứa = 1 giây ở tốc độ mới
            # Giới hạn "bùng" còn 0,25 giây để nhiều tệp chia sẻ đều hơn, tốc độ đo sát giới hạn phiên.
            self.token = min(self.token, rate * 0.25)

    def tieu_thu(self, n, dung_khi):
        """Chặn tới khi lấy được n token. `dung_khi()` trả True thì bỏ dở (khi pause/stop)."""
        while not dung_khi():
            with self.khoa:
                self._do_day()
                if self.token >= n:
                    self.token -= n
                    return True
                thieu = (n - self.token) / self.rate
            time.sleep(min(thieu, 0.05))
        return False


def doc_so_byte(s):
    """Phân tích chuỗi tốc độ: '512K', '1M', '2MiB', '1048576' -> số byte/giây."""
    s = s.strip().upper().replace("IB", "").replace("B", "")
    nhan = 1
    if s.endswith("K"):
        nhan, s = 1024, s[:-1]
    elif s.endswith("M"):
        nhan, s = 1024 * 1024, s[:-1]
    elif s.endswith("G"):
        nhan, s = 1024 * 1024 * 1024, s[:-1]
    return int(float(s) * nhan)


def dinh_dang_byte(n):
    """Số byte -> chuỗi dễ đọc (KiB/MiB)."""
    for don_vi, chia in (("MiB", 1024 * 1024), ("KiB", 1024)):
        if n >= chia:
            return f"{n / chia:.2f} {don_vi}"
    return f"{n} B"
