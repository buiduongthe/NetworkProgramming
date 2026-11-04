"""Bài 3.1 – TCP Echo Client (phiên bản đầu, chưa định khung).

Cài đặt Mã giả 3.2 (Mục 3.2.3, Giáo trình Lập trình mạng).
Đọc từng dòng từ bàn phím, gửi tới server và in phản hồi.
Lưu ý (như trong giáo trình): lời gọi recv() giả định nhận đủ phản hồi trong một lần;
điều này không đúng với thông điệp lớn và được khắc phục ở Bài 3.2 (định khung).

Chạy:  python3 echo_client.py [host] [port]      (mặc định 127.0.0.1 9000)
"""
import os
import socket
import sys


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1 (xem echo_server.py)."""
    host, port = default_host, os.environ.get("APP_PORT", default_port)
    if len(args) >= 1:
        host = args[0]
    if len(args) >= 2:
        port = args[1]
    return host, int(port)


def safe_connect(host, port, timeout_ms):
    """Mã giả 2.2 (rút gọn): thử lần lượt các địa chỉ, có thời gian chờ kết nối."""
    last_error = None
    for family, socktype, proto, _, addr in socket.getaddrinfo(host, port, type=socket.SOCK_STREAM):
        s = socket.socket(family, socktype, proto)
        s.settimeout(timeout_ms / 1000)
        try:
            s.connect(addr)
            return s
        except OSError as e:
            last_error = e
            s.close()
    raise ConnectionError(f"Không kết nối được {host}:{port}: {last_error}")


def main():
    host, port = parse_endpoint(sys.argv[1:], "127.0.0.1", 9000)
    s = safe_connect(host, port, 3000)
    s.settimeout(5.0)                                # RECV_TIMEOUT 5000 ms
    print(f"[client] Đã kết nối {host}:{port}. Nhập dòng cần gửi (Ctrl+D để kết thúc).", flush=True)
    try:
        for line in sys.stdin:                       # READ_CONSOLE()
            line = line.rstrip("\n")
            s.sendall(line.encode("utf-8"))          # SEND_ALL(ENCODE(line, "UTF-8"))
            reply = s.recv(4096)                     # giả định nhận đủ một lần
            if not reply:
                break
            print(f"[client] Gửi: {line!r} -> Nhận: {reply.decode('utf-8')!r}", flush=True)
        s.shutdown(socket.SHUT_WR)                   # SHUTDOWN(WRITE): báo hết dữ liệu cho server
    finally:
        s.close()
        print("[client] Đã đóng kết nối", flush=True)


if __name__ == "__main__":
    main()
