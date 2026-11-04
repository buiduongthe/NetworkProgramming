"""Bài mở rộng MR5.3 – Các tác vụ mạng chạy nền và bộ giới hạn đồng thời (không dùng Tk).

Mọi hàm ở đây chạy trên luồng nền, KHÔNG chạm vào widget Tk. Tiến trình được báo qua hàm
progress(phần_trăm, chữ) do giao diện cung cấp (giao diện đưa nó vào hàng đợi an toàn luồng).
Hủy theo kiểu hợp tác: tác vụ kiểm tra cờ `cancel` (threading.Event) giữa các bước.
"""
import http.client
import socket
import threading
import time


class Cancelled(Exception):
    """Tác vụ bị người dùng hủy."""


class Limiter:
    """Giới hạn số tác vụ chạy cùng lúc; giới hạn đổi được khi đang chạy (thanh trượt)."""

    def __init__(self, n):
        self.n, self.running = n, 0
        self.cv = threading.Condition()

    def set(self, n):
        with self.cv:
            self.n = n
            self.cv.notify_all()

    def acquire(self, cancel):
        with self.cv:
            while self.running >= self.n:
                if cancel.is_set():
                    return False
                self.cv.wait(0.1)             # thức dậy định kỳ để kiểm tra cờ hủy
            self.running += 1
            return True

    def release(self):
        with self.cv:
            self.running -= 1
            self.cv.notify_all()


def download(host, port, kb, delay, cancel, progress):
    """Tải /tep?kb=..&tre=.. qua HTTP, đọc từng phần và báo tiến trình theo Content-Length."""
    conn = http.client.HTTPConnection(host, port, timeout=3)
    try:
        conn.request("GET", f"/tep?kb={kb}&tre={delay}")
        resp = conn.getresponse()
        total, got = int(resp.getheader("Content-Length")), 0
        while got < total:
            if cancel.is_set():
                raise Cancelled()
            chunk = resp.read1(16 * 1024)    # đọc phần đã có, tối đa 16 KB
            if not chunk:
                raise OSError("máy chủ đóng kết nối sớm")
            got += len(chunk)
            progress(got * 100 // total, f"{got // 1024}/{total // 1024} KB")
        return f"xong {total // 1024} KB"
    finally:
        conn.close()


def check_ports(host, ports, cancel, progress):
    """Kiểm tra lần lượt từng cổng TCP (như Bài 1.2): kết nối được = mở."""
    opened = []
    for i, p in enumerate(ports):
        if cancel.is_set():
            raise Cancelled()
        with socket.socket() as s:
            s.settimeout(0.3)
            ok = s.connect_ex((host, p)) == 0
        if ok:
            opened.append(p)
        time.sleep(0.3)                       # quét chậm cho dễ quan sát
        progress((i + 1) * 100 // len(ports), f"cổng {p} {'mở' if ok else 'đóng'}")
    return f"{len(opened)} mở, {len(ports) - len(opened)} đóng"


def lookup_names(host, port, names, cancel, progress):
    """Tra cứu tên qua máy chủ "DNS" giả lập trên UDP, có thời gian chờ."""
    found = 0
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
        s.settimeout(1.0)
        for i, name in enumerate(names):
            if cancel.is_set():
                raise Cancelled()
            s.sendto(f"A {name}".encode(), (host, port))
            try:
                answer = s.recv(512).decode()
            except socket.timeout:
                answer = f"{name} (hết thời gian chờ)"
            if "NXDOMAIN" not in answer:
                found += 1
            progress((i + 1) * 100 // len(names), answer)
    return f"{found}/{len(names)} tên có địa chỉ"
