"""Bài 6.3 – Các thủ tục kế thừa dùng chung (chép gọn từ Chương 2 và Chương 3).

- parse_endpoint: Mã giả 2.1 (Mục 2.1.3)
- safe_connect:   Mã giả 2.2 (rút gọn, Mục 2.2)
- recv_exact:     Mã giả 3.4 (Mục 3.4.3)
"""
import os
import socket
import sys


class ConnectionClosed(Exception):
    """Đối tác đóng kết nối; received = số byte đã nhận."""
    def __init__(self, received):
        super().__init__(f"Kết nối bị đóng sau {received} byte")
        self.received = received


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > mặc định."""
    host = os.environ.get("APP_HOST", default_host)
    port = os.environ.get("APP_PORT", default_port)
    if len(args) >= 1:
        host = args[0]
    if len(args) >= 2:
        port = args[1]
    try:
        port = int(port)
    except ValueError:
        sys.exit(f"Cổng phải là số nguyên: {port}")
    if not 1 <= port <= 65535:
        sys.exit(f"Cổng không hợp lệ: {port}")
    return host, port


def int_arg(args, i, default):
    """Đọc tham số nguyên thứ i (nếu có), ngược lại dùng giá trị mặc định."""
    return int(args[i]) if len(args) > i else default


def safe_connect(host, port, timeout_ms):
    """Mã giả 2.2 (rút gọn): thử lần lượt các địa chỉ, có thời gian chờ kết nối."""
    last_error = None
    for family, socktype, proto, _, addr in socket.getaddrinfo(host, port, type=socket.SOCK_STREAM):
        s = socket.socket(family, socktype, proto)
        s.settimeout(timeout_ms / 1000)
        try:
            s.connect(addr)
            return s
        except OSError as e:
            last_error = e
            s.close()
    raise ConnectionError(f"Không kết nối được {host}:{port}: {last_error}")


def recv_exact(s, n):
    """Mã giả 3.4: gọi RECV lặp lại đến khi đủ n byte."""
    buf = bytearray()
    while len(buf) < n:
        chunk = s.recv(min(n - len(buf), 65536))
        if not chunk:
            raise ConnectionClosed(len(buf))
        buf += chunk
    return bytes(buf)
