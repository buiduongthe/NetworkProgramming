"""Bài mở rộng MR6.5 – Máy phát trực tiếp có giao diện Tkinter.

Lõi MayPhat (may_phat.py) giữ kết nối, luồng âm thanh 20 ms và luồng video; cửa sổ chọn nguồn
(camera thật qua ffmpeg / mẫu thử testsrc / giả lập clapper; micro thật qua sounddevice / giả lập),
fps, GOP, bấm Phát / Dừng phát / Khung khóa ngay; cứ 100 ms đọc trạng thái để vẽ xem trước, thanh
mức micro, thống kê và danh sách người xem. Thiết bị thật chỉ được mở khi bấm PHÁT.

Chạy:  python3 phat_gui.py [tên] [host] [cổng_tcp]
"""
import sys
import threading
import tkinter as tk
from tkinter import ttk

from camera import GIA_LAP, MAU_THU, co_ffmpeg, liet_ke_camera, mo_camera
from chung import CAO, FPS_MAC_DINH, GOP_MAC_DINH, RONG
from giao_dien_chung import (DO, NEN, XANH_DUONG, XANH_LA, HangDoiGiaoDien, NhatKy, ThanhMuc, anh_tk,
                             khung_nhom, nhan, nut, o_nhap)
from may_phat import MayPhat
from thiet_bi_am_thanh import GIA_LAP as MIC_GIA_LAP
from thiet_bi_am_thanh import liet_ke, mo_mic, nap_sounddevice


class CuaSoPhat:
    def __init__(self, root, ten="P", host="127.0.0.1", cong=41652):
        self.root = root
        self.ui = HangDoiGiaoDien(root)
        self.sd, _ = nap_sounddevice()
        self.lop = None
        self.anh = None
        self.ts_da_ve = None
        self.dem_giay = 0
        root.title(f"Máy phát trực tiếp – MR6.5 ({ten})")
        root.configure(bg=NEN)
        root.resizable(False, False)

        tren = tk.Frame(root, bg=NEN)
        tren.pack(fill="x", padx=6, pady=4)
        g = khung_nhom(tren, "Kết nối server")
        g.pack(side="left", fill="y")
        nhan(g, "Server IP:").grid(row=0, column=0, sticky="w")
        self.o_ip = o_nhap(g, host, 13)
        self.o_ip.grid(row=0, column=1, sticky="w")
        nhan(g, "Port:").grid(row=1, column=0, sticky="w")
        self.o_cong = o_nhap(g, cong, 7)
        self.o_cong.grid(row=1, column=1, sticky="w")
        nhan(g, "Tên:").grid(row=2, column=0, sticky="w")
        self.o_ten = o_nhap(g, ten, 10)
        self.o_ten.grid(row=2, column=1, sticky="w")
        self.nut_ket_noi = nut(g, "Kết nối", XANH_LA, self.bam_ket_noi, 14)
        self.nut_ket_noi.grid(row=3, column=0, columnspan=2, pady=3)

        g = khung_nhom(tren, "Nguồn và tham số")
        g.pack(side="left", fill="both", expand=True, padx=(6, 0))
        nhan(g, "Camera:").grid(row=0, column=0, sticky="w")
        self.ds_cam = list(liet_ke_camera())
        if co_ffmpeg():
            self.ds_cam.append((MAU_THU, "Mẫu thử ffmpeg (testsrc)"))
        self.ds_cam.append((GIA_LAP, "Camera giả lập (bảng clapper)"))
        self.chon_cam = ttk.Combobox(g, state="readonly", width=34, values=[t for _, t in self.ds_cam])
        self.chon_cam.current(len(self.ds_cam) - 1)
        self.chon_cam.grid(row=0, column=1, columnspan=4, sticky="w")
        nhan(g, "Micro:").grid(row=1, column=0, sticky="w")
        self.ds_mic = list(liet_ke(self.sd, thu=True)) + [(MIC_GIA_LAP, "Micro giả lập (440 Hz + bíp mỗi giây)")]
        self.chon_mic = ttk.Combobox(g, state="readonly", width=34, values=[t for _, t in self.ds_mic])
        self.chon_mic.current(len(self.ds_mic) - 1)
        self.chon_mic.grid(row=1, column=1, columnspan=4, sticky="w")
        nhan(g, "FPS:").grid(row=2, column=0, sticky="w")
        self.spin_fps = tk.Spinbox(g, from_=1, to=30, width=4, command=self.ap_tham_so)
        self.spin_fps.delete(0, "end")
        self.spin_fps.insert(0, str(FPS_MAC_DINH))
        self.spin_fps.grid(row=2, column=1, sticky="w")
        nhan(g, "GOP (khung):").grid(row=2, column=2, sticky="e")
        self.spin_gop = tk.Spinbox(g, from_=1, to=300, width=4, command=self.ap_tham_so)
        self.spin_gop.delete(0, "end")
        self.spin_gop.insert(0, str(GOP_MAC_DINH))
        self.spin_gop.grid(row=2, column=3, sticky="w")
        f = tk.Frame(g, bg=NEN)
        f.grid(row=3, column=0, columnspan=5, sticky="w", pady=3)
        self.nut_phat = nut(f, "PHÁT", XANH_LA, self.bat_phat, 10)
        self.nut_phat.pack(side="left")
        self.nut_dung = nut(f, "DỪNG PHÁT", DO, self.dung_phat, 10)
        self.nut_dung.pack(side="left", padx=4)
        self.nut_i = nut(f, "Khung khóa ngay", XANH_DUONG, self.khung_khoa, 14)
        self.nut_i.pack(side="left")
        for b in (self.nut_phat, self.nut_dung, self.nut_i):
            b.configure(state="disabled")
        self.chi_bao = nhan(g, "Camera: tắt | Micro: tắt" + ("" if co_ffmpeg() else " (không có ffmpeg)"),
                            fg="#404040")
        self.chi_bao.grid(row=4, column=0, columnspan=5, sticky="w")

        giua = tk.Frame(root, bg=NEN)
        giua.pack(fill="x", padx=6)
        g = khung_nhom(giua, "Xem trước (khung vừa gửi)")
        g.pack(side="left", fill="y")
        self.canvas = tk.Canvas(g, width=2 * RONG, height=2 * CAO, bg="black", highlightthickness=0)
        self.canvas.pack()
        self.muc_anh = self.canvas.create_image(0, 0, anchor="nw")
        f = tk.Frame(g, bg=NEN)
        f.pack(fill="x", pady=(4, 0))
        nhan(f, "Mức micro:").pack(side="left")
        self.thanh_mic = ThanhMuc(f, rong=230)
        self.thanh_mic.pack(side="left", padx=4)

        g = khung_nhom(giua, "Thống kê")
        g.pack(side="left", fill="both", expand=True, padx=(6, 0))
        self.nhan_tk = {}
        for k, t in (("ip", "Khung I / P đã gửi"), ("tb", "Kích thước TB I / P"), ("pmin", "P nhỏ nhất / lớn nhất"),
                     ("kb", "kbit/s tiếng / hình"), ("pli", "PLI (xin khung khóa) nhận")):
            nhan(g, t + ":").pack(anchor="w")
            self.nhan_tk[k] = nhan(g, "–", font=("TkDefaultFont", 10, "bold"), padx=12)
            self.nhan_tk[k].pack(anchor="w")
        self.nhan_ds = nhan(g, "Người xem (0):")
        self.nhan_ds.pack(anchor="w", pady=(4, 0))
        self.ds_xem = tk.Listbox(g, height=4, width=28)
        self.ds_xem.pack(anchor="w", fill="x")

        g = khung_nhom(root, "Nhật ký")
        g.pack(fill="x", padx=6, pady=4)
        self.nhat_ky = NhatKy(g, cao=5, rong=80, ten=f"phat {ten}")
        self.nhat_ky.pack(fill="x")
        self.thanh_tt = tk.Label(root, text="Trạng thái: chưa kết nối", anchor="w", relief="sunken", bg="white")
        self.thanh_tt.pack(fill="x", padx=6, pady=(0, 6))
        self.nhan_tt = "chưa kết nối"
        root.after(100, self.cap_nhat)
        root.after(1000, self.moi_giay)

    def _ghi(self, text):
        self.ui.dua_vao(self.nhat_ky.ghi, text)

    # ------------------------------------------------------------ kết nối
    def bam_ket_noi(self):
        if self.lop and self.lop.da_ket_noi:
            self.lop.dong()
            self._da_ngat("đã ngắt kết nối")
            return
        ten, host = self.o_ten.get().strip(), self.o_ip.get().strip()
        try:
            cong = int(self.o_cong.get())
        except ValueError:
            self.nhan_tt = "cổng không hợp lệ"
            return
        if self.lop:
            self.lop.dong()
        self.lop = MayPhat(ten, bao=self._ghi)
        self.nut_ket_noi.configure(state="disabled", text="Đang kết nối…")
        lop = self.lop

        def nen():                                            # connect có thể chờ tới 5 s
            try:
                self.ui.dua_vao(self._da_ket_noi, lop.ket_noi(host, cong))
            except OSError as e:
                self.ui.dua_vao(self._da_ngat, f"không kết nối được – {e}")
        threading.Thread(target=nen, daemon=True).start()

    def _da_ket_noi(self, ma):
        self.nut_ket_noi.configure(state="normal", text="Ngắt kết nối", bg=DO, activebackground=DO)
        self.nhan_tt = f"đã kết nối, mã {ma}"
        self.nut_phat.configure(state="normal")
        for o in (self.o_ip, self.o_cong, self.o_ten):
            o.configure(state="disabled")

    def _da_ngat(self, text):
        self.nut_ket_noi.configure(state="normal", text="Kết nối", bg=XANH_LA, activebackground=XANH_LA)
        self.nhan_tt = text
        for b in (self.nut_phat, self.nut_dung, self.nut_i):
            b.configure(state="disabled")
        self.chon_cam.configure(state="readonly")
        self.chon_mic.configure(state="readonly")
        for o in (self.o_ip, self.o_cong, self.o_ten):
            o.configure(state="normal")

    # ------------------------------------------------------------ phát
    def _so(self, spin, mac_dinh, toi_da):
        try:
            return max(1, min(toi_da, int(spin.get())))
        except ValueError:
            return mac_dinh

    def bat_phat(self):
        lop = self.lop
        if not (lop and lop.da_ket_noi):
            return
        ma_cam, ten_cam = self.ds_cam[self.chon_cam.current()]
        fps, gop = self._so(self.spin_fps, FPS_MAC_DINH, 30), self._so(self.spin_gop, GOP_MAC_DINH, 300)
        cam = mo_camera(ma_cam, ten_cam, lop.ten, lop.ma or 1, fps)
        chi_so, ten_mic = self.ds_mic[self.chon_mic.current()]
        mic, loi = mo_mic(self.sd, chi_so, ten_mic)
        if loi:
            self.nhat_ky.ghi(loi)
        lop.bat_dau_phat(cam, mic, fps, gop)
        self.chi_bao.configure(text=f"Camera: {cam.mo_ta} | Micro: {mic.mo_ta}",
                               fg="#006000" if cam.that or mic.that else "#8a4b00")
        self.nut_phat.configure(state="disabled")
        self.nut_dung.configure(state="normal")
        self.nut_i.configure(state="normal")
        self.chon_cam.configure(state="disabled")
        self.chon_mic.configure(state="disabled")

    def dung_phat(self):
        if self.lop:
            self.lop.dung_phat()
        self.chi_bao.configure(text="Camera: tắt | Micro: tắt", fg="#404040")
        self.nut_phat.configure(state="normal")
        self.nut_dung.configure(state="disabled")
        self.nut_i.configure(state="disabled")
        self.chon_cam.configure(state="readonly")
        self.chon_mic.configure(state="readonly")

    def khung_khoa(self):
        if self.lop:
            self.lop.khung_khoa_ngay("nút Khung khóa ngay")

    def ap_tham_so(self):
        if self.lop and self.lop.dang_phat:
            self.lop.dat_tham_so(self._so(self.spin_fps, FPS_MAC_DINH, 30), self._so(self.spin_gop, GOP_MAC_DINH, 300))

    # ------------------------------------------------------------ vẽ lại
    def moi_giay(self):
        if self.lop:
            self.lop.thong_ke_giay()                          # cập nhật kbit/s của giây vừa qua
        if self.ui.song:
            self.root.after(1000, self.moi_giay)

    def cap_nhat(self):
        lop = self.lop
        if lop:
            tt = lop.trang_thai()
            self.thanh_mic.dat(tt["muc_mic"])
            x = tt["xem_truoc"]
            if x is not None and x[3] != self.ts_da_ve and tt["dang_phat"]:
                self.anh = anh_tk(x[2], x[0], x[1], 2)
                self.canvas.itemconfigure(self.muc_anh, image=self.anh)
                self.ts_da_ve = x[3]
            elif not tt["dang_phat"]:
                self.canvas.itemconfigure(self.muc_anh, image="")
            self.nhan_tk["ip"].configure(text=f"{tt['so_i']} / {tt['so_p']}")
            self.nhan_tk["tb"].configure(text=f"{tt['tb_i']:.0f} B / {tt['tb_p']:.0f} B")
            self.nhan_tk["pmin"].configure(text=f"{tt['p_min']} B / {tt['p_max']} B")
            self.nhan_tk["kb"].configure(text=f"{tt['kbps_am']:.0f} / {tt['kbps_hinh']:.0f}")
            self.nhan_tk["pli"].configure(text=str(tt["pli_nhan"]))
            if list(self.ds_xem.get(0, "end")) != tt["nguoi_xem"]:
                self.ds_xem.delete(0, "end")
                for t in tt["nguoi_xem"]:
                    self.ds_xem.insert("end", t)
            self.nhan_ds.configure(text=f"Người xem ({len(tt['nguoi_xem'])}):")
            if self.nhan_tt.startswith("đã kết nối") and not tt["da_ket_noi"]:
                self._da_ngat("mất kết nối server")
            trang = (f"ĐANG PHÁT {tt['fps']} fps, GOP {tt['gop']} – {len(tt['nguoi_xem'])} người xem"
                     if tt["dang_phat"] else self.nhan_tt)
            self.thanh_tt.configure(text=f"Trạng thái: {trang}")
        if self.ui.song:
            self.root.after(100, self.cap_nhat)

    def dong(self):
        self.ui.song = False
        if self.lop:
            self.lop.dong()
        self.root.destroy()


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "P"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41652
    root = tk.Tk()
    cs = CuaSoPhat(root, ten, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
