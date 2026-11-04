"""Bài 2.2 – Kết nối có thời gian chờ và phân loại lỗi (client).

Cài đặt Mã giả 2.2 SAFE_CONNECT (Mục 2.2.3, Giáo trình Lập trình mạng).
Lỗi được gom về một loại NetError với ba nhóm: NOT_FOUND, REFUSED (không nên thử lại)
và UNREACHABLE (hết thời gian chờ / không có đường đi, có thể thử lại).
Kế thừa Mã giả 2.1 (PARSE_ENDPOINT) để đọc địa chỉ, cổng.

Chạy:  python3 safe_connect.py [host] [port] [timeout_ms]   (mặc định 127.0.0.1 9000 3000)
"""
import errno
import os
import socket
import sys
import time


class ConfigError(Exception):
    """Lỗi cấu hình (Mã giả 2.1)."""


class NetError(Exception):
    """Lỗi mạng đã phân loại: kind ∈ {NOT_FOUND, REFUSED, UNREACHABLE}."""

    def __init__(self, kind, retry, cause):
        super().__init__(kind)
        self.kind, self.retry, self.cause = kind, retry, cause


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1 (rút gọn): tham số dòng lệnh > APP_HOST/APP_PORT > mặc định."""
    host = os.environ.get("APP_HOST", default_host)
    port = os.environ.get("APP_PORT", default_port)
    if len(args) >= 1:
        host = args[0]
    if len(args) >= 2:
        port = args[1]
    try:
        port = int(port)
    except ValueError:
        raise ConfigError(f"Cổng phải là số nguyên: {port}") from None
    if port < 1 or port > 65535:
        raise ConfigError(f"Cổng không hợp lệ: {port}")
    return host, port


def safe_connect(host, port, timeout_ms):
    """Mã giả 2.2 – SAFE_CONNECT(host, port, timeout_ms)."""
    try:                                                          # dòng 2–6: RESOLVE
        addrs = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    except socket.gaierror as e:                                  # tên miền không tồn tại
        raise NetError("NOT_FOUND", retry=False, cause=e) from None
    refused, last_error = 0, None                                 # dòng 7
    for family, socktype, proto, _, addr in addrs:                # dòng 8
        s = socket.socket(family, socktype, proto)                # dòng 9: SOCKET(FAMILY(a), STREAM)
        ok = False
        s.settimeout(timeout_ms / 1000)                           # dòng 10: CONNECT_TIMEOUT (giây)
        try:
            s.connect(addr)                                       # dòng 11–13: CONNECT
            ok = True
            return s                                              # thành công: trả socket
        except ConnectionRefusedError as e:                       # dòng 14–15: nhận gói RST
            refused += 1
            last_error = e
        except TimeoutError as e:                                 # dòng 16–17: Timeout
            last_error = e
        except OSError as e:
            if e.errno not in (errno.ENETUNREACH, errno.EHOSTUNREACH):
                raise                                             # lỗi khác: chuyển lên nơi gọi
            last_error = e                                        # NetworkUnreachable, HostUnreachable
        finally:                                                  # dòng 18–20
            if not ok:
                s.close()                                         # chỉ đóng khi thất bại
    if refused > 0 and refused == len(addrs):                     # dòng 22–24
        raise NetError("REFUSED", retry=False, cause=last_error)
    raise NetError("UNREACHABLE", retry=True, cause=last_error)   # dòng 25


def main():
    args = sys.argv[1:]
    try:
        host, port = parse_endpoint(args, "127.0.0.1", 9000)
        timeout_ms = int(args[2]) if len(args) >= 3 else 3000
    except (ConfigError, ValueError) as e:
        sys.exit(f"[client] Lỗi cấu hình: {e}")
    print(f"[client] Kết nối tới {host}:{port}, thời gian chờ {timeout_ms} ms", flush=True)
    t0 = time.perf_counter()
    try:
        s = safe_connect(host, port, timeout_ms)
    except NetError as e:
        ms = (time.perf_counter() - t0) * 1000
        print(f"[client] NetError {e.kind}, thử lại: {'có' if e.retry else 'không'}, sau {ms:.0f} ms", flush=True)
        print(f"[client] Nguyên nhân: {type(e.cause).__name__}: {e.cause}", flush=True)
        sys.exit(1)
    ms = (time.perf_counter() - t0) * 1000
    with s:                                                       # nơi gọi chịu trách nhiệm đóng socket
        local, peer = s.getsockname(), s.getpeername()
        print(f"[client] Thành công: {local[0]}:{local[1]} -> {peer[0]}:{peer[1]}, sau {ms:.0f} ms", flush=True)
    print("[client] Đã đóng kết nối", flush=True)


if __name__ == "__main__":
    main()
