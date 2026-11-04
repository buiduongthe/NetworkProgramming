"""Bài mở rộng MR5.2 – Tranh chấp dữ liệu (race condition), khóa, và tránh bế tắc (deadlock).

1. Tranh chấp: 4 luồng cùng "đọc – làm việc khác – ghi" lên một biến đếm chung. Bước "làm việc khác"
   (ở đây time.sleep(0): nhường CPU, giống như gọi mạng hay ghi nhật ký giữa chừng) cho phép luồng
   khác chen vào giữa lúc đọc và lúc ghi → mất lần cộng. Sửa bằng threading.Lock.
2. Bế tắc: luồng T1 giữ khóa X rồi chờ khóa Y, luồng T2 giữ Y rồi chờ X – cả hai chờ nhau mãi.
   Bài dùng acquire(timeout=...) để phát hiện thay vì treo chương trình; cách tránh là luôn lấy khóa
   theo cùng một thứ tự.
"""
import threading
import time

N_THREADS, N_ITER = 4, 5_000


def race_demo():
    print(f"Tranh chấp dữ liệu: {N_THREADS} luồng × {N_ITER} lần \"đọc – nhường CPU – ghi\"")
    print("  lên một biến đếm chung (kết quả đúng phải là "
          f"{N_THREADS * N_ITER}).")

    class Counter:
        value = 0

    def unsafe(c):
        for _ in range(N_ITER):
            v = c.value            # đọc
            time.sleep(0)          # nhường CPU: luồng khác có thể chen vào đây
            c.value = v + 1        # ghi – có thể đè lên kết quả của luồng khác

    def safe(c, lock):
        for _ in range(N_ITER):
            with lock:             # đoạn găng: chỉ một luồng được vào
                v = c.value
                time.sleep(0)
                c.value = v + 1

    for name, target, extra in [("không khóa", unsafe, ()), ("có khóa (Lock)", safe, (threading.Lock(),))]:
        c = Counter()
        t = time.perf_counter()
        ths = [threading.Thread(target=target, args=(c, *extra)) for _ in range(N_THREADS)]
        for th in ths:
            th.start()
        for th in ths:
            th.join()
        dt = time.perf_counter() - t
        total = N_THREADS * N_ITER
        verdict = "đúng" if c.value == total else f"mất {total - c.value} lần cộng"
        print(f"  {name:<16}: {c.value:>6} / {total} → {verdict} ({dt:.2f} s)")


def deadlock_demo():
    print()
    print("Bế tắc (deadlock): T1 chuyển X→Y, T2 chuyển Y→X, mỗi luồng cần cả")
    print("  hai khóa; số dư ban đầu X = 100, Y = 100.")
    bal = {"X": 100, "Y": 100}
    locks = {"X": threading.Lock(), "Y": threading.Lock()}
    out_lock = threading.Lock()

    def say(text):
        with out_lock:
            print(f"  {text}", flush=True)

    def transfer(name, src, dst, amount, ordered):
        first, second = (sorted([src, dst]) if ordered else (src, dst))
        with locks[first]:
            say(f"{name} giữ khóa {first}, chờ khóa {second}")
            time.sleep(0.05)                       # để luồng kia kịp giữ khóa còn lại
            if not locks[second].acquire(timeout=0.5):
                say(f"{name}: chờ {second} quá 0,5 s → nghi bế tắc, nhả {first}, bỏ cuộc")
                return
            try:
                bal[src] -= amount
                bal[dst] += amount
                say(f"{name}: đã chuyển {amount} từ {src} sang {dst}")
            finally:
                locks[second].release()

    for ordered in (False, True):
        if ordered:
            print("Cách tránh: mọi luồng lấy khóa theo cùng thứ tự (X trước, Y sau):")
        t1 = threading.Thread(target=transfer, args=("T1", "X", "Y", 10, ordered))
        t2 = threading.Thread(target=transfer, args=("T2", "Y", "X", 5, ordered))
        t1.start()
        time.sleep(0.01)
        t2.start()
        t1.join()
        t2.join()
        print(f"  Số dư: X = {bal['X']}, Y = {bal['Y']} (tổng {bal['X'] + bal['Y']})")


def run():
    race_demo()
    deadlock_demo()
