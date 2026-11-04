"""Bài mở rộng MR3.1 – Server đích để thử bộ chuyển tiếp cổng.

Dịch vụ "chữ hoa": đọc toàn bộ dữ liệu tới khi client đóng chiều gửi (nhận b""),
rồi gửi lại bản viết hoa và đóng kết nối. Vì server chỉ trả lời SAU KHI thấy EOF,
bộ chuyển tiếp ở giữa bắt buộc phải chuyển tiếp đúng việc đóng một chiều (half-close);
nếu không, client và server sẽ chờ nhau mãi.

Chạy:  python3 server_dich.py [host] [port]      (mặc định 127.0.0.1 9000)
"""
import socket
import sys
import threading


def handle(conn, addr):
    with conn:
        chunks = []
        while True:
            data = conn.recv(65536)
            if not data:                                 # client đã SHUTDOWN(WRITE)
                break
            chunks.append(data)
        body = b"".join(chunks)
        reply = body.decode("utf-8").upper().encode("utf-8")
        conn.sendall(reply)
        print(f"[dich] {addr[0]}:{addr[1]} gửi {len(body)} byte, trả {len(reply)} byte chữ hoa", flush=True)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 9000
    srv = socket.create_server((host, port))
    print(f"[dich] Server chữ hoa lắng nghe tại {host}:{port}", flush=True)
    try:
        while True:
            conn, addr = srv.accept()
            threading.Thread(target=handle, args=(conn, addr), daemon=True).start()
    except KeyboardInterrupt:
        print("[dich] Dừng server", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
