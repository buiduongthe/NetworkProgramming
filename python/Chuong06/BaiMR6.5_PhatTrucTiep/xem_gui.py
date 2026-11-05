"""Bài mở rộng MR6.5 – Người xem phát trực tiếp có giao diện Tkinter.

Lõi NguoiXem (nguoi_xem.py) nhận, giải mã, phát âm thanh (đồng hồ chủ) và chọn khung để hiện; cửa
sổ cứ 50 ms đọc khung đang hiện để vẽ (phóng ×2) và cứ 200 ms cập nhật thanh mức, thống kê.
Điều khiển: Kết nối/Ngắt, công tắc Đồng bộ tiếng-hình, chọn nhận Âm thanh / Video / Cả hai, độ sâu
bộ đệm jitter, Phát ra loa (có sounddevice và thiết bị phát), Chụp (lưu out/<tên>_chup_N.png).

Chạy:  python3 xem_gui.py [tên] [host] [cổng_tcp]
"""
import os
import sys
import threading
import tkinter as tk

from chung import CAO, DO_SAU_MAC_DINH, RONG, ghi_png
from giao_dien_chung import (DO, NEN, XANH_DUONG, XANH_LA, HangDoiGiaoDien, NhatKy, ThanhMuc, anh_tk,
                             chu_ms, khung_nhom, nhan, nut, o_nhap)
from nguoi_xem import NguoiXem, anh_chup
from thiet_bi_am_thanh import mo_loa, nap_sounddevice


class CuaSoXem:
    def __init__(self, root, ten="A", host="127.0.0.1", cong=41652):
        self.root = root
        self.ui = HangDoiGiaoDien(root)
        sd, _ = nap_sounddevice()
        self.loa, ly_do_loa = mo_loa(sd)
        self.lop = None
        self.anh = None
        self.so_da_ve = -1
        self.so_chup = 0
        root.title(f"Người xem trực tiếp – MR6.5 ({ten})")
        root.configure(bg=NEN)
        root.resizable(False, False)

        g = khung_nhom(root, "Kết nối server")
        g.pack(fill="x", padx=6, pady=4)
        nhan(g, "IP:").pack(side="left")
        self.o_ip = o_nhap(g, host, 13)
        self.o_ip.pack(side="left")
        nhan(g, " Port:").pack(side="left")
        self.o_cong = o_nhap(g, cong, 6)
        self.o_cong.pack(side="left")
        nhan(g, " Tên:").pack(side="left")
        self.o_ten = o_nhap(g, ten, 8)
        self.o_ten.pack(side="left")
        self.nut_ket_noi = nut(g, "Kết nối", XANH_LA, self.bam_ket_noi, 13)
        self.nut_ket_noi.pack(side="left", padx=8)

        giua = tk.Frame(root, bg=NEN)
        giua.pack(fill="x", padx=6)
        g = khung_nhom(giua, "Hình (phóng ×2)")
        g.pack(side="left", fill="y")
        self.canvas = tk.Canvas(g, width=2 * RONG, height=2 * CAO, bg="black", highlightthickness=0)
        self.canvas.pack()
        self.muc_anh = self.canvas.create_image(0, 0, anchor="nw")
        self.chu_cho = self.canvas.create_text(RONG, CAO, text="Chưa kết nối", fill="#c0c0c0")
        self.nhan_khung = nhan(g, "Khung: –", font=("TkFixedFont", 9))
        self.nhan_khung.pack(anchor="w")
        f = tk.Frame(g, bg=NEN)
        f.pack(fill="x")
        nhan(f, "Tiếng:").pack(side="left")
        self.thanh = ThanhMuc(f, rong=200)
        self.thanh.pack(side="left", padx=4)
        self.bien_loa = tk.BooleanVar(value=self.loa is not None)
        self.o_loa = tk.Checkbutton(f, text="Loa", variable=self.bien_loa, bg=NEN, command=self.ap_loa,
                                    state="normal" if self.loa else "disabled")
        self.o_loa.pack(side="left")
        self.nhan_loa = nhan(g, "Loa: THẬT – thiết bị mặc định" if self.loa else f"Loa: {ly_do_loa}",
                             fg="#006000" if self.loa else "#8a4b00")
        self.nhan_loa.pack(anchor="w")

        phai = tk.Frame(giua, bg=NEN)
        phai.pack(side="left", fill="both", expand=True, padx=(6, 0))
        g = khung_nhom(phai, "Điều khiển")
        g.pack(fill="x")
        self.bien_dong_bo = tk.BooleanVar(value=True)
        tk.Checkbutton(g, text="Đồng bộ tiếng-hình (tiếng làm đồng hồ chủ)", variable=self.bien_dong_bo,
                       bg=NEN, command=self.ap_dong_bo).pack(anchor="w")
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w")
        nhan(f, "Nhận:").pack(side="left")
        self.bien_nhan = tk.StringVar(value="ca_hai")
        for gt, t in (("am_thanh", "Âm thanh"), ("video", "Video"), ("ca_hai", "Cả hai")):
            tk.Radiobutton(f, text=t, value=gt, variable=self.bien_nhan, bg=NEN,
                           command=self.ap_dang_ky).pack(side="left")
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w")
        nhan(f, "Bộ đệm jitter (khung 20 ms):").pack(side="left")
        self.spin_dem = tk.Spinbox(f, from_=1, to=25, width=4, command=self.ap_do_sau)
        self.spin_dem.delete(0, "end")
        self.spin_dem.insert(0, str(DO_SAU_MAC_DINH))
        self.spin_dem.pack(side="left")
        self.nut_chup = nut(f, "Chụp", XANH_DUONG, self.chup, 7)
        self.nut_chup.pack(side="left", padx=8)

        g = khung_nhom(phai, "Thống kê")
        g.pack(fill="both", expand=True, pady=(4, 0))
        self.nhan_tk = {}
        for k, t in (("tre", "Trễ đầu-cuối (tiếng)*"), ("lech", "Lệch tiếng-hình hiện tại"),
                     ("lech_tb", "Lệch TB giai đoạn này"), ("mat", "Tiếng nhận / mất"),
                     ("khung", "Khung nhận I / P"), ("dung", "Đứng hình / PLI gửi"), ("dau", "Khung đầu sau")):
            f = tk.Frame(g, bg=NEN)
            f.pack(fill="x")
            nhan(f, t + ":", width=22).pack(side="left")
            self.nhan_tk[k] = nhan(f, "–", font=("TkDefaultFont", 10, "bold"))
            self.nhan_tk[k].pack(side="left")
        nhan(g, "* giả định cùng đồng hồ (cùng máy / NTP); lệch +: hình chậm hơn tiếng",
             font=("TkDefaultFont", 8), fg="#404040").pack(anchor="w")

        g = khung_nhom(root, "Nhật ký")
        g.pack(fill="x", padx=6, pady=4)
        self.nhat_ky = NhatKy(g, cao=5, rong=70)
        self.nhat_ky.pack(fill="x")
        self.thanh_tt = tk.Label(root, text="Trạng thái: chưa kết nối", anchor="w", relief="sunken", bg="white")
        self.thanh_tt.pack(fill="x", padx=6, pady=(0, 6))
        self.nhan_tt = "chưa kết nối"
        root.after(50, self.ve_hinh)
        root.after(200, self.cap_nhat)

    def _ghi(self, text):
        self.ui.dua_vao(self.nhat_ky.ghi, text)

    # ------------------------------------------------------------ kết nối
    def bam_ket_noi(self):
        if self.lop and self.lop.da_ket_noi:
            self.lop.ngat()
            self._da_ngat("đã ngắt kết nối")
            return
        ten, host = self.o_ten.get().strip(), self.o_ip.get().strip()
        try:
            cong = int(self.o_cong.get())
        except ValueError:
            self.nhan_tt = "cổng không hợp lệ"
            return
        if self.lop:
            self.lop.loa = None
            self.lop.dong()
        nhan_ = self.bien_nhan.get()
        self.lop = NguoiXem(ten, bao=self._ghi, do_sau=self._do_sau(), dong_bo=self.bien_dong_bo.get(),
                            am_thanh=nhan_ != "video", video=nhan_ != "am_thanh",
                            loa=self.loa if self.bien_loa.get() else None)
        self.nut_ket_noi.configure(state="disabled", text="Đang kết nối…")
        self.so_da_ve = -1
        lop = self.lop

        def nen():
            try:
                self.ui.dua_vao(self._da_ket_noi, lop.ket_noi(host, cong))
            except OSError as e:
                self.ui.dua_vao(self._da_ngat, f"không kết nối được – {e}")
        threading.Thread(target=nen, daemon=True).start()

    def _da_ket_noi(self, ma):
        self.nut_ket_noi.configure(state="normal", text="Ngắt kết nối", bg=DO, activebackground=DO)
        self.nhan_tt = f"đã kết nối, mã {ma}"
        self.canvas.itemconfigure(self.chu_cho, text="Chờ khung hình…")
        for o in (self.o_ip, self.o_cong, self.o_ten):
            o.configure(state="disabled")

    def _da_ngat(self, text):
        self.nut_ket_noi.configure(state="normal", text="Kết nối", bg=XANH_LA, activebackground=XANH_LA)
        self.nhan_tt = text
        for o in (self.o_ip, self.o_cong, self.o_ten):
            o.configure(state="normal")

    # ------------------------------------------------------------ điều khiển
    def _do_sau(self):
        try:
            return max(1, min(25, int(self.spin_dem.get())))
        except ValueError:
            return DO_SAU_MAC_DINH

    def ap_dong_bo(self):
        if self.lop:
            self.lop.dat_dong_bo(self.bien_dong_bo.get())

    def ap_dang_ky(self):
        if self.lop:
            v = self.bien_nhan.get()
            self.lop.dang_ky(v != "video", v != "am_thanh")

    def ap_do_sau(self):
        if self.lop:
            self.lop.dat_do_sau(self._do_sau())
            self.nhat_ky.ghi(f"Bộ đệm jitter: {self._do_sau()} khung = {self._do_sau() * 20} ms")

    def ap_loa(self):
        if self.lop:
            self.lop.loa = self.loa if self.bien_loa.get() else None

    def chup(self):
        c = self.lop.chup() if self.lop else None
        if c is None:
            self.nhat_ky.ghi("Chưa có hình để chụp")
            return None
        w, h, rgb = anh_chup(c)
        os.makedirs("out", exist_ok=True)
        self.so_chup += 1
        duong = f"out/{self.lop.ten}_gui_chup_{self.so_chup}.png"
        ghi_png(duong, rgb, w, h)
        self.nhat_ky.ghi(f"Đã chụp {duong}")
        return duong

    # ------------------------------------------------------------ vẽ lại
    def ve_hinh(self):
        lop = self.lop
        if lop:
            with lop.khoa:
                hien, so = lop.hien, lop.so_hien
            if hien is not None and so != self.so_da_ve:
                w, h, rgb, ts, so_k, am = hien
                self.anh = anh_tk(rgb, w, h, 2)
                self.canvas.itemconfigure(self.muc_anh, image=self.anh)
                self.canvas.itemconfigure(self.chu_cho, text="")
                self.nhan_khung.configure(text=f"Khung #{so_k}  hình t={ts / 1000:.3f} s  tiếng t="
                                          + ("–" if am is None else f"{am / 1000:.3f} s"))
                self.so_da_ve = so
            elif hien is None and self.so_da_ve != -2:
                self.canvas.itemconfigure(self.muc_anh, image="")
                self.so_da_ve = -2
        if self.ui.song:
            self.root.after(50, self.ve_hinh)

    def cap_nhat(self):
        lop = self.lop
        if lop:
            tt = lop.trang_thai()
            self.thanh.dat(tt["muc_am"])
            g = tt["giai_doan"][-1]
            self.nhan_tk["tre"].configure(text=chu_ms(tt["tre_hien"]))
            self.nhan_tk["lech"].configure(text=chu_ms(tt["lech_hien"], True),
                                           fg="#006000" if tt["lech_hien"] is not None and abs(tt["lech_hien"]) <= 40
                                           else "#b00000")
            self.nhan_tk["lech_tb"].configure(text=(chu_ms(g["tong"] / g["n"], True) if g["n"] else "–")
                                              + f" (đồng bộ {'BẬT' if g['dong_bo'] else 'TẮT'})")
            self.nhan_tk["mat"].configure(text=f"{tt['goi_am']} / {tt['mat_am']} gói")
            self.nhan_tk["khung"].configure(text=f"{tt['khung_i']} / {tt['khung_p']} (hỏng {tt['hong']})")
            self.nhan_tk["dung"].configure(text=f"{tt['dung_hinh']} / {tt['pli_gui']}")
            self.nhan_tk["dau"].configure(text="–" if tt["khung_dau_ms"] is None else
                                          f"{tt['khung_dau_ms']:.0f} ms ({tt['nguon_dau']})")
            if self.nhan_tt.startswith("đã kết nối") and not tt["da_ket_noi"]:
                self._da_ngat("mất kết nối server")
            trang = (f"đang xem – nhận {tt['kbps']:.0f} kbit/s TB" if tt["da_ket_noi"] else self.nhan_tt)
            self.thanh_tt.configure(text=f"Trạng thái: {trang}")
        if self.ui.song:
            self.root.after(200, self.cap_nhat)

    def dong(self):
        self.ui.song = False
        if self.lop:
            self.lop.loa = None
            self.lop.dong()
        if self.loa:
            self.loa.dong()
        self.root.destroy()


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "A"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41652
    root = tk.Tk()
    cs = CuaSoXem(root, ten, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    main()
