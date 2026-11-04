"""Bài mở rộng MR5.2 – So sánh các mô hình đồng thời trên cùng một khối việc (Python).

Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1, 5.2 – bất đồng bộ, giới hạn đồng
thời) và Chương 6 (Bài 6.1, 6.2 – server đa luồng, nhóm luồng, bất đồng bộ).

Ba khối việc:
  mang  – 20 yêu cầu mạng tới máy chủ thử (mỗi yêu cầu trễ 200 ms): việc chờ I/O;
  cpu   – 8 tác vụ tính toán thuần Python (mỗi tác vụ ≈ 100 ms): việc nặng CPU;
  tron  – 20 tác vụ = yêu cầu mạng 200 ms + xử lý CPU ≈ 30 ms: bất đồng bộ + nhóm thực thi.
Mỗi mô hình in biểu đồ Gantt (tác vụ nào chạy trên worker nào, khi nào) và tổng thời gian.
Thêm: khoa – tranh chấp dữ liệu và cách sửa bằng khóa, bế tắc và cách tránh (khoa.py).

Chạy:  python3 dong_thoi.py mang <host> <port>
       python3 dong_thoi.py cpu
       python3 dong_thoi.py tron <host> <port>
       python3 dong_thoi.py khoa
"""
import asyncio
import concurrent.futures as cf
import os
import socket
import sys
import threading
import time

import khoa
from dong_ho import Recorder, axis, gantt, table

N_NET, N_CPU = 20, 8
PAUSE = 0.5          # nghỉ giữa hai mô hình để máy chủ thử báo từng "đợt" riêng


# ---------------- việc mạng (chặn) và việc CPU ----------------
def fetch_blocking(host, port, k):
    """Một yêu cầu mạng kiểu chặn (blocking): kết nối, gửi GET, chờ dòng trả lời."""
    with socket.create_connection((host, port), timeout=5) as s:
        s.sendall(f"GET {k}\n".encode())
        return s.makefile().readline().strip()


async def fetch_async(host, port, k):
    """Cùng yêu cầu nhưng bất đồng bộ: trong lúc chờ, vòng lặp sự kiện chạy tác vụ khác."""
    reader, writer = await asyncio.open_connection(host, port)
    writer.write(f"GET {k}\n".encode())
    await writer.drain()
    line = (await reader.readline()).decode().strip()
    writer.close()
    await writer.wait_closed()
    return line


def cpu_work(n):
    """Việc tính toán thuần Python (bộ sinh đồng dư tuyến tính): giữ GIL suốt thời gian chạy."""
    x = 1
    for _ in range(n):
        x = (x * 1103515245 + 12345) & 0x7FFFFFFF
    return x


def calibrate(ms):
    """Tìm số vòng lặp để cpu_work chạy ≈ ms mili giây trên máy này."""
    n, dt = 1_000_000, 1.0
    for _ in range(3):                    # đo 3 lần, lấy lần nhanh nhất (lần đầu thường chậm hơn)
        t = time.perf_counter()
        cpu_work(n)
        dt = min(dt, time.perf_counter() - t)
    return int(n * ms / 1000 / dt)


def cpu_task(k, n):
    """Chạy trong tiến trình con: trả về (pid, bắt đầu, kết thúc) để vẽ biểu đồ."""
    t = time.perf_counter()
    cpu_work(n)
    return os.getpid(), t, time.perf_counter()


class Names:
    """Đặt tên dễ đọc cho worker: luồng-1, luồng-2, ... theo thứ tự xuất hiện."""

    def __init__(self, prefix):
        self.prefix, self.map, self.lock = prefix, {}, threading.Lock()

    def get(self, key):
        with self.lock:
            if key not in self.map:
                self.map[key] = f"{self.prefix}-{len(self.map) + 1}"
            return self.map[key]


# ---------------- khối việc 1: mạng ----------------
def run_net(host, port):
    print(f"Khối việc: {N_NET} yêu cầu mạng, mỗi yêu cầu máy chủ trễ 200 ms")
    results = []

    def timed(rec, names, k):
        w = names.get(threading.get_ident())          # đặt tên worker lúc bắt đầu tác vụ
        t = time.perf_counter()
        fetch_blocking(host, port, k)
        rec.add(k, w, t, time.perf_counter())

    # 1. Tuần tự (đồng bộ)
    rec = Recorder()
    for k in range(N_NET):
        t = time.perf_counter()
        fetch_blocking(host, port, k)
        rec.add(k, "chính", t, time.perf_counter())
    results.append(("1. tuần tự", rec, "luồng"))
    time.sleep(PAUSE)

    # 2. Mỗi tác vụ một luồng
    rec, names = Recorder(), Names("luồng")
    ths = [threading.Thread(target=timed, args=(rec, names, k)) for k in range(N_NET)]
    for th in ths:
        th.start()
    for th in ths:
        th.join()
    results.append(("2. mỗi tác vụ một luồng", rec, "luồng"))
    time.sleep(PAUSE)

    # 3. Nhóm luồng (thread pool) 5 luồng
    rec, names = Recorder(), Names("luồng")
    with cf.ThreadPoolExecutor(max_workers=5) as pool:
        list(pool.map(lambda k: timed(rec, names, k), range(N_NET)))
    results.append(("3. nhóm luồng (5 luồng)", rec, "luồng"))
    time.sleep(PAUSE)

    # 4. Bất đồng bộ asyncio: một luồng, một vòng lặp sự kiện, 20 coroutine
    async def all_async(rec):
        async def one(k):
            t = time.perf_counter()
            await fetch_async(host, port, k)
            rec.add(k, "vòng lặp", t, time.perf_counter())
        await asyncio.gather(*(one(k) for k in range(N_NET)))
    rec = Recorder()
    asyncio.run(all_async(rec))
    results.append(("4. asyncio (1 luồng)", rec, "luồng"))
    show(results)
    print("Nhận xét: việc chờ I/O không tốn CPU, nên luồng hay coroutine đều chồng")
    print("  được thời gian chờ; asyncio làm được trên một luồng duy nhất (*).")


def show(results):
    scale = max(r.total() for _, r, _ in results)
    print()
    for name, rec, kind in results:
        gantt(name, rec, rec.total(), scale, kind)
    axis(scale)
    print()
    table([(name, rec.total()) for name, rec, _ in results])
    print()


# ---------------- khối việc 2: CPU ----------------
def run_cpu():
    n = calibrate(100)
    gil = sys._is_gil_enabled() if hasattr(sys, "_is_gil_enabled") else True
    ncpu = os.cpu_count() or 1
    print(f"Khối việc: {N_CPU} tác vụ CPU thuần Python, mỗi tác vụ ≈ 100 ms")
    print(f"Máy có {ncpu} CPU; GIL {'đang bật' if gil else 'đã tắt (bản free-threaded)'}")
    results = []
    rec = Recorder()
    for k in range(N_CPU):
        t = time.perf_counter()
        cpu_work(n)
        rec.add(k, "chính", t, time.perf_counter())
    results.append(("1. tuần tự", rec, "luồng"))

    rec, names = Recorder(), Names("luồng")

    def timed(k):
        w = names.get(threading.get_ident())
        t = time.perf_counter()
        cpu_work(n)
        rec.add(k, w, t, time.perf_counter())
    with cf.ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(timed, range(N_CPU)))
    results.append(("2. nhóm luồng (4 luồng)", rec, "luồng"))

    with cf.ProcessPoolExecutor(max_workers=ncpu) as pool:
        list(pool.map(cpu_task, range(ncpu), [1] * ncpu))       # khởi động trước các tiến trình
        rec, names = Recorder(), Names("tiến trình")
        for k, (pid, t1, t2) in enumerate(pool.map(cpu_task, range(N_CPU), [n] * N_CPU)):
            rec.add(k, names.get(pid), t1, t2)
    results.append((f"3. nhóm tiến trình ({ncpu} t.trình)", rec, "tiến trình"))
    show(results)
    print("Nhận xét: với GIL, mỗi lúc chỉ một luồng chạy mã Python, nên nhóm luồng")
    print("  không nhanh hơn tuần tự (mỗi tác vụ bị kéo dài vì phải chia lượt);")
    print("  nhóm tiến trình có GIL riêng cho từng tiến trình nên chạy song song")
    print("  thật trên nhiều CPU.")


# ---------------- khối việc 3: mạng + CPU ----------------
def run_mixed(host, port):
    n = calibrate(30)
    ncpu = os.cpu_count() or 1
    print(f"Khối việc: {N_NET} tác vụ = yêu cầu mạng 200 ms + xử lý CPU ≈ 30 ms")
    print("Đo thêm độ trễ vòng lặp sự kiện: coroutine \"nhịp tim\" ngủ 10 ms và")
    print("  đo xem thực tế bị đánh thức muộn bao nhiêu (trễ lớn = ứng dụng đứng).")

    async def scenario(mode, rec, pool=None):
        loop = asyncio.get_running_loop()
        lag = {"max": 0.0, "stop": False}
        names = Names("tiến trình" if mode == "process" else "luồng")

        async def heartbeat():
            while not lag["stop"]:
                t = time.perf_counter()
                await asyncio.sleep(0.01)
                lag["max"] = max(lag["max"], time.perf_counter() - t - 0.01)

        def job(k):                                   # chạy trong luồng của nhóm thực thi
            w = names.get(threading.get_ident())
            t1 = time.perf_counter()
            cpu_work(n)
            rec.add(k, w, t1, time.perf_counter())

        async def one(k):
            t = time.perf_counter()
            await fetch_async(host, port, k)
            rec.add(k, "vòng lặp", t, time.perf_counter())
            if mode == "inline":                      # CPU chạy ngay trong vòng lặp: chặn mọi thứ
                t = time.perf_counter()
                cpu_work(n)
                rec.add(k, "vòng lặp", t, time.perf_counter())
            elif mode == "to_thread":                 # nhóm luồng mặc định của asyncio
                await asyncio.to_thread(job, k)
            elif mode == "thread":                    # nhóm luồng riêng, ít luồng
                await loop.run_in_executor(pool, job, k)
            else:                                     # nhóm tiến trình: song song thật
                pid, t1, t2 = await loop.run_in_executor(pool, cpu_task, k, n)
                rec.add(k, names.get(pid), t1, t2)

        hb = asyncio.create_task(heartbeat())
        await asyncio.gather(*(one(k) for k in range(N_NET)))
        lag["stop"] = True
        await hb
        return lag["max"]

    results, lags = [], []
    rec = Recorder()
    lags.append(asyncio.run(scenario("inline", rec)))
    results.append(("A. CPU ngay trong vòng lặp", rec, "luồng"))
    time.sleep(PAUSE)
    rec = Recorder()
    lags.append(asyncio.run(scenario("to_thread", rec)))
    results.append(("B. asyncio.to_thread", rec, "luồng"))
    time.sleep(PAUSE)
    with cf.ThreadPoolExecutor(max_workers=2) as pool:
        rec = Recorder()
        lags.append(asyncio.run(scenario("thread", rec, pool)))
    results.append(("C. run_in_executor, 2 luồng", rec, "luồng"))
    time.sleep(PAUSE)
    with cf.ProcessPoolExecutor(max_workers=ncpu) as pool:
        list(pool.map(cpu_task, range(ncpu), [1] * ncpu))   # khởi động trước các tiến trình
        rec = Recorder()
        lags.append(asyncio.run(scenario("process", rec, pool)))
    results.append((f"D. run_in_executor, {ncpu} t.trình", rec, "tiến trình"))
    scale = max(r.total() for _, r, _ in results)
    print()
    for name, rec, kind in results:
        gantt(name, rec, rec.total(), scale, kind)
    axis(scale)
    print()
    print(f"  {'Mô hình':<30} {'Thời gian':>9} {'Trễ vòng lặp':>13}")
    for (name, rec, _), lag in zip(results, lags):
        print(f"  {name:<30} {rec.total():>7.2f} s {lag * 1000:>10.0f} ms")
    print()
    print("Nhận xét: A – xử lý CPU chặn vòng lặp sự kiện, nhịp tim trễ hàng trăm")
    print("  ms. B, C – đẩy CPU sang luồng khác thì vòng lặp được chạy xen, nhưng")
    print("  vì GIL nên CPU vẫn tuần tự, và càng nhiều luồng tranh GIL (B dùng")
    print(f"  nhóm mặc định {min(32, ncpu + 4)} luồng) thì vòng lặp càng chờ lâu. D – tiến trình")
    print("  riêng: vòng lặp thông suốt và CPU chạy song song.")


def main():
    if len(sys.argv) >= 4 and sys.argv[1] in ("mang", "tron"):
        host, port = sys.argv[2], int(sys.argv[3])
        (run_net if sys.argv[1] == "mang" else run_mixed)(host, port)
    elif len(sys.argv) >= 2 and sys.argv[1] == "cpu":
        run_cpu()
    elif len(sys.argv) >= 2 and sys.argv[1] == "khoa":
        khoa.run()
    else:
        print(__doc__)
        sys.exit(1)


if __name__ == "__main__":
    main()
