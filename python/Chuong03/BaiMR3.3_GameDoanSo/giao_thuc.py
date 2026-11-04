"""Bài mở rộng MR3.3 – Giao thức trò chơi đoán số: khung JSON có tiền tố độ dài, bộ sinh số giả ngẫu nhiên.

Kế thừa Bài 3.2 (Mã giả 3.3–3.6): khung = 4 byte độ dài (big-endian) + nội dung JSON mã hóa UTF-8.
Các thông điệp (mỗi thông điệp là một đối tượng JSON có trường "type"):
  client -> server: hello {name}, guess {value}, bye
  server -> client: welcome, start, turn, result, timeout, left, error {code, msg}, game_over, bye
"""
import json
import struct

MAX_FRAME = 64 * 1024          # thông điệp trò chơi rất nhỏ: giới hạn 64 KiB chống khung khổng lồ


class ConnectionClosed(Exception):
    """Đối tác đóng kết nối."""


class ProtocolError(Exception):
    """Đối tác vi phạm định dạng khung (khung quá lớn...)."""


def recv_exact(s, n):
    """Mã giả 3.4 (RECV_EXACT): nhận đúng n byte."""
    buf = bytearray()
    while len(buf) < n:
        chunk = s.recv(n - len(buf))
        if not chunk:
            raise ConnectionClosed()
        buf += chunk
    return bytes(buf)


def send_frame(s, payload):
    """Mã giả 3.6 (SEND_FRAME): 4 byte độ dài + nội dung, gửi hết bằng sendall (SEND_ALL)."""
    s.sendall(struct.pack("!I", len(payload)) + payload)


def recv_frame(s):
    """Mã giả 3.6 (RECV_FRAME): đọc tiêu đề, kiểm tra giới hạn, đọc đúng n byte."""
    (n,) = struct.unpack("!I", recv_exact(s, 4))
    if n > MAX_FRAME:
        raise ProtocolError(f"Khung quá lớn: {n} byte")
    return recv_exact(s, n)


def send_json(s, obj):
    send_frame(s, json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))


def decode_json(payload):
    """Giải mã nội dung khung; trả None nếu không phải đối tượng JSON hợp lệ."""
    try:
        obj = json.loads(payload.decode("utf-8"))
    except ValueError:                       # UnicodeDecodeError, JSONDecodeError
        return None
    return obj if isinstance(obj, dict) else None


def recv_json(s):
    return decode_json(recv_frame(s))


class Lcg:
    """Bộ sinh số giả ngẫu nhiên đồng dư tuyến tính (LCG) rất đơn giản.

    Bản Python và bản Go dùng cùng công thức nên với cùng hạt giống (seed) cho cùng dãy số,
    nhờ đó kết quả demo của hai bản trùng nhau. Không dùng cho mục đích bảo mật.
    """

    def __init__(self, seed):
        self.x = seed & 0x7FFFFFFF

    def next_int(self, lo, hi):
        """Số nguyên ngẫu nhiên trong [lo, hi]."""
        self.x = (self.x * 1103515245 + 12345) & 0x7FFFFFFF
        return lo + (self.x >> 8) % (hi - lo + 1)
