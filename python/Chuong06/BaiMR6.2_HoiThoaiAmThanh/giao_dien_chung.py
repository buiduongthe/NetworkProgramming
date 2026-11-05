"""Bài mở rộng MR6.2 – Thành phần giao diện Tkinter dùng chung cho server_gui.py và khach_gui.py.

Quy tắc quan trọng nhất của lập trình giao diện: CHỈ luồng giao diện (luồng chạy mainloop) được
chạm vào điều khiển (widget). Luồng mạng / luồng âm thanh không gọi widget trực tiếp mà:
  - đưa thông báo vào HangDoiGiaoDien (queue.Queue an toàn đa luồng), luồng giao diện lấy ra định
    kỳ bằng root.after(...);
  - hoặc ghi số liệu vào đối tượng lõi (dưới khóa), luồng giao diện đọc định kỳ (~10 lần/giây)
    để vẽ thanh đo mức.
"""
import math
import queue
import tkinter as tk

NEN = "#b0c4de"            # nền xanh nhạt như ảnh mẫu
XANH_LA, DO, XANH_DUONG, CAM = "#90ee90", "#f08080", "#add8e6", "#ffb347"


class HangDoiGiaoDien:
    """Cầu nối luồng nền -> luồng giao diện: dua_vao() gọi từ bất kỳ luồng nào; chạy() trên luồng giao diện."""

    def __init__(self, root, chu_ky_ms=50):
        self.root, self.chu_ky, self.hang = root, chu_ky_ms, queue.Queue()
        self.song = True
        root.after(chu_ky_ms, self._doc)

    def dua_vao(self, ham, *tham_so):
        self.hang.put((ham, tham_so))

    def _doc(self):
        while True:
            try:
                ham, ts = self.hang.get_nowait()
            except queue.Empty:
                break
            ham(*ts)                                   # chạy trên luồng giao diện
        if self.song:
            self.root.after(self.chu_ky, self._doc)


class ThanhMuc(tk.Canvas):
    """Thanh đo mức âm: -60 dBFS (trống) … 0 dBFS (đầy); xanh < -12, vàng < -3, đỏ gần cắt ngọn.
    Thanh tụt chậm (2 dB mỗi lần vẽ) để mắt kịp nhìn, chữ (bên phải) hiện đúng giá trị vừa đo."""

    CHU = 66                                            # bề rộng phần chữ bên phải (điểm ảnh)

    def __init__(self, cha, rong=150, cao=16, **kw):
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
    """Ô nhật ký cuộn (chỉ đọc)."""

    def __init__(self, cha, cao=8, rong=60, **kw):
        super().__init__(cha, **kw)
        self.van_ban = tk.Text(self, height=cao, width=rong, state="disabled", wrap="word",
                               font=("TkFixedFont", 9), bg="white")
        cuon = tk.Scrollbar(self, command=self.van_ban.yview)
        self.van_ban.configure(yscrollcommand=cuon.set)
        self.van_ban.pack(side="left", fill="both", expand=True)
        cuon.pack(side="right", fill="y")

    def ghi(self, text):
        self.van_ban.configure(state="normal")
        self.van_ban.insert("end", text + "\n")
        if int(self.van_ban.index("end-1c").split(".")[0]) > 500:   # giữ tối đa 500 dòng
            self.van_ban.delete("1.0", "2.0")
        self.van_ban.see("end")
        self.van_ban.configure(state="disabled")


def nut(cha, chu, mau, lenh, rong=11):
    return tk.Button(cha, text=chu, bg=mau, activebackground=mau, width=rong, command=lenh,
                     relief="raised", bd=1)


def khung_nhom(cha, tieu_de):
    return tk.LabelFrame(cha, text=tieu_de, bg=NEN, padx=6, pady=4, font=("TkDefaultFont", 9, "bold"))


def nhan(cha, chu="", **kw):
    kw.setdefault("bg", NEN)
    return tk.Label(cha, text=chu, **kw)


class TreHoan:
    """Gom các lần kéo thanh trượt liên tiếp: chỉ thực hiện `ham(giá trị)` sau `ms` không đổi nữa
    (tránh gửi hàng chục thông điệp TCP khi người dùng kéo)."""

    def __init__(self, root, ham, ms=200):
        self.root, self.ham, self.ms, self.hen = root, ham, ms, None

    def __call__(self, gia_tri):
        if self.hen is not None:
            self.root.after_cancel(self.hen)
        self.hen = self.root.after(self.ms, self._chay, gia_tri)

    def _chay(self, gia_tri):
        self.hen = None
        self.ham(gia_tri)
