"""Bài 5.3 – Ứng dụng tra cứu có giao diện Tkinter.

Cài đặt Mã giả 5.3 (cách 1: luồng nền và UI_INVOKE) cho nút Kết nối và Mã giả 5.4
(cách 2: trình xử lý sự kiện bất đồng bộ) cho nút Tra cứu – Mục 5.3.2, Giáo trình Lập trình mạng.
Kế thừa: Mã giả 2.2 (SAFE_CONNECT, bản chặn), 3.8 (một lượt tra cứu), 5.1 (khung JSON bất đồng bộ).
- UI_INVOKE trên Tkinter: luồng nền đưa (thủ tục, tham số) vào queue.Queue, luồng chính đọc
  định kỳ bằng root.after (bảng ui-dispatch).
- Cách 2: vòng lặp thông điệp Tk được chạy bên trong vòng lặp sự kiện asyncio (root.update()
  rồi await asyncio.sleep), nên sau mỗi await mã chạy lại trên luồng giao diện.

Chạy:  python3 lookup_gui.py [host] [port]      (mặc định 127.0.0.1 9100)
"""
import asyncio
import queue
import sys
import threading
import tkinter as tk

from khung_json_async import (ConnectionClosed, NetError, ProtocolError,
                              recv_json_async, safe_connect, send_json_async)


class LookupWindow:
    def __init__(self, root, host, port):
        self.root, self.alive, self.ticks = root, True, 0
        self.reader = self.writer = None                   # conn: trường của cửa sổ
        self.tasks = set()                                 # các tác vụ của cửa sổ, bị hủy khi đóng
        self.ui_queue = queue.Queue()
        root.title("Tra cứu thuật ngữ – Bài 5.3")
        root.geometry("600x190+0+0")
        f = tk.Frame(root, padx=10, pady=10)
        f.pack(fill="both", expand=True)
        tk.Label(f, text="Máy chủ:").grid(row=0, column=0, sticky="w")
        self.txt_host = tk.Entry(f, width=16)
        self.txt_host.insert(0, host)
        self.txt_host.grid(row=0, column=1, sticky="w")
        tk.Label(f, text="Cổng:").grid(row=0, column=2, sticky="e")
        self.txt_port = tk.Entry(f, width=7)
        self.txt_port.insert(0, str(port))
        self.txt_port.grid(row=0, column=3, sticky="w")
        self.btn_connect = tk.Button(f, text="Kết nối", width=10, command=self.on_connect_click)
        self.btn_connect.grid(row=0, column=4, padx=6)
        tk.Label(f, text="Thuật ngữ:").grid(row=1, column=0, sticky="w", pady=8)
        self.txt_term = tk.Entry(f, width=34)
        self.txt_term.grid(row=1, column=1, columnspan=3, sticky="w")
        self.btn_lookup = tk.Button(f, text="Tra cứu", width=10, state="disabled",
                                    command=lambda: self.spawn(self.on_lookup_click()))
        self.btn_lookup.grid(row=1, column=4, padx=6)
        self.txt_result = tk.Label(f, text="", width=64, height=3, anchor="nw", justify="left",
                                   relief="sunken", bg="white", wraplength=540)
        self.txt_result.grid(row=2, column=0, columnspan=5, sticky="we")
        self.lbl_status = tk.Label(f, text="Chưa kết nối", anchor="w")
        self.lbl_status.grid(row=3, column=0, columnspan=3, sticky="w", pady=6)
        self.lbl_clock = tk.Label(f, text="", anchor="e", fg="gray30")
        self.lbl_clock.grid(row=3, column=3, columnspan=2, sticky="e")
        root.protocol("WM_DELETE_WINDOW", self.close)
        root.after(50, self.poll_ui_queue)
        root.after(100, self.tick)

    # ---- tiện ích giao diện -------------------------------------------------------
    def ui_invoke(self, proc, *args):                      # UI_INVOKE: gọi được từ luồng nền
        self.ui_queue.put((proc, args))

    def poll_ui_queue(self):                               # luồng chính đọc hàng đợi mỗi 50 ms
        while not self.ui_queue.empty():
            proc, args = self.ui_queue.get_nowait()
            proc(*args)
        if self.alive:
            self.root.after(50, self.poll_ui_queue)

    def tick(self):                                        # đồng hồ: chứng tỏ giao diện không đứng hình
        self.ticks += 1
        self.lbl_clock.config(text=f"Đồng hồ giao diện: {self.ticks / 10:.1f} s")
        if self.alive:
            self.root.after(100, self.tick)

    def show_status(self, text):
        self.lbl_status.config(text=text)

    def set_enabled(self, button, enabled):
        button.config(state="normal" if enabled else "disabled")

    def spawn(self, coro):                                 # khởi động trình xử lý bất đồng bộ
        t = asyncio.get_running_loop().create_task(coro)
        self.tasks.add(t)
        t.add_done_callback(self.tasks.discard)

    # ---- Mã giả 5.3: cách (1) luồng nền và UI_INVOKE -------------------------------
    def on_connect_click(self):                            # trên luồng giao diện
        self.btn_connect.config(state="disabled")
        threading.Thread(target=self.connect_worker, daemon=True,
                         args=(self.txt_host.get(), int(self.txt_port.get()))).start()

    def connect_worker(self, host, port):                  # Mã giả 5.3, dòng 5–14: chạy trên luồng nền
        try:
            c = safe_connect(host, port, 3000)             # dòng 7: chặn luồng nền, không chặn giao diện
            self.ui_invoke(lambda: self.spawn(self.attach(c)))   # dòng 8: SET_FIELD(conn, c) trên luồng UI
            self.ui_invoke(self.show_status, "Đã kết nối")      # dòng 9
        except NetError as e:                              # dòng 10–12
            self.ui_invoke(self.show_status, "Lỗi: " + e.code)
            self.ui_invoke(self.set_enabled, self.btn_connect, True)

    async def attach(self, conn):                          # SET_FIELD: lưu socket vào trường của cửa sổ,
        self.reader, self.writer = await asyncio.open_connection(sock=conn)   # bọc cho asyncio
        self.btn_lookup.config(state="normal")

    # ---- Mã giả 5.4: cách (2) trình xử lý sự kiện bất đồng bộ ------------------------
    async def on_lookup_click(self):
        self.btn_lookup.config(state="disabled")           # chống bấm lặp
        self.lbl_status.config(text="Đang tra cứu...")
        try:
            req = {"op": "lookup", "term": self.txt_term.get()}
            await send_json_async(self.writer, req)
            resp = await recv_json_async(self.reader)      # UI không bị chặn
            if resp.get("status") == 200:                  # dòng 8–12; đã ở luồng giao diện
                self.txt_result.config(text=resp["text"])
            else:
                self.txt_result.config(text=f"({resp.get('status')}) {resp.get('error')}")
            self.lbl_status.config(text="Xong")
        except (NetError, ProtocolError, ConnectionClosed, OSError) as e:
            self.lbl_status.config(text="Lỗi: " + str(e))
        finally:
            if self.alive:
                self.btn_lookup.config(state="normal")

    def close(self):                                       # đóng cửa sổ: hủy tác vụ, đóng kết nối
        self.alive = False
        for t in list(self.tasks):
            t.cancel()                                     # CANCEL(ct)
        if self.writer:
            self.writer.close()


async def run_gui(win):
    """Vòng lặp thông điệp Tk chạy xen kẽ với vòng lặp sự kiện asyncio trên cùng một luồng."""
    while win.alive:
        win.root.update()
        await asyncio.sleep(0.01)
    win.root.destroy()


if __name__ == "__main__":
    a = sys.argv[1:]
    w = LookupWindow(tk.Tk(), a[0] if a else "127.0.0.1", int(a[1]) if len(a) > 1 else 9100)
    asyncio.run(run_gui(w))
