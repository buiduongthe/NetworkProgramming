"""Bài mở rộng MR7.1 – Server HTTP phục vụ tệp có hỗ trợ tải từng phần (Range, RFC 9110).

Phục vụ một tệp nhị phân sinh sẵn tại /tep.bin:
  - HEAD: trả Content-Length, Accept-Ranges: bytes, ETag, X-Checksum-SHA256.
  - GET có "Range: bytes=a-" hoặc "bytes=a-b": trả 206 Partial Content + Content-Range;
    dải không thỏa mãn được: 416 + "Content-Range: bytes */<kích thước>".
  - If-Range: <ETag>: nếu ETag không khớp (tệp đã đổi) thì bỏ qua Range, trả 200 toàn bộ.
  - Giả lập sự cố mạng: --so-lan-cat N yêu cầu GET đầu tiên bị ngắt kết nối sau --cat-sau byte.

Chạy:  python3 server_tep.py [host] [port] [--kich-thuoc B] [--cat-sau B] [--so-lan-cat N]
"""
import argparse
import hashlib
import itertools
import re
import socket
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class MayChuHTTP(ThreadingHTTPServer):
    """ThreadingHTTPServer không in traceback khi client đóng ngang (Windows: đóng socket còn dữ liệu chưa đọc -> RST
    -> ConnectionResetError/ConnectionAbortedError; mặc định http.server in cả traceback ra màn hình)."""

    def handle_error(self, request, client_address):
        if isinstance(sys.exc_info()[1], ConnectionError):
            return
        super().handle_error(request, client_address)


OPTS = None
DATA = b""
ETAG = ""
SHA = ""
counter = itertools.count(1)
cuts_left = 0


def make_data(size):
    """Nội dung tệp cố định (giống bản Go) để hai bản cho cùng SHA-256."""
    return bytes((i * 31 + i // 7) % 256 for i in range(size))


def log(text):
    print(f"[server] {text}", flush=True)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        pass

    def common_headers(self, status, length, extra=None):
        self.send_response(status)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Content-Length", str(length))
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("ETag", ETAG)
        self.send_header("Connection", "close")
        for k, v in (extra or {}).items():
            self.send_header(k, v)
        self.end_headers()

    def do_HEAD(self):
        k = next(counter)
        if self.path != "/tep.bin":
            self.common_headers(404, 0)
            return
        self.common_headers(200, len(DATA), {"X-Checksum-SHA256": SHA})
        log(f"#{k} HEAD -> 200, {len(DATA)} byte")

    def do_GET(self):
        global cuts_left
        k = next(counter)
        if self.path != "/tep.bin":
            self.common_headers(404, 0)
            return
        size = len(DATA)
        rng = self.headers.get("Range")
        if_range = self.headers.get("If-Range")
        start, end, status = 0, size - 1, 200
        if rng and (if_range is None or if_range == ETAG):        # If-Range khớp mới dùng Range
            m = re.fullmatch(r"bytes=(\d+)-(\d*)", rng.strip())
            if m:
                start = int(m.group(1))
                end = min(int(m.group(2)), size - 1) if m.group(2) else size - 1
                if start >= size or start > end:
                    self.common_headers(416, 0, {"Content-Range": f"bytes */{size}"})
                    log(f"#{k} GET Range={rng} -> 416")
                    return
                status = 206
        extra = {"Content-Range": f"bytes {start}-{end}/{size}"} if status == 206 else {}
        body = DATA[start:end + 1]
        self.common_headers(status, len(body), extra)
        if cuts_left > 0:                                         # giả lập đứt kết nối giữa chừng
            cuts_left -= 1
            self.wfile.write(body[:OPTS.cat_sau])
            self.wfile.flush()
            self.connection.shutdown(socket.SHUT_RDWR)
            self.close_connection = True
            log(f"#{k} GET Range={rng or '-'} -> {status}, NGẮT sau {min(OPTS.cat_sau, len(body))} byte")
            return
        self.wfile.write(body)
        log(f"#{k} GET Range={rng or '-'} -> {status}, gửi {len(body)} byte")


def main():
    global OPTS, DATA, ETAG, SHA, cuts_left
    ap = argparse.ArgumentParser()
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("port", nargs="?", type=int, default=8000)
    ap.add_argument("--kich-thuoc", type=int, default=300000)
    ap.add_argument("--cat-sau", type=int, default=100000, help="số byte gửi trước khi ngắt")
    ap.add_argument("--so-lan-cat", type=int, default=0, help="số yêu cầu GET đầu tiên bị ngắt")
    OPTS = ap.parse_args()
    DATA = make_data(OPTS.kich_thuoc)
    SHA = hashlib.sha256(DATA).hexdigest()
    ETAG = f'"{SHA[:16]}"'
    cuts_left = OPTS.so_lan_cat
    srv = MayChuHTTP((OPTS.host, OPTS.port), Handler)
    log(f"http://{OPTS.host}:{OPTS.port}/tep.bin, {len(DATA)} byte, ETag {ETAG}")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        log("Dừng server")
    finally:
        srv.server_close()


if __name__ == "__main__":
    main()
