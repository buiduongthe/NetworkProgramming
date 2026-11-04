"""Bài 6.7 – Bộ sinh tải vòng kín đo thông lượng và phân vị độ trễ p50, p95, p99.

Cài đặt Mã giả 6.9 (Mục 6.5.1, Giáo trình Lập trình mạng). Kế thừa Mã giả 5.2 (khởi động N tác vụ
rồi WHEN_ALL = asyncio.gather) và các hàm khung bất đồng bộ của Mã giả 5.1 (async_frame.py).
Chạy lần lượt các mức tải (số client) trong danh sách levels và in bảng kết quả.

Chạy:  python3 load_test.py [host] [port] [n_requests] [levels]   (mặc định 127.0.0.1 9000 20 1,10,100)
"""
import asyncio
import math
import sys
import time

from async_frame import ConnectionClosed, ProtocolError, recv_json_async, send_json_async
from netutil import int_arg, parse_endpoint


def percentile(values, p):
    """PERCENTILE theo hạng gần nhất: giá trị mà p% số mẫu không vượt quá."""
    if not values:
        return 0.0
    s = sorted(values)
    return s[max(0, math.ceil(p / 100 * len(s)) - 1)]


async def load_test(host, port, n_clients, n_requests):
    lat = []                                            # dòng 2: CONCURRENT_LIST (asyncio chạy một luồng)
    errors = 0                                          # errors ← ATOMIC_COUNTER(0)
    req = {"op": "lookup", "term": "TCP"}               # dòng 3

    async def one_client():                             # dòng 4–20
        nonlocal errors
        writer = None                                   # s ← NULL
        try:
            reader, writer = await asyncio.wait_for(asyncio.open_connection(host, port), 3)  # SAFE_CONNECT_ASYNC
            for _ in range(n_requests):                 # dòng 8
                t0 = time.perf_counter()                # NOW_PRECISE()
                await send_json_async(writer, req)
                r = await recv_json_async(reader)
                if r.get("status") != 200:              # dòng 12: ví dụ 503 – server bận
                    errors += 1                         # INCREMENT(errors); BREAK
                    break
                lat.append((time.perf_counter() - t0) * 1000)
        except (OSError, asyncio.TimeoutError, ConnectionClosed, ProtocolError, ValueError):   # dòng 15
            errors += 1                                 # ghi nhận, không dừng cả bài
        finally:
            if writer is not None:                      # dòng 18: IF s ≠ NULL THEN CLOSE(s)
                writer.close()

    t = time.perf_counter()                             # dòng 21
    await asyncio.gather(*[one_client() for _ in range(n_clients)])   # dòng 22: WHEN_ALL
    t = time.perf_counter() - t
    return {"ok": len(lat), "errors": errors, "rps": len(lat) / t, "p50": percentile(lat, 50),
            "p95": percentile(lat, 95), "p99": percentile(lat, 99)}


async def main_async(args):
    host, port = parse_endpoint(args, "127.0.0.1", 9000)
    n_requests = int_arg(args, 2, 20)
    levels = [int(x) for x in (args[3] if len(args) > 3 else "1,10,100").split(",")]
    print(f"[load] Server {host}:{port}, {n_requests} yêu cầu/client, vòng kín", flush=True)
    print(f"[load] {'client':>6} {'OK':>6} {'lỗi':>5} {'yc/giây':>8} {'p50':>7} {'p95':>7} {'p99':>7}",
          flush=True)
    for n in levels:
        r = await load_test(host, port, n, n_requests)
        print(f"[load] {n:>6} {r['ok']:>6} {r['errors']:>5} {r['rps']:>8.1f} {r['p50']:>7.1f} "
              f"{r['p95']:>7.1f} {r['p99']:>7.1f}", flush=True)
    print("[load] OK: số yêu cầu thành công; lỗi: số client gặp lỗi (bị từ chối, mất kết nối, mã ≠ 200)", flush=True)
    print("[load] p50, p95, p99: phân vị độ trễ, tính bằng ms", flush=True)


if __name__ == "__main__":
    asyncio.run(main_async(sys.argv[1:]))
