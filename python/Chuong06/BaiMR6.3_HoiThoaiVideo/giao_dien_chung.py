"""Bài mở rộng MR6.3 – Thành phần giao diện Tkinter dùng chung cho server_gui.py và khach_gui.py.

Quy tắc: CHỈ luồng giao diện (luồng chạy mainloop) chạm vào điều khiển. Luồng mạng / luồng đọc
camera đưa thông báo vào HangDoiGiaoDien (queue.Queue) hoặc ghi số liệu vào đối tượng lõi dưới khóa;
luồng giao diện đọc định kỳ bằng root.after(...) rồi vẽ lại.
Hiển thị ảnh: khung RGB thô (bytes) + header PPM "P6 rộng cao 255" là một ảnh PPM hợp lệ mà
tk.PhotoImage đọc được trực tiếp – không cần thư viện ảnh ngoài.
"""
import queue
import tkinter as tk

NEN = "#e6e6e6"


class HangDoiGiaoDien:
    """Cầu nối luồng nền -> luồng giao diện: dua_vao() gọi từ bất kỳ luồng nào."""

    def __init__(self, root, chu_ky_ms=50):
        self.root, self.chu_ky, self.hang, self.song = root, chu_ky_ms, queue.Queue(), True
        root.after(chu_ky_ms, self._doc)

    def dua_vao(self, ham, *tham_so):
        self.hang.put((ham, tham_so))

    def _doc(self):
        while True:
            try:
                ham, ts = self.hang.get_nowait()
            except queue.Empty:
                break
            ham(*ts)
        if self.song:
            self.root.after(self.chu_ky, self._doc)


def anh_tk(rgb, w, h, phong=1):
    """bytes RGB -> tk.PhotoImage (phóng to `phong` lần bằng zoom của Tk – lặp điểm gần nhất)."""
    img = tk.PhotoImage(data=b"P6 %d %d 255\n" % (w, h) + bytes(rgb), format="PPM")
    return img.zoom(phong) if phong > 1 else img


class NhatKy(tk.Frame):
    def __init__(self, cha, cao=8, rong=40, **kw):
        super().__init__(cha, **kw)
        self.van_ban = tk.Text(self, height=cao, width=rong, state="disabled", wrap="word",
                               font=("TkFixedFont", 8), bg="white")
        cuon = tk.Scrollbar(self, command=self.van_ban.yview)
        self.van_ban.configure(yscrollcommand=cuon.set)
        self.van_ban.pack(side="left", fill="both", expand=True)
        cuon.pack(side="right", fill="y")

    def ghi(self, text):
        self.van_ban.configure(state="normal")
        self.van_ban.insert("end", text + "\n")
        if int(self.van_ban.index("end-1c").split(".")[0]) > 500:
            self.van_ban.delete("1.0", "2.0")
        self.van_ban.see("end")
        self.van_ban.configure(state="disabled")
