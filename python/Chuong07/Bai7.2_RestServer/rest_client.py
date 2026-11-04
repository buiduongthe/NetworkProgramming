"""Bài 7.2 – Client thử server REST: gửi nhiều yêu cầu trên MỘT kết nối giữ lại (keep-alive).

Dùng lại Reader, READ_LINE, READ_HEADERS, READ_EXACT của Mã giả 7.1 (Mục 7.2.1).
Mỗi dòng nhập có dạng:  PHƯƠNG_THỨC ĐƯỜNG_DẪN [THÂN_JSON]
ví dụ  GET /api/v1/terms/TCP   hoặc   POST /api/v1/terms {"term":"QUIC","text":"..."}

Chạy:  python3 rest_client.py [host] [port]      (mặc định 127.0.0.1 8080)
"""
import socket
import sys


class Reader:
    """Mã giả 3.5 – Reader { sock, pending }."""

    def __init__(self, sock):
        self.sock, self.pending = sock, b""

    def read_line(self, max_len):
        while (i := self.pending.find(b"\n")) < 0:
            if len(self.pending) > max_len:
                raise ValueError("Dòng quá dài")
            chunk = self.sock.recv(4096)
            if not chunk:
                return None
            self.pending += chunk
        if i > max_len:                                  # Mã giả 3.5 dòng 5: có LF nhưng quá dài
            raise ValueError("Dòng quá dài")
        line, self.pending = self.pending[:i], self.pending[i + 1:]
        return line.rstrip(b"\r").decode("utf-8")

    def read_exact(self, n):
        data, self.pending = self.pending[:n], self.pending[n:]
        while len(data) < n:
            chunk = self.sock.recv(n - len(data))
            if not chunk:
                raise ConnectionError("Kết nối đóng giữa chừng")
            data += chunk
        return data


def read_headers(r):
    hdr = {}
    while (line := r.read_line(8192)) not in ("", None):
        k, _, v = line.partition(":")
        hdr[k.strip().lower()] = v.strip()
    return hdr


def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 8080
    s = socket.create_connection((host, port), timeout=3)   # SAFE_CONNECT (rút gọn)
    s.settimeout(5.0)
    r = Reader(s)
    print(f"[client] Đã kết nối {host}:{port} (một kết nối cho mọi yêu cầu)", flush=True)
    try:
        for line in sys.stdin:
            parts = line.strip().split(" ", 2)
            if len(parts) < 2:
                continue
            method, path = parts[0], parts[1]
            body = parts[2].encode("utf-8") if len(parts) == 3 else b""
            h = f"{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\n"
            if body:
                h += f"Content-Type: application/json\r\nContent-Length: {len(body)}\r\n"
            s.sendall((h + "\r\n").encode("ascii") + body)
            status_line = r.read_line(8192)
            if status_line is None:
                print("[client] Server đã đóng kết nối", flush=True)
                break
            hdr = read_headers(r)
            data = r.read_exact(int(hdr.get("content-length", "0")))
            print(f"[client] {method} {path} -> {status_line}", flush=True)
            if "location" in hdr:
                print(f"[client]   Location: {hdr['location']}", flush=True)
            if data:
                print(f"[client]   {data.decode('utf-8')}", flush=True)
    finally:
        s.close()
        print("[client] Đã đóng kết nối", flush=True)


if __name__ == "__main__":
    main()
