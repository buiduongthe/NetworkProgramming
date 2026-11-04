"""Bài 6.3 – Nhiều client echo đồng thời để kiểm tra server reactor.

Mỗi client (một luồng) dựa trên Mã giả 3.2 nhưng nhận bằng RECV_EXACT (Mã giả 3.4): gửi một dòng chào,
nhận lại đúng số byte đã gửi; sau đó gửi một khối lớn (mặc định 1 MiB) trong một luồng phụ, đồng thời
nhận lại đủ khối ở luồng chính và so khớp. Khối lớn hơn MAX_BUF của server nên kiểm tra được việc
gửi một phần và áp lực ngược (server ngừng đọc khi bộ đệm gửi đầy). Cuối cùng client gửi thêm 200 000 byte
rồi đóng chiều gửi (SHUTDOWN WRITE) và đọc đến hết: server phải gửi nốt dữ liệu còn trong bộ đệm rồi mới đóng.

Chạy:  python3 echo_clients.py [host] [port] [n] [size]   (mặc định 127.0.0.1 9000 3 1048576)
"""
import os
import socket
import sys
import threading
import time

from netutil import ConnectionClosed, int_arg, parse_endpoint, recv_exact, safe_connect

_print_lock = threading.Lock()
TAIL = 200000                       # khối cuối gửi trước khi đóng chiều gửi (lớn hơn MAX_BUF của server)


def log(msg):
    """In một dòng trọn vẹn (các luồng không chen giữa dòng của nhau)."""
    with _print_lock:
        print(msg, flush=True)


def one_client(i, host, port, size):
    try:
        s = safe_connect(host, port, 3000)
    except ConnectionError as e:
        log(f"[client {i}] Lỗi: {e}")
        return
    s.settimeout(10.0)
    try:
        hello = f"Xin chào reactor từ client {i}".encode("utf-8")
        s.sendall(hello)
        back = recv_exact(s, len(hello))
        log(f"[client {i}] Nhận lại: '{back.decode('utf-8')}'")
        block = os.urandom(size)
        sender = threading.Thread(target=s.sendall, args=(block,))  # gửi song song với nhận
        sender.start()
        got = recv_exact(s, size)
        sender.join()
        log(f"[client {i}] Gửi {size} byte, nhận lại {len(got)} byte, khớp: {'có' if got == block else 'không'}")
        tail = os.urandom(TAIL)                                     # phần 3: gửi khối cuối rồi đóng chiều gửi
        def send_tail():
            s.sendall(tail)
            s.shutdown(socket.SHUT_WR)                              # SHUTDOWN(s, WRITE) như Mã giả 3.2
        sender = threading.Thread(target=send_tail)
        sender.start()
        rest = bytearray()
        while chunk := s.recv(65536):                               # đọc đến khi server đóng (FIN)
            rest += chunk
        sender.join()
        log(f"[client {i}] Gửi {TAIL} byte rồi SHUTDOWN(WRITE), nhận lại {len(rest)} byte trước FIN của server, "
            f"khớp: {'có' if bytes(rest) == tail else 'không'}")
    except (ConnectionClosed, OSError) as e:
        log(f"[client {i}] Lỗi: {e}")
    finally:
        s.close()


def main():
    host, port = parse_endpoint(sys.argv[1:], "127.0.0.1", 9000)
    n = int_arg(sys.argv, 3, 3)
    size = int_arg(sys.argv, 4, 1048576)
    print(f"[client] Mở {n} kết nối đồng thời, mỗi kết nối gửi thêm {size} byte", flush=True)
    t0 = time.perf_counter()
    threads = [threading.Thread(target=one_client, args=(i, host, port, size)) for i in range(1, n + 1)]
    for th in threads:
        th.start()
    for th in threads:
        th.join()
    print(f"[client] Hoàn tất sau {round((time.perf_counter() - t0) * 1000)} ms", flush=True)


if __name__ == "__main__":
    main()
