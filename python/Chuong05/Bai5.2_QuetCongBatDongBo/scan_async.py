"""Bài 5.2 – Kiểm tra nhiều cổng đồng thời có giới hạn (asyncio).

Cài đặt Mã giả 5.2 (Mục 5.2.5, Giáo trình Lập trình mạng): SCAN_PORTS, CHECK_ONE.
Kế thừa Mã giả 1.2 (PORT_CHECK): PORT_CHECK_ASYNC có cùng thân hàm, CONNECT được thay bằng
AWAIT CONNECT_ASYNC. Chương trình quét cùng một dải cổng hai lần: tuần tự (max_parallel = 1)
và đồng thời (max_parallel = k), rồi in thời gian của mỗi lần.
Chỉ chạy trên máy thuộc quyền quản lý hoặc mạng thí nghiệm cô lập (Mục 1.2.5).

Chạy:  python3 scan_async.py [host] [cổng_đầu] [cổng_cuối] [max_parallel]
       (mặc định 127.0.0.1 9000 9019 10)
"""
import asyncio
import socket
import sys
import time


async def port_check_async(host, port, timeout_ms):        # Mã giả 1.2 + AWAIT
    loop = asyncio.get_running_loop()
    try:
        addrs = await loop.getaddrinfo(host, port, type=socket.SOCK_STREAM)   # RESOLVE_HOST
    except socket.gaierror:
        return "UNRESOLVED"
    state = "FILTERED"                                     # chưa địa chỉ nào phản hồi
    for _, _, _, _, addr in addrs:
        try:
            _, writer = await asyncio.wait_for(             # AWAIT CONNECT_ASYNC, CONNECT_TIMEOUT
                asyncio.open_connection(addr[0], addr[1]), timeout_ms / 1000)
            writer.close()                                 # FINALLY CLOSE(s)
            return "OPEN"                                  # bắt tay ba bước thành công
        except ConnectionRefusedError:
            state = "CLOSED"                               # nhận gói RST
        except (TimeoutError, OSError):
            continue                                       # thử địa chỉ tiếp theo
    return state


async def check_one(host, p, gate):                        # Mã giả 5.2, dòng 9–16
    async with gate:                                       # ACQUIRE_ASYNC ... FINALLY RELEASE
        return p, await port_check_async(host, p, 1000)


async def scan_ports(host, ports, max_parallel):           # Mã giả 5.2, dòng 1–8
    gate = asyncio.Semaphore(max_parallel)                 # giới hạn số kết nối
    tasks = [check_one(host, p, gate) for p in ports]      # chưa chờ
    return await asyncio.gather(*tasks)                    # WHEN_ALL: chờ tất cả hoàn tất


async def timed_scan(host, ports, k):
    t0 = time.perf_counter()
    result = await scan_ports(host, ports, k)
    return result, time.perf_counter() - t0


async def main():
    a = sys.argv[1:]
    host = a[0] if len(a) > 0 else "127.0.0.1"
    first = int(a[1]) if len(a) > 1 else 9000
    last = int(a[2]) if len(a) > 2 else 9019
    k = int(a[3]) if len(a) > 3 else 10
    ports = list(range(first, last + 1))
    print(f"[scan] Quét {len(ports)} cổng {first}–{last} trên {host}, thời gian chờ 1000 ms", flush=True)
    seq, t_seq = await timed_scan(host, ports, 1)
    par, t_par = await timed_scan(host, ports, k)
    for p, state in par:
        if state != "CLOSED":                              # chỉ in cổng không đóng
            print(f"[scan]   cổng {p}: {state}", flush=True)
    counts = {s: sum(1 for _, x in par if x == s) for s in ("OPEN", "CLOSED", "FILTERED")}
    print(f"[scan] Tổng hợp: OPEN={counts['OPEN']}, CLOSED={counts['CLOSED']}, "
          f"FILTERED={counts['FILTERED']}", flush=True)
    print(f"[scan] Hai lần quét cho cùng kết quả: {'có' if seq == par else 'không'}", flush=True)
    print(f"[scan] Tuần tự   (max_parallel=1):  {t_seq:.2f} s", flush=True)
    print(f"[scan] Đồng thời (max_parallel={k}): {t_par:.2f} s", flush=True)


if __name__ == "__main__":
    asyncio.run(main())
