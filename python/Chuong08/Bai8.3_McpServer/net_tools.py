"""Bài 8.3 – Thân ba công cụ mạng và lớp kiểm soát đích của MCP server.

- Mã giả 1.1 RESOLVE_HOST, 1.2 PORT_CHECK, 7.8 PING: chép gọn, PRINT đổi thành LOG ra stderr
  (mọi dòng in ra stdout sẽ chen vào luồng thông điệp MCP – Mục 8.3.5).
- Mã giả 8.7 VALIDATE_NAME, VALIDATE_TARGET, VALIDATE_PORT (Mục 8.5.4): đích phải nằm trong ALLOWED_NETS.
Công cụ dùng socket API, không gọi shell. Tệp được mcp_server.py dùng; không chạy riêng.
"""
import ipaddress
import os
import re
import socket
import struct
import sys
import time


class NetError(Exception):
    """Lỗi khi thực thi công cụ (máy không phản hồi...): trả về mô hình với isError = true."""


class PolicyError(Exception):
    """Đầu vào bị chính sách từ chối: cũng trả về với isError = true."""


class ResolveError(Exception):
    """Không phân giải được tên máy (VALIDATE_TARGET): trả về với isError = true."""


def log(*parts):
    print("[mcp]", *parts, file=sys.stderr, flush=True)       # nhật ký: chỉ ghi ra STDERR


def family_name(f):
    return {socket.AF_INET: "INET", socket.AF_INET6: "INET6"}.get(f, str(f))


def resolve_host(name):
    """Mã giả 1.1 – RESOLVE_HOST(name), PRINT -> LOG(stderr); lỗi phân giải -> danh sách rỗng."""
    try:
        infos = socket.getaddrinfo(name, None, type=socket.SOCK_STREAM)
        addrs = [(fam, sa[0]) for fam, _, _, _, sa in infos]
        for fam, a in addrs:
            log("  resolve:", family_name(fam), a)
        return addrs
    except socket.gaierror as e:
        log("  resolve: Không phân giải được", name, e.errno)
        return []


def port_check(host, port, timeout_ms):
    """Mã giả 1.2 – PORT_CHECK(host, port, timeout_ms) -> OPEN | CLOSED | FILTERED | UNRESOLVED."""
    addrs = resolve_host(host)
    if not addrs:
        return "UNRESOLVED"
    state = "FILTERED"
    for fam, a in addrs:
        s = socket.socket(fam, socket.SOCK_STREAM)
        s.settimeout(timeout_ms / 1000)                        # CONNECT_TIMEOUT
        try:
            s.connect((a, port))
            return "OPEN"
        except ConnectionRefusedError:
            state = "CLOSED"                                   # nhận gói RST
        except OSError:                                        # Timeout, NetworkUnreachable, HostUnreachable
            continue
        finally:
            s.close()
    return state


def inet_checksum(b):
    """Mã giả 7.8 – INET_CHECKSUM (RFC 1071)."""
    if len(b) % 2 == 1:
        b += b"\x00"
    total = sum(struct.unpack(f"!{len(b) // 2}H", b))
    while total > 0xFFFF:
        total = (total & 0xFFFF) + (total >> 16)
    return ~total & 0xFFFF


def ping(dst, seq, timeout_ms):
    """Mã giả 7.8 – PING(host, seq, timeout_ms) bằng socket RAW (cần quyền quản trị)."""
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_RAW, socket.IPPROTO_ICMP)
    except PermissionError:
        raise NetError("Không mở được socket RAW: cần quyền quản trị")
    ident = os.getpid() & 0xFFFF
    p = struct.pack("!BBHHH", 8, 0, 0, ident, seq) + os.urandom(32)
    p = p[:2] + struct.pack("!H", inet_checksum(p)) + p[4:]
    try:
        t0 = time.perf_counter()
        s.sendto(p, (dst, 0))
        deadline = time.monotonic() + timeout_ms / 1000        # Mã giả 7.8 dòng 21: hạn chót chung
        while True:                                            # bỏ qua gói ICMP khác
            s.settimeout(max(0.001, deadline - time.monotonic()))   # dòng 23: chờ phần còn lại
            d, addr = s.recvfrom(1500)                         # RAW: có cả tiêu đề IP
            ihl = (d[0] & 0x0F) * 4
            m = d[ihl:]
            if m[0] == 0 and struct.unpack("!HH", m[4:8]) == (ident, seq):
                return addr[0], (time.perf_counter() - t0) * 1000, d[8]   # (địa chỉ, RTT ms, TTL)
    except socket.timeout:                                     # Timeout
        raise TimeoutError(f"Không nhận được Echo Reply trong {timeout_ms} ms") from None
    finally:
        s.close()


# ---------------- Mã giả 8.7 – Kiểm soát đích của công cụ mạng ----------------
def parse_cidrs(text):
    """PARSE_CIDRS: danh sách CIDR phân cách bằng dấu phẩy; biến môi trường vắng -> danh sách rỗng."""
    return [ipaddress.ip_network(x.strip()) for x in (text or "").split(",") if x.strip()]


ALLOWED_NETS = parse_cidrs(os.environ.get("ALLOWED_NETS"))


def is_ip_address(host):
    """IS_IP_ADDRESS: địa chỉ IPv4 hoặc IPv6 viết bằng số (ipaddress.ip_address)."""
    try:
        ipaddress.ip_address(host)
        return True
    except ValueError:
        return False


def validate_name(host):
    if isinstance(host, str) and is_ip_address(host):         # dòng 3: kể cả IPv6 "::1"
        return host
    if not isinstance(host, str) or len(host) > 253 or not re.fullmatch(r"[A-Za-z0-9.-]+", host):
        raise PolicyError("Tên máy không hợp lệ")
    return host


def validate_target(host):
    try:
        infos = socket.getaddrinfo(validate_name(host), None, type=socket.SOCK_STREAM)   # RESOLVE
    except socket.gaierror:
        raise ResolveError(f"Không phân giải được {host}") from None
    addrs = [sa[0] for _, _, _, _, sa in infos]
    for a in addrs:
        if not any(ipaddress.ip_address(a) in net for net in ALLOWED_NETS):   # IN_ANY
            raise PolicyError("Ngoài phạm vi cho phép: " + a)
    return addrs[0]                        # dùng đúng địa chỉ đã kiểm tra (chống DNS rebinding)


def validate_port(p):
    if not isinstance(p, int) or isinstance(p, bool) or p < 1 or p > 65535:
        raise PolicyError("Cổng không hợp lệ")
    return p
