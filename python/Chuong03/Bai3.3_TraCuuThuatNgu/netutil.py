"""Tiện ích kết nối kế thừa từ Chương 2, chép gọn vào bài để bài tự chứa.

- parse_endpoint: Mã giả 2.1 (PARSE_ENDPOINT, Mục 2.1.3)
- safe_connect:   Mã giả 2.2 (SAFE_CONNECT, Mục 2.2)
"""
import os
import socket
import sys


class NetError(Exception):
    """Lỗi mạng đã phân loại: kind ∈ {NOT_FOUND, REFUSED, UNREACHABLE}, retry = có nên thử lại."""

    def __init__(self, kind, retry, cause):
        super().__init__(f"{kind} (thử lại: {'có' if retry else 'không'}): {cause}")
        self.kind, self.retry, self.cause = kind, retry, cause


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > giá trị mặc định."""
    host = os.environ.get("APP_HOST", default_host)
    port = os.environ.get("APP_PORT", default_port)
    if len(args) >= 1:
        host = args[0]
    if len(args) >= 2:
        port = args[1]
    try:
        port = int(port)                                   # TO_INT
    except ValueError:
        sys.exit(f"Cổng phải là số nguyên: {port}")        # ConfigError
    if not 1 <= port <= 65535:
        sys.exit(f"Cổng không hợp lệ: {port}")
    return host, port


def safe_connect(host, port, timeout_ms):
    """Mã giả 2.2: thử lần lượt các địa chỉ, có thời gian chờ, phân loại lỗi."""
    try:
        addrs = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)   # RESOLVE
    except socket.gaierror as e:
        raise NetError("NOT_FOUND", False, e)
    refused, last_error = 0, None
    for family, socktype, proto, _, addr in addrs:
        s = socket.socket(family, socktype, proto)
        s.settimeout(timeout_ms / 1000)                    # CONNECT_TIMEOUT
        try:
            s.connect(addr)                                # CONNECT: bắt tay ba bước
            return s
        except ConnectionRefusedError as e:                # nhận RST
            refused += 1
            last_error = e
            s.close()
        except OSError as e:                               # Timeout, Unreachable
            last_error = e
            s.close()
    if refused > 0 and refused == len(addrs):
        raise NetError("REFUSED", False, last_error)
    raise NetError("UNREACHABLE", True, last_error)
