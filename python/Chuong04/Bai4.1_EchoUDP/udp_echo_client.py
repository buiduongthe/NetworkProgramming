"""Bài 4.1 – UDP Echo Client có thời gian chờ.

Cài đặt Mã giả 4.2 (Mục 4.2.1, Giáo trình Lập trình mạng).
Đọc từng dòng từ bàn phím, gửi một datagram tới server và chờ phản hồi tối đa 2 giây;
hết thời gian chờ thì báo datagram có thể đã mất (không có kết nối nên không có lỗi nào khác).

Chạy:  python3 udp_echo_client.py [host] [port]      (mặc định 127.0.0.1 9000)
"""
import os
import socket
import sys


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1 (xem udp_echo_server.py)."""
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
    host, port = parse_endpoint(sys.argv[1:], "127.0.0.1", 9000)    # Mã giả 4.2, dòng 2
    family, _, _, _, server = socket.getaddrinfo(host, port, type=socket.SOCK_DGRAM)[0]  # dòng 3: FIRST(RESOLVE)
    s = socket.socket(family, socket.SOCK_DGRAM)                      # dòng 4: SOCKET(FAMILY(server), DGRAM)
    s.settimeout(2.0)                                                 # dòng 5: RECV_TIMEOUT 2000 ms
    print(f"[client] Gửi tới {host}:{port} qua UDP. Nhập dòng cần gửi (Ctrl+Z/Ctrl+D để kết thúc).",
          flush=True)
    try:
        for line in sys.stdin:                                        # dòng 6: READ_CONSOLE()
            line = line.rstrip("\r\n")
            s.sendto(line.encode("utf-8"), server)                    # dòng 7: SENDTO
            try:
                data, addr = s.recvfrom(65535)                        # dòng 9: RECVFROM
                print(f"[client] Gửi: '{line}' -> Nhận: '{data.decode('utf-8')}'", flush=True)
            except (socket.timeout, ConnectionResetError):            # dòng 11: CATCH Timeout, ConnectionReset
                # ConnectionResetError: Windows báo ICMP port unreachable (Mục 4.2.2)
                print(f"[client] Gửi: '{line}' -> Không có phản hồi: datagram có thể đã mất", flush=True)
    finally:
        s.close()                                                     # dòng 15: CLOSE
        print("[client] Đã đóng socket", flush=True)


if __name__ == "__main__":
    main()
