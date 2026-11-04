"""Bài 8.2 – Thùng token giới hạn tần suất.

Cài đặt Mã giả 8.3 – ACQUIRE_TOKENS (Mục 8.2.4, Giáo trình Lập trình mạng).
Thùng dung lượng C, nạp lại R đơn vị/giây; mỗi yêu cầu lấy n đơn vị, thiếu thì chờ.
Chương trình cho nhiều tác vụ đồng thời gửi một chuỗi yêu cầu qua cùng một thùng và
in thời điểm mỗi yêu cầu được phép, so sánh với lý thuyết: C yêu cầu đầu đi ngay,
yêu cầu thứ m > C được phép sau (m − C)/R giây.

Chạy:  python3 thung_token.py [C] [R] [số_yêu_cầu] [số_tác_vụ]      (mặc định 10 2 14 2)
"""
import asyncio
import sys
import time


class Bucket:
    """STRUCT Bucket { cap, rate, tokens, last, lock ← ASYNC_LOCK() }  (Mã giả 8.3, dòng 1)."""

    def __init__(self, cap, rate):
        self.cap = cap
        self.rate = rate
        self.tokens = cap                     # thùng đầy lúc khởi tạo
        self.last = time.monotonic()
        self.lock = asyncio.Lock()            # ASYNC_LOCK()


async def acquire_tokens(b, n):
    """Mã giả 8.3 – ACQUIRE_TOKENS(b, n)."""
    if n > b.cap:                                              # dòng 3
        raise ValueError("n vượt dung lượng")                 # ArgumentError
    while True:                                                # dòng 4: LOOP
        async with b.lock:                                     # dòng 5: WITH b.lock DO
            now = time.monotonic()                             # dòng 6: NOW_SECONDS()
            refill = (now - b.last) * b.rate                   # dòng 7
            b.tokens = min(b.cap, b.tokens + refill)           # dòng 8: không vượt cap
            b.last = now                                       # dòng 9
            if b.tokens >= n:                                  # dòng 10–12
                b.tokens -= n
                return
            wait_ms = (n - b.tokens) / b.rate * 1000           # dòng 13
        await asyncio.sleep(wait_ms / 1000)                    # dòng 15: chờ ngoài khóa


async def worker(b, wid, count, t0, granted):
    """Một tác vụ gửi `count` yêu cầu, mỗi yêu cầu cần 1 đơn vị."""
    for _ in range(count):
        await acquire_tokens(b, 1)
        granted.append((time.monotonic() - t0, wid))           # ghi lại thời điểm được phép
        await asyncio.sleep(0)                                 # nhường lượt: giả lập gửi yêu cầu


async def main():
    args = sys.argv[1:]
    cap = float(args[0]) if len(args) > 0 else 10
    rate = float(args[1]) if len(args) > 1 else 2
    total = int(args[2]) if len(args) > 2 else 14
    tasks = int(args[3]) if len(args) > 3 else 2
    b = Bucket(cap, rate)
    print(f"[bucket] Thùng: dung lượng C = {cap:g}, tốc độ nạp R = {rate:g} đơn vị/giây, mỗi yêu cầu n = 1")
    print(f"[bucket] Gửi {total} yêu cầu từ {tasks} tác vụ đồng thời (thùng đầy lúc bắt đầu)")
    t0, granted = time.monotonic(), []
    counts = [total // tasks + (1 if w < total % tasks else 0) for w in range(tasks)]
    await asyncio.gather(*(worker(b, w + 1, counts[w], t0, granted) for w in range(tasks)))
    for m, (t, wid) in enumerate(sorted(granted), start=1):
        theory = max(0.0, (m - cap) / rate)
        print(f"[bucket] Yêu cầu {m:02d} (tác vụ {wid}) được phép tại t = {t:6.3f} s   lý thuyết {theory:6.3f} s")
    try:
        print(f"[bucket] Thử lấy n = {cap + 1:g} > C = {cap:g} đơn vị")
        await acquire_tokens(b, cap + 1)
    except ValueError as e:                                    # dòng 3: báo lỗi ngay, không chờ mãi
        print(f"[bucket] Lỗi ArgumentError: {e}")


if __name__ == "__main__":
    asyncio.run(main())
