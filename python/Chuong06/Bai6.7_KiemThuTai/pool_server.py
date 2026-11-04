"""Bài 6.7 – Bản sao server nhóm luồng của Bài 6.1 (Mã giả 6.2, Mục 6.2.2) để kiểm thử tải.

Khác Bài 6.1: không ghi nhật ký từng kết nối (số kết nối bị từ chối 503 được bộ sinh tải thống kê)
và có tham số delay_ms – độ trễ giả lập của PROCESS_REQUEST.

Chạy:  python3 pool_server.py [host] [port] [k] [queue_size] [delay_ms]   (mặc định 0.0.0.0 9000 8 16 50)
"""
import queue
import socket
import sys
import threading

import lookup_service
from netutil import int_arg, parse_endpoint, send_json
from thread_server import AtomicCounter, log, serve


def worker(i, q):
    """WORKER: lấy kết nối khỏi hàng đợi rồi phục vụ bằng SERVE."""
    while True:
        conn, addr = q.get()                                    # q.take(): chờ đến khi có việc
        serve(conn, addr, AtomicCounter(1))                     # bộ đếm tạm


def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9000)
    k = int_arg(sys.argv, 3, 8)
    queue_size = int_arg(sys.argv, 4, 16)
    lookup_service.DELAY_MS = int_arg(sys.argv, 5, 50)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)    # SOCKET, SETOPT, BIND, LISTEN như Mã giả 6.1
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((host, port))
    srv.listen(128)
    q = queue.Queue(maxsize=queue_size)                         # BLOCKING_QUEUE(queue_size)
    for i in range(1, k + 1):                                   # FOR i ← 1 TO k: SPAWN_THREAD(WORKER, q)
        threading.Thread(target=worker, args=(i, q), daemon=True).start()
    log(f"[server] Đang lắng nghe tại {host}:{port}")
    log(f"[server] Nhóm {k} luồng thợ, hàng đợi {queue_size} chỗ, độ trễ {lookup_service.DELAY_MS} ms")
    try:
        while True:
            conn, a = srv.accept()                              # ACCEPT
            addr = f"{a[0]}:{a[1]}"
            try:
                q.put_nowait((conn, addr))                      # q.try_put: không chờ
            except queue.Full:                                  # hàng đợi đầy
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
