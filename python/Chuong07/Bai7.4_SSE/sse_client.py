"""Bài 7.4 – Bộ đọc luồng Server-Sent Events trên socket TCP.

Cài đặt Mã giả 7.5 (SSE_READ, Mục 7.3.3, Giáo trình Lập trình mạng).
Kế thừa: Mã giả 7.1 (gửi GET, đọc dòng trạng thái và tiêu đề, giải mã chunked), 3.5 (READ_LINE).
Khi phản hồi dùng chunked, bộ đọc dòng được đặt phía sau lớp giải mã khối, để các dòng độ dài khối
không lẫn vào luồng sự kiện.

Chạy:  python3 sse_client.py [host] [port] [đường_dẫn]   (mặc định 127.0.0.1 8080 /events)
"""
import socket
import sys


class Reader:
    """Mã giả 3.5 – Reader { sock, pending }; 'sock' là bất kỳ nguồn nào có recv(n)."""

    def __init__(self, sock):
        self.sock, self.pending = sock, b""


def read_line(r, max_len):
    """Mã giả 3.5 – READ_LINE (None khi nguồn đã hết)."""
    while (i := r.pending.find(b"\n")) < 0:
        if len(r.pending) > max_len:
            raise ValueError("Dòng quá dài")
        chunk = r.sock.recv(4096)
        if not chunk:
            return None
        r.pending += chunk
    if i > max_len:                                  # Mã giả 3.5 dòng 5: có LF nhưng quá dài
        raise ValueError("Dòng quá dài")
    line, r.pending = r.pending[:i], r.pending[i + 1:]
    return line.rstrip(b"\r").decode("utf-8")


def read_exact(r, n):
    data, r.pending = r.pending[:n], r.pending[n:]
    while len(data) < n:
        chunk = r.sock.recv(n - len(data))
        if not chunk:
            raise ConnectionError("Kết nối đóng giữa chừng")
        data += chunk
    return data


def read_headers(r):
    """Mã giả 7.1 – READ_HEADERS (dòng 25–34)."""
    hdr = {}
    while True:
        line = read_line(r, 8192)
        if line is None:                                # dòng 29: đóng giữa phần tiêu đề
            raise ConnectionError("Kết nối đóng khi chưa đọc hết phần tiêu đề")   # ConnectionClosed
        if line == "":                                  # dòng 30: dòng trống – hết
            return hdr
        k, _, v = line.partition(":")
        hdr[k.strip().lower()] = v.strip()


class ChunkedSource:
    """Lớp giải mã khối (READ_CHUNKED của Mã giả 7.1 viết dạng luồng): recv() trả dữ liệu thân thuần."""

    def __init__(self, r):
        self.r, self.left, self.done = r, 0, False

    def recv(self, n):
        if self.done:
            return b""
        if self.left == 0:
            size = int(read_line(self.r, 1024).split(";")[0], 16)
            if size == 0:                                 # khối 0: hết thân, bỏ trailer
                read_headers(self.r)
                self.done = True
                return b""
            self.left = size
        data = read_exact(self.r, min(n, self.left))
        self.left -= len(data)
        if self.left == 0:
            read_line(self.r, 2)                          # CRLF sau mỗi khối
        return data


def sse_read(r, on_event, state):
    """Mã giả 7.5: r là Reader đặt sau phần tiêu đề."""
    ev = {"type": "message", "data": []}
    while (line := read_line(r, 65536)) is not None:
        if line == "":                                    # dòng trống: hết một sự kiện
            if ev["data"]:
                on_event(ev["type"], "\n".join(ev["data"]))
            ev = {"type": "message", "data": []}
        elif not line.startswith(":"):                    # ":" mở đầu dòng chú thích
            field, _, value = line.partition(":")
            if value.startswith(" "):
                value = value[1:]                         # STRIP_ONE_SPACE
            if field == "event":
                ev["type"] = value
            elif field == "data":
                ev["data"].append(value)
            elif field == "id":
                state["last_event_id"] = value
            elif field == "retry":
                state["reconnect_ms"] = int(value)
    # None: server đóng luồng


def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 8080
    path = sys.argv[3] if len(sys.argv) >= 4 else "/events"
    s = socket.create_connection((host, port), timeout=3)      # SAFE_CONNECT (rút gọn)
    s.settimeout(30.0)
    with s:
        s.sendall(f"GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: text/event-stream\r\n"
                  "Cache-Control: no-cache\r\nConnection: close\r\n\r\n".encode("ascii"))
        r = Reader(s)
        status = read_line(r, 8192)
        hdr = read_headers(r)
        print(f"[client] {status}; Content-Type: {hdr.get('content-type')}", flush=True)
        if not status.split(" ")[1] == "200":
            return
        if hdr.get("transfer-encoding", "").lower().endswith("chunked"):
            print("[client] Thân dùng chunked: đặt bộ đọc dòng sau lớp giải mã khối", flush=True)
            r = Reader(ChunkedSource(r))
        state = {"last_event_id": None, "reconnect_ms": None}
        def on_event(t, d):                                    # in '\n' dạng hai ký tự để thấy nhiều dòng data
            print(f"[client] Sự kiện '{t}': '{d.replace(chr(10), '\\n')}'", flush=True)
        sse_read(r, on_event, state)
        print(f"[client] Server đóng luồng; id cuối = {state['last_event_id']}, "
              f"retry = {state['reconnect_ms']} ms", flush=True)


if __name__ == "__main__":
    main()
