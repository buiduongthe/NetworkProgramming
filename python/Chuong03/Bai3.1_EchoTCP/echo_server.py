"""Bài 3.1 – TCP Echo Server tuần tự.

Cài đặt Mã giả 3.1 (Mục 3.2.2, Giáo trình Lập trình mạng).
Server nhận dữ liệu từ client và gửi trả nguyên văn; phục vụ lần lượt từng client.

Chạy:  python3 echo_server.py [host] [port]      (mặc định 0.0.0.0 9000)
"""
import os
import socket
import sys


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1: ưu tiên tham số dòng lệnh > biến môi trường APP_PORT > giá trị mặc định."""
    host, port = default_host, os.environ.get("APP_PORT", default_port)
    if len(args) >= 1:
        host = args[0]
    if len(args) >= 2:
        port = args[1]
    try:
        port = int(port)
    except ValueError:
        sys.exit(f"Cổng phải là số nguyên: {port}")
    if not 1 <= port <= 65535:
        sys.exit(f"Cổng không hợp lệ: {port}")
    return host, port


def handle_echo(conn):
    """Vòng phục vụ một client: nhận bao nhiêu gửi trả bấy nhiêu."""
    while True:
        data = conn.recv(4096)          # RECV: trả về b"" khi client đã đóng chiều gửi
        if not data:
            break
        conn.sendall(data)              # SEND_ALL: gửi hết, kể cả khi SEND chỉ gửi được một phần


def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9000)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)      # SOCKET(INET, STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)    # SETOPT(REUSEADDR)
    srv.bind((host, port))                                        # BIND
    srv.listen(16)                                                # LISTEN, backlog 16
    print(f"[server] Đang lắng nghe tại {host}:{port}", flush=True)
    try:
        while True:                                               # vòng chấp nhận kết nối
            conn, addr = srv.accept()                             # ACCEPT: chờ client mới
            print(f"[server] Client kết nối: {addr[0]}:{addr[1]}", flush=True)
            try:
                handle_echo(conn)
            except OSError as e:                                  # lỗi của một client không dừng server
                print(f"[server] Lỗi với {addr}: {e}", flush=True)
            finally:
                conn.close()
                print(f"[server] Client {addr[0]}:{addr[1]} đã đóng kết nối", flush=True)
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
