"""Bài mở rộng MR5.3 – Máy chủ thử cục bộ cho ứng dụng giao diện đa tác vụ (chỉ chạy trong máy).

Một tiến trình cung cấp ba dịch vụ giả lập:
  - HTTP  <port>/tcp : GET /tep?kb=<số KB>&tre=<ms> – gửi tệp <kb> KB theo từng khối 16 KB,
                       nghỉ <tre> ms sau mỗi khối (giả lập đường truyền chậm, độ trễ khác nhau);
  - "DNS" <port>/udp : nhận "A <tên>", trả "<tên> <địa chỉ>" hoặc "<tên> NXDOMAIN" sau 100–400 ms
                       (bảng tên giả lập, không phải giao thức DNS thật – xem Bài 4.3);
  - cổng mở <port+1>, <port+2>/tcp: chỉ nhận rồi đóng kết nối, để ứng dụng kiểm tra cổng.

Chạy:  python3 may_chu_thu.py <host> <port>
"""
import socket
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse


class MayChuHTTP(ThreadingHTTPServer):
    """ThreadingHTTPServer không in traceback khi client đóng ngang (Windows: đóng socket còn dữ liệu chưa đọc -> RST
    -> ConnectionResetError/ConnectionAbortedError; mặc định http.server in cả traceback ra màn hình)."""

    def handle_error(self, request, client_address):
        if isinstance(sys.exc_info()[1], ConnectionError):
            return
        super().handle_error(request, client_address)


CHUNK = 16 * 1024
NAMES = {"router.lab": ("10.10.0.1", 100), "may-in.lab": ("10.10.0.21", 250),
         "camera-1.lab": ("10.10.0.31", 400), "nas.lab": ("10.10.0.40", 150)}


_out = threading.Lock()


def log(text):
    with _out:                             # nhiều luồng cùng in: in nguyên dòng một lần
        print(f"[server] {text}", flush=True)


class FileHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        url = urlparse(self.path)
        q = parse_qs(url.query)
        if url.path != "/tep":
            self.send_error(404)
            return
        kb, delay = int(q.get("kb", ["256"])[0]), int(q.get("tre", ["50"])[0])
        size = kb * 1024
        log(f"HTTP GET /tep {kb} KB, nghỉ {delay} ms mỗi khối")
        self.send_response(200)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Content-Length", str(size))
        self.end_headers()
        sent = 0
        try:
            while sent < size:
                n = min(CHUNK, size - sent)
                self.wfile.write(b"x" * n)
                self.wfile.flush()
                sent += n
                if sent < size:                    # khối cuối: không cần nghỉ
                    time.sleep(delay / 1000)
        except (BrokenPipeError, ConnectionResetError):
            log(f"HTTP: client đóng kết nối giữa chừng ({sent // 1024}/{kb} KB)")

    def log_message(self, *args):          # tắt nhật ký mặc định của http.server
        pass


def dns_server(host, port):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind((host, port))
    while True:
        data, addr = s.recvfrom(512)
        name = data.decode(errors="replace").removeprefix("A ").strip()

        def answer(name=name, addr=addr):
            ip, ms = NAMES.get(name, ("NXDOMAIN", 300))
            time.sleep(ms / 1000)
            s.sendto(f"{name} {ip}".encode(), addr)
        threading.Thread(target=answer, daemon=True).start()


def open_port(host, port):
    ls = socket.create_server((host, port))
    while True:
        c, _ = ls.accept()
        c.close()


def main():
    host, port = sys.argv[1], int(sys.argv[2])
    httpd = MayChuHTTP((host, port), FileHandler)
    threading.Thread(target=dns_server, args=(host, port), daemon=True).start()
    for p in (port + 1, port + 2):
        threading.Thread(target=open_port, args=(host, p), daemon=True).start()
    log(f"HTTP {host}:{port}/tcp, \"DNS\" {host}:{port}/udp, cổng mở {port + 1}, {port + 2}")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        log("dừng")


if __name__ == "__main__":
    main()
