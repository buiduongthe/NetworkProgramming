"""Bài mở rộng MR5.2 – Ghi lại khoảng thời gian chạy của từng tác vụ và vẽ biểu đồ Gantt bằng văn bản.

Mỗi đoạn ghi (tác vụ, worker, bắt đầu, kết thúc) theo đồng hồ time.perf_counter() – đồng hồ đơn điệu
dùng chung cho mọi luồng và tiến trình trên cùng máy. Mỗi hàng của biểu đồ là một worker (luồng, tiến
trình, vòng lặp sự kiện); mỗi cột là một khoảng thời gian bằng nhau:
    chữ cái  = đúng một tác vụ đang chạy trên worker đó (A = tác vụ 1, B = tác vụ 2, ...)
    *        = nhiều tác vụ cùng lúc trên một worker (đặc trưng của mô hình bất đồng bộ)
Ký tự của một cột lấy theo thời điểm giữa cột.
"""
import threading
import time

COLS = 48
LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"


class Recorder:
    def __init__(self):
        self.t0 = time.perf_counter()
        self.segs = []                      # (tác vụ, worker, bắt đầu, kết thúc) – giây kể từ t0
        self.lock = threading.Lock()

    def add(self, task, worker, start, end):
        """start, end: giá trị time.perf_counter() tuyệt đối."""
        with self.lock:
            self.segs.append((task, worker, start - self.t0, end - self.t0))

    def total(self):
        return max(s[3] for s in self.segs)


def gantt(title, rec, total, scale, kind="worker", max_rows=8):
    print(f"{title}: {total:.2f} s")
    workers = []
    for s in sorted(rec.segs, key=lambda s: s[2]):
        if s[1] not in workers:
            workers.append(s[1])
    for w in workers[:max_rows]:
        mine = [s for s in rec.segs if s[1] == w]
        cells = []
        for c in range(COLS):
            lo, hi = scale * c / COLS, scale * (c + 1) / COLS
            mid = (lo + hi) / 2
            now = [s for s in mine if s[2] <= mid < s[3]]          # đang chạy giữa cột
            if len(now) >= 2:
                cells.append("*")                                  # nhiều tác vụ cùng lúc
            elif now:
                cells.append(LETTERS[now[0][0]])
            else:                                                  # tác vụ ngắn hơn một cột
                on = [s for s in mine if s[2] < hi and s[3] > lo]
                cells.append(LETTERS[on[0][0]] if on else " ")
        print(f"  {w:<12}|{''.join(cells)}|")
    if len(workers) > max_rows:
        print(f"  … và {len(workers) - max_rows} {kind} khác, mỗi {kind} chạy 1 tác vụ")


def axis(scale):
    right = f"{scale:.1f} s"
    print(f"  {'':<12} 0{right:>{COLS}}")


def table(rows):
    """rows: [(tên mô hình, thời gian)]; hàng đầu là mô hình tuần tự (mốc so sánh)."""
    base = rows[0][1]
    print(f"  {'Mô hình':<30} {'Thời gian':>9} {'Tăng tốc':>9}")
    for name, t in rows:
        print(f"  {name:<30} {t:>7.2f} s {base / t:>8.1f}×")
