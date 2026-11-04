"""Bài 2.2 – Server thử nghiệm tối giản cho safe_connect.py.

Chỉ chấp nhận kết nối, in địa chỉ client rồi đóng ngay; dùng để tạo trường hợp kết nối
thành công. Chuỗi lời gọi SOCKET – BIND – LISTEN – ACCEPT được trình bày chi tiết ở Chương 3.

Chạy:  python3 test_server.py [host] [port]      (mặc định 0.0.0.0 9000)
"""
import os
import socket
import sys


def main():
    args = sys.argv[1:]
    host = args[0] if len(args) >= 1 else os.environ.get("APP_HOST", "0.0.0.0")
    port = int(args[1] if len(args) >= 2 else os.environ.get("APP_PORT", 9000))
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)      # SOCKET(INET, STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)    # SETOPT(REUSEADDR)
    srv.bind((host, port))                                        # BIND
    srv.listen(16)                                                # LISTEN
    print(f"[server] Đang lắng nghe tại {host}:{port}", flush=True)
    try:
        while True:
            conn, addr = srv.accept()                             # ACCEPT
            print(f"[server] Client kết nối: {addr[0]}:{addr[1]}", flush=True)
            conn.close()                                          # CLOSE ngay
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
