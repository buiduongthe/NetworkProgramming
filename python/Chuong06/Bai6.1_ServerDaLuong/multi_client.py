"""Bài 6.1 – Mở nhiều client tra cứu đồng thời để quan sát server đa luồng và nhóm luồng.

Mỗi client chạy trong một luồng, thực hiện LOOKUP_CLIENT (Mã giả 3.8) rút gọn: gửi một yêu cầu
lookup "TCP", in phản hồi kèm thời điểm t tính từ lúc chương trình bắt đầu, giữ kết nối hold_ms
(mô phỏng người dùng chưa rời đi) rồi gửi bye. Các client khởi động cách nhau 100 ms.

Chạy:  python3 multi_client.py [host] [port] [n] [hold_ms]   (mặc định 127.0.0.1 9000 5 1000)
"""
import socket
import sys
import threading
import time

from netutil import ConnectionClosed, int_arg, parse_endpoint, recv_json, safe_connect, send_json

_print_lock = threading.Lock()


def log(msg):
    """In một dòng trọn vẹn (các luồng không chen giữa dòng của nhau)."""
    with _print_lock:
        print(msg, flush=True)


def one_client(i, host, port, hold_ms, t0):
    def t():
        return round((time.perf_counter() - t0) * 1000)
    try:
        s = safe_connect(host, port, 3000)
    except ConnectionError as e:
        log(f"[client {i}] Lỗi: {e}")
        return
    s.settimeout(5.0)                                           # RECV_TIMEOUT 5000 ms
    try:
        send_json(s, {"op": "lookup", "term": "TCP"})
        resp = recv_json(s)
        if resp.get("status") != 200:
            log(f"[client {i}] t = {t()} ms: bị từ chối {resp.get('status')} – {resp.get('error')}")
            return
        log(f"[client {i}] t = {t()} ms: 200 {resp['term']} – {resp['text']}")
        time.sleep(hold_ms / 1000)                              # giữ kết nối
        send_json(s, {"op": "bye"})
    except (ConnectionClosed, ConnectionResetError):            # server đóng ngay sau ACCEPT
        log(f"[client {i}] t = {t()} ms: server đóng kết nối ngay")
    except OSError as e:
        log(f"[client {i}] Lỗi: {e}")
    finally:
        s.close()


def main():
    host, port = parse_endpoint(sys.argv[1:], "127.0.0.1", 9000)
    n = int_arg(sys.argv, 3, 5)
    hold_ms = int_arg(sys.argv, 4, 1000)
    print(f"[client] {n} client cách nhau 100 ms, giữ kết nối {hold_ms} ms", flush=True)
    t0 = time.perf_counter()
    threads = []
    for i in range(1, n + 1):
        th = threading.Thread(target=one_client, args=(i, host, port, hold_ms, t0))
        th.start()
        threads.append(th)
        time.sleep(0.1)
    for th in threads:
        th.join()
    print(f"[client] Hoàn tất sau {round((time.perf_counter() - t0) * 1000)} ms", flush=True)


if __name__ == "__main__":
    main()
