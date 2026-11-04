"""Bài mở rộng MR6.1 – Client thử cho server tắt an toàn.

Gửi lần lượt các yêu cầu "LAM <ms>", in phản hồi; sau đó tiếp tục giữ kết nối và in mọi
dòng server gửi (BYE, HUY) cho tới khi server đóng kết nối.

Chạy:  python3 khach.py <tên> <host> <port> [ms ...]
"""
import socket
import sys
import time


def main():
    name, host, port = sys.argv[1], sys.argv[2], int(sys.argv[3])
    jobs = sys.argv[4:]
    tag = f"[khach {name}]"
    try:
        s = socket.create_connection((host, port), timeout=3)
    except OSError as e:
        print(f"{tag} Không kết nối được: {e.strerror or e}", flush=True)
        sys.exit(1)
    s.settimeout(10)
    f = s.makefile("r", encoding="utf-8")
    t0 = time.monotonic()
    with s, f:
        for ms in jobs:
            s.sendall(f"LAM {ms}\n".encode())
            print(f"{tag} Gửi LAM {ms}", flush=True)
            reply = f.readline()
            if not reply:
                print(f"{tag} Server đóng kết nối khi chưa trả lời", flush=True)
                return
            print(f"{tag} t={time.monotonic() - t0:.1f}s nhận: {reply.strip()}", flush=True)
        for line in f:                                  # giữ kết nối, chờ thông báo của server
            print(f"{tag} t={time.monotonic() - t0:.1f}s nhận: {line.strip()}", flush=True)
        print(f"{tag} t={time.monotonic() - t0:.1f}s server đóng kết nối", flush=True)


if __name__ == "__main__":
    main()
