"""Bài 7.3 – Client WebSocket tự viết bằng thư viện chuẩn (Python không có client WebSocket chuẩn).

Phía client của Mã giả 7.3 (gửi yêu cầu nâng cấp, kiểm tra Sec-WebSocket-Accept) và Mã giả 7.4
(đọc khung với from_client = FALSE). Mọi khung client gửi đều được che bằng khóa ngẫu nhiên 4 byte.
Đọc từng dòng từ bàn phím, gửi thành khung văn bản và in khung trả lời; cuối cùng gửi khung đóng 1000.

Chạy:  python3 ws_client.py [host] [port]      (mặc định 127.0.0.1 8080)
"""
import base64
import hashlib
import os
import socket
import struct
import sys

WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


def recv_exact(s, n):
    """Mã giả 3.4 – RECV_EXACT."""
    buf = b""
    while len(buf) < n:
        chunk = s.recv(n - len(buf))
        if not chunk:
            raise ConnectionError(f"Kết nối đóng khi mới nhận {len(buf)}/{n} byte")
        buf += chunk
    return buf


def ws_connect(host, port, path="/"):
    """Gửi yêu cầu nâng cấp (Sec-WebSocket-Key = Base64 của 16 byte ngẫu nhiên), kiểm tra phản hồi 101."""
    s = socket.create_connection((host, port), timeout=3)
    s.settimeout(5.0)
    key = base64.b64encode(os.urandom(16)).decode("ascii")
    s.sendall((f"GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\n"
               f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode("ascii"))
    head = b""
    while b"\r\n\r\n" not in head:                   # đọc từng byte để không lấy lố sang khung đầu tiên
        head += recv_exact(s, 1)
    lines = head.decode("ascii").split("\r\n")
    hdr = {k.strip().lower(): v.strip() for k, _, v in (l.partition(":") for l in lines[1:] if l)}
    expect = base64.b64encode(hashlib.sha1((key + WS_GUID).encode("ascii")).digest()).decode("ascii")
    if not lines[0].startswith("HTTP/1.1 101") or hdr.get("sec-websocket-accept") != expect:
        s.close()
        raise ConnectionError(f"Bắt tay thất bại: {lines[0]}")
    return s, lines[0]


def ws_send(s, opcode, data, mask=True):
    """Gửi một khung FIN = 1; client che dữ liệu bằng khóa 4 byte ngẫu nhiên."""
    n = len(data)
    b1 = 0x80 if mask else 0
    if n < 126:
        head = struct.pack("!BB", 0x80 | opcode, b1 | n)
    elif n < 65536:
        head = struct.pack("!BBH", 0x80 | opcode, b1 | 126, n)
    else:
        head = struct.pack("!BBQ", 0x80 | opcode, b1 | 127, n)
    if mask:
        key = os.urandom(4)
        data = bytes(d ^ key[i % 4] for i, d in enumerate(data))
        head += key
    s.sendall(head + data)


def ws_read_frame(s):
    """Mã giả 7.4 với from_client = FALSE: khung từ server không che."""
    b = recv_exact(s, 2)
    fin, opcode = (b[0] & 0x80) != 0, b[0] & 0x0F
    masked, n = (b[1] & 0x80) != 0, b[1] & 0x7F
    if n == 126:
        n = struct.unpack("!H", recv_exact(s, 2))[0]
    if n == 127:
        n = struct.unpack("!Q", recv_exact(s, 8))[0]
    mask = recv_exact(s, 4) if masked else None
    data = bytearray(recv_exact(s, n))
    if masked:
        for i in range(n):
            data[i] ^= mask[i % 4]
    return fin, opcode, bytes(data)


def close_code(data):
    return struct.unpack("!H", data[:2])[0] if len(data) >= 2 else 1005


def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 8080
    s, status = ws_connect(host, port)
    print(f"[client] Bắt tay thành công: {status}", flush=True)
    try:
        for line in sys.stdin:
            ws_send(s, 0x1, line.rstrip("\n").encode("utf-8"))
            _, opcode, data = ws_read_frame(s)
            if opcode == 0x8:
                print(f"[client] Server đóng kết nối, mã {close_code(data)}", flush=True)
                return
            print(f"[client] Nhận: '{data.decode('utf-8')}'", flush=True)
        ws_send(s, 0x8, struct.pack("!H", 1000))     # khung đóng, mã 1000: đóng bình thường
        _, opcode, data = ws_read_frame(s)
        print(f"[client] Server xác nhận đóng, mã {close_code(data)}", flush=True)
    finally:
        s.close()
        print("[client] Đã đóng kết nối", flush=True)


if __name__ == "__main__":
    main()
