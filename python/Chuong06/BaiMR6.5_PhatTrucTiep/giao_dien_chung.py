"""Bài mở rộng MR6.5 – Thành phần giao diện Tkinter dùng chung cho server_gui, phat_gui, xem_gui.

Quy tắc (như MR6.2, MR6.3): CHỈ luồng giao diện (luồng chạy mainloop) chạm vào điều khiển. Luồng
mạng/âm thanh/hiển thị của lõi đưa thông báo vào HangDoiGiaoDien (queue.Queue) hoặc ghi số liệu vào
đối tượng lõi dưới khóa; luồng giao diện đọc định kỳ bằng root.after(...) rồi vẽ lại.
Ảnh: khung RGB thô + header PPM "P6 rộng cao 255" là ảnh PPM mà tk.PhotoImage đọc trực tiếp.
"""
import math
import queue
import time
import tkinter as tk

NEN = "#b0c4de"
XANH_LA, DO, XANH_DUONG, CAM = "#90ee90", "#f08080", "#add8e6", "#ffb347"


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
            try:
                ham(*ts)
            except tk.TclError:                        # cửa sổ đã đóng
                pass
        if self.song:
            self.root.after(self.chu_ky, self._doc)


def anh_tk(rgb, w, h, phong=1):
    """bytes RGB -> tk.PhotoImage (phóng to `phong` lần bằng zoom của Tk)."""
    img = tk.PhotoImage(data=b"P6 %d %d 255\n" % (w, h) + bytes(rgb), format="PPM")
    return img.zoom(phong) if phong > 1 else img


class ThanhMuc(tk.Canvas):
    """Thanh đo mức âm -60 … 0 dBFS (như MR6.2): xanh < -12, vàng < -3, đỏ gần cắt ngọn."""

    CHU = 66

    def __init__(self, cha, rong=200, cao=16, **kw):
        super().__init__(cha, width=rong, height=cao, bg="#202020", highlightthickness=1,
                         highlightbackground="#808080", **kw)
        self.rong, self.cao, self.hien = rong - self.CHU, cao, -math.inf
        self.create_rectangle(self.rong, 0, rong + 2, cao + 2, fill="#f4f4f4", width=0)
        self.thanh = self.create_rectangle(0, 0, 0, cao, fill="#30c030", width=0)
        self.chu = self.create_text(rong - 4, cao // 2 + 1, anchor="e", fill="black",
                                    font=("TkDefaultFont", 8), text="--- dBFS")

    def dat(self, db):
        self.hien = db if db > self.hien else self.hien - 2.0
        ti_le = 0.0 if self.hien < -60 else min(1.0, (self.hien + 60) / 60)
        mau = "#30c030" if self.hien < -12 else ("#e0c020" if self.hien < -3 else "#e03030")
        self.coords(self.thanh, 0, 0, ti_le * self.rong, self.cao + 2)
        self.itemconfigure(self.thanh, fill=mau)
        self.itemconfigure(self.chu, text="--- dBFS" if db < -90 else f"{db:5.1f} dBFS")


class NhatKy(tk.Frame):
    """Ô nhật ký cuộn (chỉ đọc), mỗi dòng có giờ."""

    def __init__(self, cha, cao=6, rong=60, **kw):
        super().__init__(cha, **kw)
        self.van_ban = tk.Text(self, height=cao, width=rong, state="disabled", wrap="word",
                               font=("TkFixedFont", 9), bg="white")
        cuon = tk.Scrollbar(self, command=self.van_ban.yview)
        self.van_ban.configure(yscrollcommand=cuon.set)
        self.van_ban.pack(side="left", fill="both", expand=True)
        cuon.pack(side="right", fill="y")

    def ghi(self, text):
        self.van_ban.configure(state="normal")
        self.van_ban.insert("end", time.strftime("%H:%M:%S ") + text + "\n")
        if int(self.van_ban.index("end-1c").split(".")[0]) > 500:
            self.van_ban.delete("1.0", "2.0")
        self.van_ban.see("end")
        self.van_ban.configure(state="disabled")


def nut(cha, chu, mau, lenh, rong=12):
    return tk.Button(cha, text=chu, bg=mau, activebackground=mau, width=rong, command=lenh,
                     relief="raised", bd=1)


def khung_nhom(cha, tieu_de):
    return tk.LabelFrame(cha, text=tieu_de, bg=NEN, padx=6, pady=4, font=("TkDefaultFont", 9, "bold"))


def nhan(cha, chu="", **kw):
    kw.setdefault("bg", NEN)
    kw.setdefault("anchor", "w")
    return tk.Label(cha, text=chu, **kw)


def o_nhap(cha, gia_tri, rong):
    o = tk.Entry(cha, width=rong)
    o.insert(0, str(gia_tri))
    return o


def chu_ms(x, dau=False):
    if x is None:
        return "–"
    return f"{x:+.0f} ms" if dau else f"{x:.0f} ms"
