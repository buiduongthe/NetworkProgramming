"""Bài 4.1 – UDP Echo Server.

Cài đặt Mã giả 4.1 (Mục 4.2.1, Giáo trình Lập trình mạng).
So với TCP Echo Server (Mã giả 3.1): không có LISTEN/ACCEPT, một socket phục vụ mọi client;
mỗi lần RECVFROM nhận đúng một datagram nguyên vẹn kèm địa chỉ người gửi.

Chạy:  python3 udp_echo_server.py [host] [port]      (mặc định 0.0.0.0 9000)
"""
import os
import socket
import sys


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > giá trị mặc định."""
    host = os.environ.get("APP_HOST", default_host)
    port = os.environ.get("APP_PORT", default_port)
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


def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9000)      # Mã giả 4.1, dòng 2
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)             # dòng 3: SOCKET(INET, DGRAM)
    s.bind((host, port))                                              # dòng 4: BIND
    print(f"[server] Đang lắng nghe UDP tại {host}:{port}", flush=True)
    try:
        while True:                                                   # dòng 5: LOOP
            try:
                data, addr = s.recvfrom(65535)                        # dòng 6: datagram nguyên vẹn
            except ConnectionResetError:                              # Windows: ICMP port unreachable (Mục 4.2.2)
                continue
            print(f"[server] Nhận {len(data)} byte từ {addr[0]}:{addr[1]}", flush=True)  # dòng 7
            s.sendto(data, addr)                                      # dòng 8: trả lời đúng người gửi
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        s.close()


if __name__ == "__main__":
    main()
