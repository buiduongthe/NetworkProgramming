"""Bài mở rộng MR5.3 – Giao diện luôn phản hồi khi nhiều tác vụ mạng chạy song song (Tkinter).

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 5.3 (giao diện bất đồng bộ), Bài 5.5 (tải tệp có
tiến trình và hủy), Bài 5.2 (giới hạn đồng thời), Bài 1.2 (kiểm tra cổng), Bài 4.3 (tra cứu tên qua UDP).

Quy tắc vàng của lập trình giao diện: CHỈ luồng giao diện được chạm vào widget, và luồng giao diện
KHÔNG BAO GIỜ được chờ mạng. Vì vậy:
  - mỗi tác vụ chạy trên một luồng nền, xin lượt qua bộ giới hạn đồng thời (thanh trượt);
  - luồng nền chỉ đưa thông điệp vào queue.Queue (an toàn luồng);
  - luồng giao diện cứ 30 ms dùng after() lấy thông điệp ra và cập nhật thanh tiến trình, nhật ký;
  - đồng hồ và hoạt hình chạy bằng after(20 ms); khoảng cách thật giữa hai nhịp được đo lại – nếu
    luồng giao diện bị chặn, khoảng cách này tăng vọt (giao diện "đứng").
Nút "Làm SAI" chạy đúng việc tải tệp đó ngay trên luồng giao diện để thấy giao diện đứng bao lâu.

Chạy:  python3 ung_dung.py <host> <port> [--demo]
       (--demo: tự bấm các nút theo kịch bản, chụp ảnh cửa sổ thành ket_qua_gui.png rồi thoát)
"""
import math
import queue
import subprocess
import sys
import threading
import time
import tkinter as tk
from tkinter import ttk

from tac_vu import Cancelled, Limiter, check_ports, download, lookup_names

TICK_MS = 20
FILES = [(256, 60), (256, 90), (384, 70), (256, 120), (320, 50), (256, 100), (192, 80), (256, 40)]
NAMES = ["router.lab", "may-in.lab", "camera-1.lab", "nas.lab", "khong-co.lab"]


class Task:
    def __init__(self, tid, name):
        self.id, self.name = tid, name
        self.cancel = threading.Event()
        self.done = False
        self.widgets = []


class App:
    def __init__(self, root, host, port):
        self.root, self.host, self.port = root, host, port
        self.q = queue.Queue()
        self.limiter = Limiter(3)
        self.tasks, self.next_id = [], 1
        self.t0 = self.last = time.perf_counter()
        self.ticks = 0
        self.gaps = {}                       # giai đoạn -> [số nhịp, tổng, lớn nhất] (ms)
        self.bad_pending = False
        self.skip_gap = False                # bỏ qua một nhịp (kịch bản minh họa đang chụp ảnh)
        root.title("MR5.3 – Giao diện đa tác vụ (Tkinter)")
        root.geometry("980x680")
        self.build()
        self.tick()
        self.poll()

    # ---------- dựng giao diện ----------
    def build(self):
        top = ttk.Frame(self.root, padding=8)
        top.pack(fill="x")
        self.lbl_clock = ttk.Label(top, font=("DejaVu Sans Mono", 18, "bold"))
        self.lbl_clock.pack(side="left")
        self.canvas = tk.Canvas(top, width=260, height=36, bg="#f4f6fa", highlightthickness=0)
        self.canvas.pack(side="left", padx=12)
        self.ball = self.canvas.create_oval(0, 8, 20, 28, fill="#2457c5", outline="")
        self.arc = self.canvas.create_arc(222, 2, 254, 34, start=0, extent=270, style="arc",
                                          outline="#e07b00", width=4)
        self.lbl_stats = ttk.Label(self.root, padding=(8, 0), foreground="#333", justify="left",
                                   text="Khoảng cách giữa hai nhịp đồng hồ (đặt 20 ms): đang đo…")
        self.lbl_stats.pack(fill="x")

        bar = ttk.Frame(self.root, padding=8)
        bar.pack(fill="x")
        self.spin_n = ttk.Spinbox(bar, from_=1, to=len(FILES), width=3)
        self.spin_n.set(5)
        self.spin_n.pack(side="left")
        self.btn_dl = ttk.Button(bar, text="Tải tệp", command=self.start_downloads)
        self.btn_dl.pack(side="left", padx=(2, 8))
        self.btn_ports = ttk.Button(bar, text="Kiểm tra cổng", command=self.start_ports)
        self.btn_ports.pack(side="left", padx=4)
        self.btn_dns = ttk.Button(bar, text="Tra cứu tên", command=self.start_dns)
        self.btn_dns.pack(side="left", padx=4)
        ttk.Label(bar, text="Giới hạn đồng thời").pack(side="left", padx=(16, 4))
        self.var_limit = tk.IntVar(value=3)
        self.scale = tk.Scale(bar, from_=1, to=8, orient="horizontal", length=120, showvalue=True,
                              variable=self.var_limit, command=lambda v: self.limiter.set(int(v)))
        self.scale.pack(side="left")
        self.btn_cancel_all = ttk.Button(bar, text="Hủy tất cả", command=self.cancel_all)
        self.btn_cancel_all.pack(side="left", padx=8)
        self.btn_bad = tk.Button(bar, text="Làm SAI (tải trên luồng GUI)", bg="#f6d5d9",
                                 command=self.bad)
        self.btn_bad.pack(side="left", padx=4)

        self.frm_tasks = ttk.LabelFrame(self.root, text="Tác vụ", padding=6)
        self.frm_tasks.pack(fill="x", padx=8)
        frm_log = ttk.LabelFrame(self.root, text="Nhật ký", padding=4)
        frm_log.pack(fill="both", expand=True, padx=8, pady=8)
        self.txt_log = tk.Text(frm_log, height=8, font=("DejaVu Sans Mono", 9), state="disabled")
        self.txt_log.pack(fill="both", expand=True)

    # ---------- nhịp đồng hồ: phải luôn chạy đều ----------
    def tick(self):
        now = time.perf_counter()
        gap = (now - self.last) * 1000
        self.last = now
        if self.skip_gap:
            self.skip_gap = False
        elif self.ticks > 0:
            if self.bad_pending:
                phase, self.bad_pending = "luồng GUI bị chặn", False
            elif any(not t.done for t in self.tasks):
                phase = "chạy song song"
            else:
                phase = "rỗi"
            g = self.gaps.setdefault(phase, [0, 0.0, 0.0])
            g[0], g[1], g[2] = g[0] + 1, g[1] + gap, max(g[2], gap)
        self.ticks += 1
        lt = time.localtime()
        self.lbl_clock.config(text=time.strftime("%H:%M:%S", lt) + f".{int(now * 1000) % 1000:03d}")
        x = 110 + 100 * math.sin(now * 3)                 # quả bóng chạy qua lại
        self.canvas.coords(self.ball, x, 8, x + 20, 28)
        self.canvas.itemconfig(self.arc, start=(now * 360) % 360)
        self.lbl_stats.config(text=self.stats_text())
        self.root.after(TICK_MS, self.tick)

    def stats_text(self):
        parts = []
        for phase in ("chạy song song", "luồng GUI bị chặn"):
            if phase in self.gaps:
                n, s, mx = self.gaps[phase]
                parts.append(f"  • {phase}: tối đa {mx:.0f} ms, trung bình {s / n:.1f} ms ({n} nhịp)")
        return "\n".join(["Khoảng cách thật giữa hai nhịp đồng hồ (đặt 20 ms):"] + (parts or ["  đang đo…"]))

    # ---------- nhận thông điệp từ luồng nền ----------
    def poll(self):
        try:
            while True:
                msg = self.q.get_nowait()
                kind, task = msg[0], msg[1]
                if kind == "progress":
                    task.widgets[1]["value"] = msg[2]
                    task.widgets[2].config(text=msg[3])
                elif kind == "state":
                    task.widgets[2].config(text=msg[2])
                elif kind == "done":
                    task.done = True
                    task.widgets[2].config(text=msg[2])
                    task.widgets[3].state(["disabled"])
                    self.log(f"{task.name}: {msg[2]}")
        except queue.Empty:
            pass
        self.root.after(30, self.poll)

    def log(self, text):
        line = f"{time.perf_counter() - self.t0:6.2f} s  {text}"
        print(f"[gui] {line}", flush=True)
        self.txt_log.config(state="normal")
        self.txt_log.insert("end", line + "\n")
        self.txt_log.see("end")
        self.txt_log.config(state="disabled")

    # ---------- tạo và chạy tác vụ ----------
    def clear_finished(self):
        for t in [t for t in self.tasks if t.done]:
            for w in t.widgets:
                w.destroy()
            self.tasks.remove(t)

    def spawn(self, name, job, *args):
        t = Task(self.next_id, name)
        self.next_id += 1
        row = len(self.tasks)
        lbl = ttk.Label(self.frm_tasks, text=f"#{t.id} {name}", width=22)
        bar = ttk.Progressbar(self.frm_tasks, length=300, maximum=100)
        st = ttk.Label(self.frm_tasks, text="chờ lượt", width=34)
        btn = ttk.Button(self.frm_tasks, text="Hủy", width=5, command=t.cancel.set)
        for col, w in enumerate((lbl, bar, st, btn)):
            w.grid(row=row, column=col, padx=3, pady=1, sticky="w")
        t.widgets = [lbl, bar, st, btn]
        self.tasks.append(t)
        threading.Thread(target=self.run_task, args=(t, job, args), daemon=True).start()
        return t

    def run_task(self, t, job, args):
        """Chạy trên luồng nền: xin lượt, làm việc, báo kết quả qua hàng đợi."""
        def progress(pct, text):
            self.q.put(("progress", t, pct, text))
        if not self.limiter.acquire(t.cancel):
            self.q.put(("done", t, "đã hủy (khi đang chờ lượt)"))
            return
        try:
            self.q.put(("state", t, "đang chạy…"))
            result = job(*args, t.cancel, progress)
            self.q.put(("done", t, result))
        except Cancelled:
            self.q.put(("done", t, "đã hủy"))
        except OSError as e:
            self.q.put(("done", t, f"lỗi: {e}"))
        finally:
            self.limiter.release()

    def start_downloads(self):
        self.clear_finished()
        n = int(self.spin_n.get())
        self.log(f"Bấm Tải tệp: {n} tệp, giới hạn đồng thời {self.var_limit.get()}")
        for kb, delay in FILES[:n]:
            self.spawn(f"Tải {kb} KB ({delay} ms/khối)", download, self.host, self.port, kb, delay)

    def start_ports(self):
        self.clear_finished()
        ports = [self.port, self.port + 1, self.port + 2, self.port + 3]
        self.log(f"Bấm Kiểm tra cổng: {', '.join(map(str, ports))}")
        self.spawn("Kiểm tra 4 cổng", check_ports, self.host, ports)

    def start_dns(self):
        self.clear_finished()
        self.log(f"Bấm Tra cứu tên: {len(NAMES)} tên qua UDP")
        self.spawn(f"Tra cứu {len(NAMES)} tên", lookup_names, self.host, self.port, NAMES)

    def cancel_all(self):
        running = [t for t in self.tasks if not t.done]
        self.log(f"Bấm Hủy tất cả: {len(running)} tác vụ")
        for t in running:
            t.cancel.set()

    def bad(self):
        """CÁCH SAI: chờ mạng ngay trên luồng giao diện – cả cửa sổ đứng cho tới khi xong."""
        self.log("Bấm Làm SAI: tải 256 KB ngay trên luồng giao diện…")
        t = time.perf_counter()
        try:
            download(self.host, self.port, 256, 80, threading.Event(), lambda pct, text: None)
        except OSError as e:
            self.log(f"lỗi: {e}")
        dt = (time.perf_counter() - t) * 1000
        self.bad_pending = True
        self.log(f"Luồng giao diện bị chặn {dt:.0f} ms: đồng hồ, nút, thanh tiến trình đứng yên")

    def close(self):
        for t in self.tasks:
            t.cancel.set()
        self.root.destroy()


# ---------------- kịch bản minh họa tự động (--demo) ----------------
def demo(app, out="ket_qua_gui.png"):
    def snapshot(path):
        app.root.update()
        subprocess.run(["import", "-window", app.root.wm_frame(), path], check=True)
        app.skip_gap = True                       # thời gian chụp ảnh không tính vào thống kê

    def running():
        return [t for t in app.tasks if not t.done]

    def steps():
        yield 0.6
        app.scale.set(3)
        app.limiter.set(3)
        app.spin_n.set(5)
        app.btn_dl.invoke()
        app.btn_ports.invoke()
        app.btn_dns.invoke()
        yield 0.9
        app.log(f"Kịch bản bấm Hủy ở tác vụ #{app.tasks[1].id}")
        app.tasks[1].widgets[3].invoke()
        yield 0.6
        waiting = sum(1 for t in app.tasks if t.widgets[2]["text"] == "chờ lượt")
        app.log(f"Chụp ảnh 1: {len(running())} tác vụ chưa xong, {waiting} đang chờ lượt")
        snapshot(".gui_1.png")
        yield lambda: not running()
        app.spin_n.set(4)
        app.btn_dl.invoke()                       # bốn tệp chạy nền trong lúc làm sai
        yield 0.3
        app.btn_bad.invoke()                      # chặn luồng giao diện ~1,3 s
        yield 0.1
        snapshot(".gui_2.png")
        yield lambda: not running()
        app.spin_n.set(4)
        app.btn_dl.invoke()
        yield 0.5
        app.btn_cancel_all.invoke()
        yield lambda: not running()
        yield 0.2
        subprocess.run(["convert", ".gui_1.png", ".gui_2.png", "-bordercolor", "#d0d4dc",
                        "-border", "6", "-append", out], check=True)
        subprocess.run(["rm", "-f", ".gui_1.png", ".gui_2.png"])
        app.log(f"Đã chụp cửa sổ: {out}")
        for phase, (n, s, mx) in app.gaps.items():
            app.log(f"Nhịp {phase:<17}: {n:>4} nhịp, TB {s / n:6.1f} ms, tối đa {mx:6.0f} ms")
        app.close()

    gen = steps()

    def step():
        try:
            w = next(gen)
        except StopIteration:
            return
        if callable(w):                           # chờ tới khi điều kiện đúng
            def check():
                if w():
                    app.root.after(50, step)
                else:
                    app.root.after(50, check)
            check()
        else:
            app.root.after(int(w * 1000), step)
    step()


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(1)
    root = tk.Tk()
    app = App(root, sys.argv[1], int(sys.argv[2]))
    root.protocol("WM_DELETE_WINDOW", app.close)
    if "--demo" in sys.argv:
        demo(app)
    root.mainloop()


if __name__ == "__main__":
    main()
