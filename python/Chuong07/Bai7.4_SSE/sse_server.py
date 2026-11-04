"""Bài 7.4 – Server Server-Sent Events nhỏ đi kèm bộ đọc luồng.

Phát luồng text/event-stream (Mục 7.3.3) bằng mã hóa khối (chunked) vì server không biết trước độ dài.
Luồng gồm: chú thích (nhịp tim), retry, hai sự kiện "delta" có id, một sự kiện nhiều dòng data
(cố ý chia đôi qua hai khối, cắt giữa dòng) và sự kiện "done"; sau đó gửi khối 0 và đóng kết nối.
Mỗi kết nối một luồng (Mã giả 6.1).

Chạy:  python3 sse_server.py [host] [port]      (mặc định 0.0.0.0 8080)
"""
import socket
import sys
import threading
import time

EVENTS = [
    ": nhip tim\n\n",
    "retry: 3000\n\n",
    'id: 1\nevent: delta\ndata: {"text":"Giao thức"}\n\n',
    'id: 2\nevent: delta\ndata:{"text":" TCP"}\n\n',          # không có dấu cách sau ':' cũng hợp lệ
    "id: 3\ndata: dòng thứ nhất\ndata: dòng th",             # sự kiện này bị cắt giữa dòng ...
    "ứ hai\n\n",                                              # ... và kết thúc ở khối sau
    "event: done\ndata: [DONE]\n\n",
]


def send_chunk(conn, text):
    """Một khối của Transfer-Encoding: chunked: độ dài hex, CRLF, dữ liệu, CRLF."""
    b = text.encode("utf-8")
    conn.sendall(f"{len(b):X}\r\n".encode("ascii") + b + b"\r\n")
    return len(b)


def serve(conn, addr):
    with conn:
        try:
            head = b""
            while b"\r\n\r\n" not in head:                   # đọc dòng yêu cầu và tiêu đề
                chunk = conn.recv(4096)
                if not chunk:
                    return
                head += chunk
            line = head.split(b"\r\n")[0].decode("ascii")
            if line.split(" ")[1] != "/events":
                conn.sendall(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                return
            print(f"[server] Client {addr} đăng ký luồng: {line}", flush=True)
            conn.sendall(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n"
                         b"Cache-Control: no-cache\r\nTransfer-Encoding: chunked\r\n\r\n")
            for i, ev in enumerate(EVENTS, 1):
                n = send_chunk(conn, ev)
                shown = ev.replace("\n", "\\n")             # in '\n' dạng hai ký tự
                print(f"[server] Gửi khối {i} ({n} byte): '{shown}'", flush=True)
                time.sleep(0.3)                              # sự kiện đến dần theo thời gian
            conn.sendall(b"0\r\n\r\n")                       # khối 0: kết thúc thân
            print(f"[server] Kết thúc luồng cho {addr}", flush=True)
        except OSError as e:
            print(f"[server] Client {addr} ngắt kết nối: {e}", flush=True)


def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "0.0.0.0"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 8080
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((host, port))
    srv.listen(16)
    print(f"[server] Đang lắng nghe tại {host}:{port}", flush=True)
    try:
        while True:
            conn, (ip, p) = srv.accept()
            threading.Thread(target=serve, args=(conn, f"{ip}:{p}"), daemon=True).start()
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
