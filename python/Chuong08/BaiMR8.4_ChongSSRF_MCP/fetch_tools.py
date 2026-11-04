"""Bài mở rộng MR8.4 – Thân công cụ fetch_url: bản NGÂY THƠ và bản AN TOÀN (chống SSRF).

Dùng bởi mcp_server_ssrf.py. SSRF (Server-Side Request Forgery) là lỗ hổng khi server nhận một URL
từ bên ngoài rồi tự đi tải URL đó: kẻ tấn công ép server truy cập dịch vụ nội bộ (máy chủ siêu dữ liệu
đám mây 169.254.169.254, dịch vụ chỉ nghe loopback, mạng riêng 10/8…) mà client bên ngoài không với tới.

Hai bộ bảng được "tiêm" (inject) qua biến môi trường để demo chạy hoàn toàn offline, tất định:
  SSRF_RESOLVER = {"tên miền": ["ip1", "ip2", ...]}  – bảng phân giải tên giả lập; nếu danh sách có
                  nhiều IP thì mỗi lần gọi trả IP kế tiếp (mô phỏng DNS rebinding / TOCTOU).
  SSRF_DIAL     = {"ip": cổng}                        – nối tới 127.0.0.1:cổng thay cho ip thật
                  (để một "địa chỉ công khai" 203.0.113.x hay 169.254.169.254 trỏ về server cục bộ).
Hai bảng này CHỈ tồn tại cho bản demo offline; thực tế dùng DNS thật và nối thẳng tới IP đã kiểm tra.
"""
import ipaddress
import json
import os
import socket
import sys
import time
import urllib.parse

# ---- Dải địa chỉ BỊ CHẶN (liệt kê tường minh, KHÔNG dựa vào is_private vì dải tài liệu cũng là private) ----
BLOCKED_NETS = [ipaddress.ip_network(x) for x in [
    "0.0.0.0/8",        # địa chỉ "không xác định" / mạng này
    "127.0.0.0/8",      # loopback
    "10.0.0.0/8",       # RFC 1918
    "172.16.0.0/12",    # RFC 1918
    "192.168.0.0/16",   # RFC 1918
    "169.254.0.0/16",   # link-local, gồm 169.254.169.254 (siêu dữ liệu đám mây)
    "::1/128",          # loopback IPv6
    "::/128",           # không xác định IPv6
    "fc00::/7",         # IPv6 unique-local
    "fe80::/10",        # IPv6 link-local
]]
MAX_REDIRECT = 5
MAX_BYTES = 64 * 1024          # giới hạn kích thước phản hồi
HAN_GIAY = 5.0                 # giới hạn tổng thời gian một lần fetch


class SsrfError(Exception):
    """URL bị chính sách an toàn từ chối (trả về mô hình với isError = true)."""


class FetchError(Exception):
    """Lỗi khi thực thi (không nối được, phản hồi hỏng…)."""


# -------------------- Bảng phân giải và bảng "dial" được tiêm (chỉ cho demo) --------------------
_RESOLVER = {k: list(v) for k, v in json.loads(os.environ.get("SSRF_RESOLVER", "{}")).items()}
_DIAL = {k: int(v) for k, v in json.loads(os.environ.get("SSRF_DIAL", "{}")).items()}


def resolve_once(host):
    """Phân giải tên -> một địa chỉ IP (chuỗi). Nếu host đã là IP thì trả nguyên.

    Với bảng giả lập: mỗi lần gọi trả IP kế tiếp trong danh sách (IP cuối "dính"), nhờ đó có thể
    mô phỏng DNS rebinding: lần kiểm tra trả IP công khai, lần nối thật trả IP nội bộ.
    """
    try:
        ipaddress.ip_address(host)
        return host                                   # host vốn là địa chỉ IP dạng số
    except ValueError:
        pass
    if host in _RESOLVER:
        lst = _RESOLVER[host]
        ip = lst[0]
        if len(lst) > 1:
            lst.pop(0)                                # advance: lần sau trả IP kế tiếp
        return ip
    # Không có trong bảng giả lập -> dùng DNS thật (ngoài demo).
    try:
        return socket.getaddrinfo(host, None)[0][4][0]
    except socket.gaierror:
        raise FetchError(f"Không phân giải được tên {host}") from None


def validate_ip(ip):
    """Chặn nếu IP thuộc dải nội bộ/loopback/link-local… (dùng cho bản AN TOÀN)."""
    addr = ipaddress.ip_address(ip)
    for net in BLOCKED_NETS:
        if addr.version == net.version and addr in net:
            raise SsrfError(f"Chặn SSRF: địa chỉ {ip} thuộc dải cấm {net}")


def dial(ip, port, timeout):
    """Mở socket tới (ip, port). Bảng _DIAL (chỉ demo) cho phép ánh xạ ip -> 127.0.0.1:cổng_cục_bộ."""
    if ip in _DIAL:
        return socket.create_connection(("127.0.0.1", _DIAL[ip]), timeout=timeout)
    return socket.create_connection((ip, port), timeout=timeout)


def _split_url(url):
    u = urllib.parse.urlsplit(url)
    host = u.hostname or ""
    port = u.port or (443 if u.scheme == "https" else 80)
    path = u.path or "/"
    if u.query:
        path += "?" + u.query
    return u, host, port, path


def _http_get(ip, host, port, path, timeout, limit_bytes):
    """Gửi GET HTTP/1.0 (Connection: close) tới IP đã chọn, đọc phản hồi (giới hạn kích thước)."""
    s = dial(ip, port, timeout)
    try:
        s.settimeout(timeout)
        req = (f"GET {path} HTTP/1.0\r\nHost: {host}\r\n"
               f"Accept: */*\r\nConnection: close\r\n\r\n").encode("utf-8")
        s.sendall(req)
        data = b""
        while len(data) < limit_bytes:
            chunk = s.recv(4096)
            if not chunk:
                break
            data += chunk
    finally:
        s.close()
    head, _, body = data.partition(b"\r\n\r\n")
    lines = head.decode("latin1").split("\r\n")
    status = int(lines[0].split(" ")[1]) if len(lines[0].split(" ")) > 1 else 0
    headers = {}
    for ln in lines[1:]:
        if ":" in ln:
            k, v = ln.split(":", 1)
            headers[k.strip().lower()] = v.strip()
    return status, headers, body


def fetch_naive(url):
    """Bản NGÂY THƠ: KHÔNG kiểm tra gì. Phân giải tên khi 'kiểm tra' rồi phân giải LẠI khi nối
    (lỗi TOCTOU), theo mọi chuyển hướng, không giới hạn. Đây là ví dụ dễ bị SSRF – không dùng thật."""
    log = []
    for _ in range(MAX_REDIRECT + 1):
        u, host, port, path = _split_url(url)
        _ = resolve_once(host)                        # "kiểm tra" hình thức rồi vứt đi (vô dụng)
        ip = resolve_once(host)                       # nối bằng lần phân giải MỚI -> rebinding lọt
        log.append(f"nối tới {host} ({ip}):{port}{path}")
        status, headers, body = _http_get(ip, host, port, path, HAN_GIAY, 1 << 30)
        if status in (301, 302, 303, 307, 308) and "location" in headers:
            url = urllib.parse.urljoin(url, headers["location"])
            log.append(f"theo chuyển hướng -> {url}")
            continue
        return status, body, log
    raise FetchError("Quá nhiều chuyển hướng")


def fetch_safe(url):
    """Bản AN TOÀN: lần lượt áp các lớp phòng thủ SSRF."""
    log = []
    deadline = time.monotonic() + HAN_GIAY
    for hop in range(MAX_REDIRECT + 1):
        u, host, port, path = _split_url(url)
        if u.scheme not in ("http", "https"):                     # (1) chỉ cho http/https
            raise SsrfError(f"Chặn SSRF: giao thức '{u.scheme}' không được phép")
        if u.username or u.password:                              # (2) loại bỏ thông tin đăng nhập
            log.append("gỡ bỏ thông tin đăng nhập nhúng trong URL")
        if not host:
            raise SsrfError("Chặn SSRF: URL thiếu tên máy")
        ip = resolve_once(host)                                   # (3) phân giải MỘT lần
        validate_ip(ip)                                           # (4) chặn dải nội bộ/loopback/link-local
        con_lai = deadline - time.monotonic()
        if con_lai <= 0:                                          # (5) giới hạn tổng thời gian
            raise FetchError("Hết thời gian cho phép")
        log.append(f"cho phép {host} ({ip}):{port}{path}; nối tới ĐÚNG IP đã kiểm tra")
        # (6) nối tới chính IP đã kiểm tra (chống DNS rebinding/TOCTOU), (7) giới hạn kích thước
        status, headers, body = _http_get(ip, host, port, path, min(2.0, con_lai), MAX_BYTES)
        if status in (301, 302, 303, 307, 308) and "location" in headers:
            if hop >= MAX_REDIRECT:                               # (8) giới hạn số lần chuyển hướng
                raise SsrfError("Chặn SSRF: vượt số lần chuyển hướng cho phép")
            url = urllib.parse.urljoin(url, headers["location"])
            log.append(f"kiểm tra lại chặng chuyển hướng -> {url}")
            continue                                              # (9) kiểm tra lại MỌI chặng
        return status, body, log
    raise FetchError("Quá nhiều chuyển hướng")


def log_err(*parts):
    print("[ssrf]", *parts, file=sys.stderr, flush=True)
