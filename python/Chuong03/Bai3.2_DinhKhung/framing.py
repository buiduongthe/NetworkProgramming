"""Bài 3.2 – Thư viện định khung thông điệp trên dòng byte TCP.

Cài đặt Mã giả 3.3 (SEND_ALL, Mục 3.2.4), Mã giả 3.4 (RECV_EXACT), Mã giả 3.5 (READ_LINE)
và Mã giả 3.6 (SEND_FRAME, RECV_FRAME, SEND_JSON, RECV_JSON) – Mục 3.4.3 Giáo trình Lập trình mạng.
Khung = 4 byte độ dài (big-endian, không dấu) + nội dung; thông điệp JSON mã hóa UTF-8.
"""
import json
import struct

MAX_FRAME = 1048576                                  # 1 MiB


class ConnectionClosed(Exception):
    """Đối tác đóng kết nối; received = số byte đã nhận của khung (hoặc tệp) dở dang."""

    def __init__(self, received=0, message=None):
        super().__init__(message or f"Kết nối đóng giữa chừng (đã nhận {received} byte)")
        self.received = received


class ProtocolError(Exception):
    """Đối tác vi phạm định dạng giao thức (khung quá lớn, dòng quá dài...)."""


class JsonError(ValueError):
    """Nội dung khung không phải JSON hợp lệ mã hóa UTF-8."""


def send_all(s, data):
    """Mã giả 3.3: SEND có thể gửi ít hơn yêu cầu nên lặp đến khi hết (tương đương s.sendall)."""
    view = memoryview(data)
    sent = 0
    while sent < len(data):
        n = s.send(view[sent:])                      # có thể gửi ít hơn yêu cầu
        sent += n


def recv_exact(s, n):
    """Mã giả 3.4: nhận đúng n byte; báo ConnectionClosed kèm số byte đã nhận nếu đóng giữa chừng."""
    buf = bytearray()
    while len(buf) < n:
        chunk = s.recv(n - len(buf))
        if len(chunk) == 0:
            raise ConnectionClosed(received=len(buf))
        buf += chunk
    return bytes(buf)


class Reader:
    """Mã giả 3.5: STRUCT Reader { sock, pending } – bộ đệm tồn tại giữa các lần READ_LINE."""

    def __init__(self, sock):
        self.sock = sock
        self.pending = b""


def read_line(r, max_len):
    """Mã giả 3.5: đọc một dòng kết thúc bằng LF hoặc CRLF; trả None khi đối tác đã đóng."""
    while True:
        i = r.pending.find(b"\n")                    # vị trí '\n'
        if i > max_len:                              # dòng 5: đã thấy LF nhưng dòng quá dài
            raise ProtocolError("Dòng quá dài")
        if i >= 0:
            line = r.pending[:i]
            r.pending = r.pending[i + 1:]
            if line.endswith(b"\r"):                 # STRIP_CR
                line = line[:-1]
            return line.decode("utf-8")
        if len(r.pending) > max_len:
            raise ProtocolError("Dòng quá dài")
        chunk = r.sock.recv(4096)
        if len(chunk) == 0:
            return None                              # đối tác đã đóng
        r.pending += chunk


def send_frame(s, payload):
    """Mã giả 3.6: ghép 4 byte độ dài với nội dung rồi gửi một lần."""
    send_all(s, struct.pack("!I", len(payload)) + payload)     # UINT32_BE(LENGTH(payload)) + payload


def recv_frame(s):
    """Mã giả 3.6: nhận đủ tiêu đề, kiểm tra MAX_FRAME, nhận đúng n byte nội dung."""
    (n,) = struct.unpack("!I", recv_exact(s, 4))               # FROM_UINT32_BE
    if n > MAX_FRAME:
        raise ProtocolError(f"Khung quá lớn: {n}")
    try:
        return recv_exact(s, n)
    except ConnectionClosed as e:                              # đã nhận xong tiêu đề
        raise ConnectionClosed(received=4 + e.received)


def json_encode(obj):
    """JSON_ENCODE: giữ nguyên chữ tiếng Việt (không thoát \\uXXXX), không thêm khoảng trắng."""
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":"))


def json_decode(payload):
    """DECODE(…, "UTF-8") rồi JSON_DECODE; mọi lỗi giải mã đều quy về JsonError."""
    try:
        return json.loads(payload.decode("utf-8"))
    except ValueError as e:                          # UnicodeDecodeError, JSONDecodeError
        raise JsonError(str(e))


def send_json(s, obj):
    send_frame(s, json_encode(obj).encode("utf-8"))           # Mã giả 3.6, SEND_JSON


def recv_json(s):
    return json_decode(recv_frame(s))                          # Mã giả 3.6, RECV_JSON
