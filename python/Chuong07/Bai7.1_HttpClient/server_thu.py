"""Bài 7.1 – Server HTTP thử nghiệm cho client tối giản.

Server tuần tự (khung của Mã giả 3.1) trả về ba kiểu định khung thân của HTTP/1.1 (Mục 7.2.1):
  /api/v1/terms/TCP  -> Content-Length
  /chunked           -> Transfer-Encoding: chunked (có phần mở rộng khối và trailer)
  /dong-ket-noi      -> không có độ dài, server đóng kết nối khi hết dữ liệu
  đường dẫn khác     -> 404

Chạy:  python3 server_thu.py [host] [port]      (mặc định 0.0.0.0 8080)
"""
import json
import socket
import sys

TERM = {"term": "TCP", "text": "Giao thức vận chuyển hướng kết nối"}


def read_request_head(conn):
    """Đọc dòng yêu cầu và tiêu đề đến dòng trống (đủ cho server thử)."""
    buf = b""
    while b"\r\n\r\n" not in buf:
        chunk = conn.recv(4096)
        if not chunk:
            return None
        buf += chunk
    return buf.split(b"\r\n")[0].decode("ascii")


def respond(conn, path):
    if path == "/api/v1/terms/TCP":
        body = json.dumps(TERM, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        head = ("HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\n"
                f"Content-Length: {len(body)}\r\n\r\n")
        conn.sendall(head.encode("ascii") + body)
        return "Content-Length"
    if path == "/chunked":
        parts = ["Thân phản hồi ", "được chia thành ", "nhiều khối."]
        out = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n"
        out += b"Transfer-Encoding: chunked\r\nTrailer: X-So-Khoi\r\n\r\n"
        for i, p in enumerate(parts):
            b = p.encode("utf-8")
            ext = ";vi-du=mo-rong" if i == 0 else ""      # phần mở rộng khối (bị client bỏ qua)
            out += f"{len(b):X}{ext}\r\n".encode("ascii") + b + b"\r\n"
        out += b"0\r\nX-So-Khoi: 3\r\n\r\n"               # khối 0, trailer, dòng trống
        conn.sendall(out)
        return "chunked"
    if path == "/dong-ket-noi":
        conn.sendall("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n"
                     "Không có Content-Length: client đọc đến khi kết nối đóng.".encode("utf-8"))
        return "đóng kết nối"
    body = b'{"type":"about:blank","title":"Not Found","status":404}'
    conn.sendall(("HTTP/1.1 404 Not Found\r\nContent-Type: application/problem+json\r\n"
                  f"Content-Length: {len(body)}\r\n\r\n").encode("ascii") + body)
    return "Content-Length"


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
            conn, addr = srv.accept()
            with conn:
                line = read_request_head(conn)
                if line is None:
                    continue
                how = respond(conn, line.split(" ")[1])
                print(f"[server] {line} -> định khung: {how}", flush=True)
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
