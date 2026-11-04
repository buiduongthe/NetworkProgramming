"""Bài 6.1 – Server tra cứu dùng nhóm luồng và hàng đợi có giới hạn.

Cài đặt Mã giả 6.2 (Mục 6.2.2, Giáo trình Lập trình mạng). Kế thừa Mã giả 6.1: dùng lại
nguyên SERVE (thread_server.py); k luồng thợ dùng chung một hàng đợi; hàng đợi đầy → 503.

Chạy:  python3 pool_server.py [host] [port] [k] [queue_size]   (mặc định 0.0.0.0 9000 8 16)
"""
import queue
import socket
import sys
import threading

from netutil import int_arg, parse_endpoint, send_json
from thread_server import AtomicCounter, log, serve


def worker(i, q):
    """WORKER: lấy kết nối khỏi hàng đợi rồi phục vụ bằng SERVE."""
    while True:
        conn, addr = q.get()                                    # q.take(): chờ đến khi có việc
        log(f"[server] Luồng thợ {i} phục vụ {addr}")
        serve(conn, addr, AtomicCounter(1))                     # bộ đếm tạm


def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9000)
    k = int_arg(sys.argv, 3, 8)
    queue_size = int_arg(sys.argv, 4, 16)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)    # SOCKET, SETOPT, BIND, LISTEN như Mã giả 6.1
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((host, port))
    srv.listen(128)
    q = queue.Queue(maxsize=queue_size)                         # BLOCKING_QUEUE(queue_size)
    for i in range(1, k + 1):                                   # FOR i ← 1 TO k: SPAWN_THREAD(WORKER, q)
        threading.Thread(target=worker, args=(i, q), daemon=True).start()
    log(f"[server] Đang lắng nghe tại {host}:{port}")
    log(f"[server] Nhóm {k} luồng thợ, hàng đợi {queue_size} chỗ")
    try:
        while True:
            conn, a = srv.accept()                              # ACCEPT
            addr = f"{a[0]}:{a[1]}"
            log(f"[server] Nhận {addr}")
            try:
                q.put_nowait((conn, addr))                      # q.try_put: không chờ
            except queue.Full:                                  # hàng đợi đầy
                log(f"[server] Hàng đợi đầy, trả 503 cho {addr}")
                try:
                    send_json(conn, {"status": 503, "error": "Server bận"})
                except OSError:
                    pass                                        # client đã đi: bỏ qua
                conn.close()
    except KeyboardInterrupt:
        log("[server] Dừng server")
    finally:
        srv.close()


if __name__ == "__main__":
    main()
