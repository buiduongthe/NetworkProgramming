"""Bài 6.1 – Server tra cứu đa luồng: mỗi kết nối một luồng.

Cài đặt Mã giả 6.1 (Mục 6.2.1, Giáo trình Lập trình mạng). Kế thừa Mã giả 3.1 (khung server),
phần nghiệp vụ là HANDLE_LOOKUP của Mã giả 3.7 (lookup_service.py).

Chạy:  python3 thread_server.py [host] [port] [max_clients]   (mặc định 0.0.0.0 9000 500)
"""
import socket
import sys
import threading

from lookup_service import handle_lookup
from netutil import ConnectionClosed, ProtocolError, int_arg, parse_endpoint

MAX_CLIENTS = 500                                       # CONST MAX_CLIENTS ← 500
_print_lock = threading.Lock()


def log(msg):
    """LOG: in một dòng trọn vẹn (các luồng không chen giữa dòng của nhau)."""
    with _print_lock:
        print(msg, flush=True)


class AtomicCounter:
    """ATOMIC_COUNTER: phép tăng, giảm được bảo vệ bằng khóa nên không bị xen ngang."""
    def __init__(self, value=0):
        self._value = value
        self._lock = threading.Lock()

    @property
    def value(self):
        with self._lock:
            return self._value

    def increment(self):
        with self._lock:
            self._value += 1
            return self._value

    def decrement(self):
        with self._lock:
            self._value -= 1
            return self._value


def serve(conn, addr, active):
    """SERVE – khối TRY của Mã giả 3.1, chạy trong luồng riêng của mỗi kết nối."""
    try:
        conn.settimeout(300)                            # SETOPT(RECV_TIMEOUT, 300000): đóng kết nối nhàn rỗi
        handle_lookup(conn)
    except (OSError, ConnectionClosed, ProtocolError) as e:   # NetError, Timeout (socket.timeout ⊂ OSError)
        log(f"[server] Lỗi với {addr}: {e}")
    finally:
        conn.close()
        active.decrement()                              # luôn giảm bộ đếm, kể cả khi lỗi
        log(f"[server] Client {addr} đã đóng kết nối")


def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9000)
    max_clients = int_arg(sys.argv, 3, MAX_CLIENTS)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)        # SOCKET(INET, STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)      # SETOPT(REUSEADDR)
    srv.bind((host, port))                                          # BIND
    srv.listen(128)                                                 # LISTEN, backlog 128
    log(f"[server] Đang lắng nghe tại {host}:{port}")
    log(f"[server] Mỗi kết nối một luồng, MAX_CLIENTS = {max_clients}")
    active = AtomicCounter(0)
    try:
        while True:
            conn, a = srv.accept()                                  # ACCEPT
            addr = f"{a[0]}:{a[1]}"
            if active.value >= max_clients:                         # MỚI: giới hạn
                log(f"[server] Từ chối {addr}: đã đủ {max_clients} kết nối")
                conn.close()
                continue
            n = active.increment()
            log(f"[server] Nhận {addr}, tạo luồng mới (đang phục vụ: {n})")
            threading.Thread(target=serve, args=(conn, addr, active), daemon=True).start()  # SPAWN_THREAD
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
