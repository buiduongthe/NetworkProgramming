"""Bài mở rộng MR6.3 – Server hội thoại video có giao diện Tkinter ("Camera Server").

Bên trái: lưới 4×4 ô, mỗi client một ô (hình mới nhất server nhận được, tên, fps nhận; chấm đỏ khi
camera tắt). Bên phải: START/STOP SERVER, danh sách client, điều khiển client đang chọn (tắt/bật
camera – khóa của server, fps gửi tối đa 1–15, độ phân giải gửi 1/1, 1/2, 1/4), fps chung, nhật ký.
Mạng và ghép lưới gửi cho client do PhongVideo (phong_video.py) làm trên luồng nền; cửa sổ cứ
100 ms đọc anh_chup() một lần và chỉ tạo lại ảnh của ô nào vừa đổi.

Chạy:  python3 server_gui.py [host] [cổng_tcp] [cổng_udp]      (mặc định 127.0.0.1 41630 41631)
"""
import sys
import threading
import time
import tkinter as tk
from tkinter import ttk

from chung import CAO, RONG
from giao_dien_chung import NEN, HangDoiGiaoDien, NhatKy, anh_tk
from phong_video import PhongVideo

SO_COT = SO_HANG = 4
KHE = 4                                        # khoảng cách giữa các ô (điểm ảnh)


class CuaSoServer:
    def __init__(self, root, host="127.0.0.1", cong_tcp=41630, cong_udp=41631, in_nhat_ky=False):
        self.root, self.in_nhat_ky = root, in_nhat_ky
        self.phong = None
        self.ui = HangDoiGiaoDien(root)
        self.chon = None                       # mã client đang chọn
        self.so_o_da_ve, self.anh_o = {}, {}   # mã -> số hiệu ô đã vẽ / PhotoImage (giữ tham chiếu!)
        self.ds = []
        root.title("Camera Server – MR6.3")
        root.configure(bg=NEN)

        # ---- Lưới 4×4
        rong, cao = SO_COT * (RONG + KHE) + KHE, SO_HANG * (CAO + KHE) + KHE
        self.luoi = tk.Canvas(root, width=rong, height=cao, bg="#9a9a9a", highlightthickness=0)
        self.luoi.grid(row=0, column=0, padx=6, pady=6, sticky="n")
        self.o = []
        for n in range(SO_COT * SO_HANG):
            x, y = KHE + (n % SO_COT) * (RONG + KHE), KHE + (n // SO_COT) * (CAO + KHE)
            self.o.append({
                "khung": self.luoi.create_rectangle(x - 2, y - 2, x + RONG + 1, y + CAO + 1,
                                                    fill="#dcdcdc", outline="#707070", width=2),
                "anh": self.luoi.create_image(x, y, anchor="nw"),
                "nen_chu": self.luoi.create_rectangle(0, 0, 0, 0, fill="black", width=0),
                "fps": self.luoi.create_text(x + RONG - 4, y + 4, anchor="ne", fill="white",
                                             font=("TkDefaultFont", 8, "bold"), text=""),
                "cham": self.luoi.create_oval(x + 6, y + 6, x + 18, y + 18, fill="red", outline="white",
                                              state="hidden"),
            })
        self.luoi.bind("<Button-1>", self._bam_luoi)

        # ---- Khung phải
        p = tk.Frame(root, bg=NEN)
        p.grid(row=0, column=1, sticky="ns", padx=(0, 6), pady=6)
        g = tk.LabelFrame(p, text="Kết nối", bg=NEN)
        g.pack(fill="x")
        tk.Label(g, text="IP:", bg=NEN).grid(row=0, column=0, sticky="w")
        self.o_ip = tk.Entry(g, width=13)
        self.o_ip.insert(0, host)
        self.o_ip.grid(row=0, column=1, columnspan=3, sticky="w")
        tk.Label(g, text="TCP:", bg=NEN).grid(row=1, column=0, sticky="w")
        self.o_tcp = tk.Entry(g, width=6)
        self.o_tcp.insert(0, str(cong_tcp))
        self.o_tcp.grid(row=1, column=1, sticky="w")
        tk.Label(g, text="UDP:", bg=NEN).grid(row=1, column=2, sticky="e")
        self.o_udp = tk.Entry(g, width=6)
        self.o_udp.insert(0, str(cong_udp))
        self.o_udp.grid(row=1, column=3, sticky="w")
        self.nut_start = tk.Button(g, text="START SERVER", width=13, command=self.bat_server)
        self.nut_start.grid(row=2, column=0, columnspan=2, pady=3)
        self.nut_stop = tk.Button(g, text="STOP SERVER", width=13, command=self.dung_server, state="disabled")
        self.nut_stop.grid(row=2, column=2, columnspan=2, pady=3)

        tk.Label(p, text="Danh sách client:", bg=NEN).pack(anchor="w", pady=(4, 0))
        self.ds_khach = tk.Listbox(p, height=5, width=40, exportselection=False, font=("TkFixedFont", 8))
        self.ds_khach.pack(fill="x")
        self.ds_khach.bind("<<ListboxSelect>>", self._chon_ds)

        g = tk.LabelFrame(p, text="Client đang chọn: –", bg=NEN)
        g.pack(fill="x", pady=4)
        self.khung_chon = g
        self.nut_tat_cam = tk.Button(g, text="Tắt camera", width=11, command=lambda: self._dat_cam(True))
        self.nut_tat_cam.grid(row=0, column=0, padx=2, pady=2)
        self.nut_bat_cam = tk.Button(g, text="Bật camera", width=11, command=lambda: self._dat_cam(False))
        self.nut_bat_cam.grid(row=0, column=1, padx=2, pady=2)
        tk.Label(g, text="FPS gửi tối đa:", bg=NEN).grid(row=1, column=0, sticky="w")
        self.spin_fps = tk.Spinbox(g, from_=1, to=15, width=5, command=self.ap_fps)
        self.spin_fps.grid(row=1, column=1, sticky="w")
        self.spin_fps.delete(0, "end")                     # chưa chọn client: hiện giá trị mặc định 10
        self.spin_fps.insert(0, "10")
        self.spin_fps.bind("<Return>", lambda e: self.ap_fps())
        tk.Label(g, text="Độ phân giải gửi:", bg=NEN).grid(row=2, column=0, sticky="w")
        self.chon_chia = ttk.Combobox(g, state="readonly", width=5, values=["1/1", "1/2", "1/4"])
        self.chon_chia.current(0)
        self.chon_chia.grid(row=2, column=1, sticky="w")
        self.chon_chia.bind("<<ComboboxSelected>>", lambda e: self.ap_chia())
        f = tk.Frame(p, bg=NEN)
        f.pack(fill="x")
        tk.Label(f, text="FPS chung (gửi tới mọi client):", bg=NEN).pack(side="left")
        self.spin_fps_chung = tk.Spinbox(f, from_=1, to=15, width=5, command=self.ap_fps_chung)
        self.spin_fps_chung.delete(0, "end")
        self.spin_fps_chung.insert(0, "10")
        self.spin_fps_chung.pack(side="left")
        self.spin_fps_chung.bind("<Return>", lambda e: self.ap_fps_chung())
        tk.Label(p, text="Nhật ký:", bg=NEN).pack(anchor="w", pady=(4, 0))
        self.nhat_ky = NhatKy(p, cao=10, rong=44)
        self.nhat_ky.pack(fill="both", expand=True)

        self.thanh_tt = tk.Label(root, text="Trạng thái: server chưa chạy", anchor="w", relief="sunken",
                                 bg="white")
        self.thanh_tt.grid(row=1, column=0, columnspan=2, sticky="we", padx=6, pady=(0, 6))
        root.protocol("WM_DELETE_WINDOW", self.dong)
        root.after(100, self.cap_nhat)

    # ------------------------------------------------------------ nhật ký, server
    def ghi(self, text):
        self.nhat_ky.ghi(time.strftime("[%H:%M:%S] ") + text)
        if self.in_nhat_ky:
            print(f"[server-gui] {text}", flush=True)

    def bat_server(self):
        try:
            phong = PhongVideo(lambda t: self.ui.dua_vao(self.ghi, t))
            phong.fps_chung = int(self.spin_fps_chung.get())
            phong.bat_dau(self.o_ip.get().strip(), int(self.o_tcp.get()), int(self.o_udp.get()))
        except (OSError, ValueError) as e:
            self.thanh_tt.configure(text=f"Trạng thái: không chạy được server – {e}")
            self.ghi(f"Lỗi: {e}")
            return
        self.phong = phong
        self.thanh_tt.configure(text=f"Trạng thái: server đang chạy tại {phong.dia_chi}")
        self.nut_start.configure(state="disabled")
        self.nut_stop.configure(state="normal")

    def dung_server(self):
        phong, self.phong = self.phong, None
        if phong is None:
            return
        self.nut_stop.configure(state="disabled")
        self.thanh_tt.configure(text="Trạng thái: đang dừng server…")

        def dung_nen():                                  # chờ luồng nền dừng – không chặn giao diện
            phong.dung()
            self.ui.dua_vao(self._da_dung)
        threading.Thread(target=dung_nen, daemon=True).start()

    def _da_dung(self):
        self.thanh_tt.configure(text="Trạng thái: server đã dừng")
        self.nut_start.configure(state="normal")

    # ------------------------------------------------------------ chọn client, điều khiển
    def _bam_luoi(self, e):
        n = (e.y // (CAO + KHE)) * SO_COT + e.x // (RONG + KHE)
        if n < len(self.ds):
            self.chon_client(self.ds[n]["ma"])

    def _chon_ds(self, e):
        s = self.ds_khach.curselection()
        if s and s[0] < len(self.ds):
            self.chon_client(self.ds[s[0]]["ma"])

    def chon_client(self, ma):
        self.chon = ma
        k = next((k for k in self.ds if k["ma"] == ma), None)
        if k:
            self.spin_fps.delete(0, "end")
            self.spin_fps.insert(0, str(k["fps_toi_da"]))
            self.chon_chia.current({1: 0, 2: 1, 4: 2}[k["chia"]])

    def _dat_cam(self, tat):
        if self.phong and self.chon is not None:
            self.phong.dat_camera(self.chon, tat)

    def ap_fps(self):
        try:
            n = max(1, min(15, int(self.spin_fps.get())))
        except ValueError:
            return
        if self.phong and self.chon is not None:
            self.phong.dat_fps_toi_da(self.chon, n)

    def ap_chia(self):
        if self.phong and self.chon is not None:
            self.phong.dat_do_phan_giai(self.chon, (1, 2, 4)[self.chon_chia.current()])

    def ap_fps_chung(self):
        try:
            n = max(1, min(15, int(self.spin_fps_chung.get())))
        except ValueError:
            return
        if self.phong:
            self.phong.dat_fps_chung(n)

    # ------------------------------------------------------------ vẽ lại định kỳ
    def cap_nhat(self):
        ds = self.phong.anh_chup(self.so_o_da_ve) if self.phong else []
        self.ds = ds[:SO_COT * SO_HANG]
        for n, o in enumerate(self.o):
            k = self.ds[n] if n < len(self.ds) else None
            if k is None:
                self.luoi.itemconfigure(o["anh"], image="")
                self.luoi.itemconfigure(o["fps"], text="")
                self.luoi.coords(o["nen_chu"], 0, 0, 0, 0)
                self.luoi.itemconfigure(o["cham"], state="hidden")
                self.luoi.itemconfigure(o["khung"], outline="#707070")
                continue
            if k["o"] is not None:                       # ô đổi: tạo lại PhotoImage
                self.anh_o[k["ma"]] = anh_tk(k["o"], RONG, CAO)
                self.so_o_da_ve[k["ma"]] = k["so_o"]
            if k["ma"] in self.anh_o:
                self.luoi.itemconfigure(o["anh"], image=self.anh_o[k["ma"]])
            self.luoi.itemconfigure(o["fps"], text=f"{k['ten']}  {k['fps_vao']:.0f} fps nhận")
            self.luoi.tag_raise(o["nen_chu"], o["anh"])
            self.luoi.tag_raise(o["fps"], o["nen_chu"])
            x1, y1, x2, y2 = self.luoi.bbox(o["fps"])
            self.luoi.coords(o["nen_chu"], x1 - 2, y1 - 1, x2 + 2, y2 + 1)
            self.luoi.itemconfigure(o["cham"], state="hidden" if k["cam_mo"] else "normal")
            self.luoi.itemconfigure(o["khung"], outline="#1e5bd8" if k["ma"] == self.chon else "#707070")
        dang_co = {k["ma"] for k in ds}
        for ma in [m for m in self.anh_o if m not in dang_co]:
            self.anh_o.pop(ma)
            self.so_o_da_ve.pop(ma, None)
        dong = [f"#{k['ma']} {k['ten']:<4} {k['ly_do']:<12} vào {k['fps_vao']:2.0f}fps {k['kbps_vao']:4.0f}kb"
                for k in ds]
        if list(self.ds_khach.get(0, "end")) != dong:
            self.ds_khach.delete(0, "end")
            for d in dong:
                self.ds_khach.insert("end", d)
        k = next((k for k in ds if k["ma"] == self.chon), None)
        self.khung_chon.configure(text=f"Client đang chọn: #{k['ma']} {k['ten']} – camera {k['ly_do']}"
                                  if k else "Client đang chọn: –")
        if self.phong:
            vao = sum(k["kbps_vao"] for k in ds)
            ra = sum(k["kbps_ra"] for k in ds)
            self.thanh_tt.configure(text=f"Trạng thái: server đang chạy tại {self.phong.dia_chi} – "
                                    f"{len(ds)} client, vào {vao:.0f} kbit/s, ra {ra:.0f} kbit/s")
        if self.ui.song:
            self.root.after(100, self.cap_nhat)

    def dong(self):
        self.ui.song = False
        if self.phong:
            self.phong.dung()
        self.root.destroy()


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong_tcp = int(sys.argv[2]) if len(sys.argv) > 2 else 41630
    cong_udp = int(sys.argv[3]) if len(sys.argv) > 3 else 41631
    root = tk.Tk()
    CuaSoServer(root, host, cong_tcp, cong_udp)
    root.mainloop()


if __name__ == "__main__":
    main()
