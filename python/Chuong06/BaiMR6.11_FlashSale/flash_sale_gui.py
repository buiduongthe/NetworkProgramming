"""Bài mở rộng MR6.11 – Trang flash sale (ứng dụng người dùng, Tkinter).

Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở giao_dien_chung.CuaSoNguoiDungCoSo;
tệp này vẽ trang "giờ vàng" như sàn thương mại điện tử: dải FLASH SALE, đồng hồ đếm ngược tới giờ mở
bán (chạy cục bộ theo mo_con_ms của cap_nhat), thẻ sản phẩm (ảnh vẽ bằng Canvas, nhãn giảm giá, giá
sale cỡ lớn, giá gốc gạch ngang), thanh tồn kho "Đã bán x%" (cam, "SẮP CHÁY HÀNG" khi > 80 %, "HẾT
HÀNG"), chọn số lượng 1–2, nút MUA NGAY bị khóa tới giờ mở bán, đơn hàng của bạn và "vừa mua" trực tiếp.

Chạy:  python3 flash_sale_gui.py <tên> [host] [cổng]       (mặc định 127.0.0.1 41752)
"""
import sys
import time
import tkinter as tk

from chung import tien
from giao_dien_chung import DO_DAM, NEN, CuaSoNguoiDungCoSo, NhatKy, khung_nhom

DO_SALE, CAM_SALE = "#e4002b", "#ff6a00"


def ve_tai_nghe(cv, cx, cy, k=1.0, nen=True):
    """Ảnh sản phẩm: hộp sạc và đôi tai nghe không dây, tâm (cx, cy), tỉ lệ k."""
    if nen:
        w, h = int(cv["width"]), int(cv["height"])
        cv.create_rectangle(0, 0, w, h, fill="#fff1e6", width=0)
        for i, mau in enumerate(("#ffe0cc", "#ffd1b3", "#ffc299")):
            r = (95 - i * 22) * k
            cv.create_oval(cx - r, cy - r + 10 * k, cx + r, cy + r + 10 * k, fill=mau, width=0)
    x0, x1, y0, y1 = cx - 52 * k, cx + 52 * k, cy + 2 * k, cy + 62 * k          # hộp sạc bo góc
    r = 22 * k
    diem = [x0 + r, y0, x1 - r, y0, x1, y0, x1, y0 + r, x1, y1 - r, x1, y1, x1 - r, y1, x0 + r, y1, x0, y1,
            x0, y1 - r, x0, y0 + r, x0, y0]
    cv.create_polygon(*[d + 3 * k for d in diem], smooth=True, fill="#c9b8a8", outline="")          # bóng
    cv.create_polygon(*diem, smooth=True, fill="#fafafa", outline="#9a9a9a", width=1)
    cv.create_line(x0 + 4 * k, y0 + 22 * k, x1 - 4 * k, y0 + 22 * k, fill="#b8b8b8")
    cv.create_oval(cx - 3 * k, y0 + 34 * k, cx + 3 * k, y0 + 40 * k, fill="#2ecc71", outline="")
    for d in (-1, 1):                                                            # hai tai nghe
        hx, hy = cx + d * 28 * k, cy - 42 * k
        cv.create_polygon(hx - d * 4 * k, hy + 10 * k, hx + d * 6 * k, hy + 10 * k, hx + d * 12 * k, hy + 52 * k,
                          hx + d * 2 * k, hy + 54 * k, fill="#f4f4f4", outline="#9a9a9a", smooth=True)
        cv.create_oval(hx - 17 * k, hy - 15 * k, hx + 17 * k, hy + 17 * k, fill="#ffffff", outline="#9a9a9a")
        cv.create_oval(hx - 8 * k, hy - 6 * k, hx + 8 * k, hy + 8 * k, fill="#3a3a3a", outline="")
        cv.create_oval(hx - 4 * k, hy - 3 * k, hx + 1 * k, hy + 1 * k, fill="#7a7a7a", outline="")


def dong_ho(giay):
    giay = max(0, int(giay + 0.999))
    return f"{giay // 3600:02d}:{giay // 60 % 60:02d}:{giay % 60:02d}"


class CuaSoFlashSale(CuaSoNguoiDungCoSo):
    TIEU_DE = "Flash sale"

    def _ve(self, khung):
        self.mo_luc = None                               # thời điểm mở bán theo đồng hồ máy này
        self.don = []                                    # đơn của tôi
        self.dang_cho = False
        dai = tk.Frame(khung, bg=DO_SALE)
        dai.pack(fill="x", pady=(4, 0))
        tk.Label(dai, text="FLASH SALE", bg=DO_SALE, fg="white",
                 font=("TkDefaultFont", 20, "bold italic")).pack(side="left", padx=(10, 4), pady=4)
        tk.Label(dai, text="GIỜ VÀNG\nGIÁ SỐC", bg=DO_SALE, fg="#ffe066", justify="left",
                 font=("TkDefaultFont", 9, "bold")).pack(side="left")
        f = tk.Frame(dai, bg=DO_SALE)
        f.pack(side="right", padx=10)
        self.nhan_trang_thai = tk.Label(f, text="Bắt đầu sau", bg=DO_SALE, fg="white",
                                        font=("TkDefaultFont", 10, "bold"))
        self.nhan_trang_thai.pack(side="left", padx=(0, 6))
        self.nhan_dong_ho = tk.Label(f, text="--:--:--", bg="#1a1a1a", fg="white", padx=6,
                                     font=("TkFixedFont", 16, "bold"))
        self.nhan_dong_ho.pack(side="left")

        the = tk.Frame(khung, bg="white", highlightthickness=1, highlightbackground="#d0d0d0")
        the.pack(fill="x", pady=4)
        self.cv = tk.Canvas(the, width=230, height=210, highlightthickness=0, bg="white")
        self.cv.pack(side="left", padx=6, pady=6)
        ve_tai_nghe(self.cv, 115, 92)
        phai = tk.Frame(the, bg="white")
        phai.pack(side="left", fill="both", expand=True, padx=(4, 8), pady=6)
        self.nhan_ten = tk.Label(phai, text="–", bg="white", font=("TkDefaultFont", 12, "bold"), anchor="w",
                                 justify="left", wraplength=300)
        self.nhan_ten.pack(anchor="w")
        f = tk.Frame(phai, bg="white")
        f.pack(anchor="w", pady=(4, 0))
        self.nhan_gia = tk.Label(f, text="–", bg="white", fg=DO_SALE, font=("TkDefaultFont", 22, "bold"))
        self.nhan_gia.pack(side="left")
        self.nhan_goc = tk.Label(f, text="", bg="white", fg="#909090", font=("TkDefaultFont", 11, "overstrike"))
        self.nhan_goc.pack(side="left", padx=(8, 0), anchor="s", pady=(0, 4))
        self.nhan_gioi_han = tk.Label(phai, text="", bg="white", fg="#606060", font=("TkDefaultFont", 9))
        self.nhan_gioi_han.pack(anchor="w")
        self.cv_kho = tk.Canvas(phai, width=300, height=24, bg="white", highlightthickness=0)
        self.cv_kho.pack(anchor="w", pady=(6, 2))
        f = tk.Frame(phai, bg="white")
        f.pack(anchor="w", pady=(4, 0))
        tk.Label(f, text="Số lượng:", bg="white").pack(side="left")
        self.bien_sl = tk.IntVar(value=1)
        self.chon_sl = []
        for n in (1, 2):
            r = tk.Radiobutton(f, text=str(n), value=n, variable=self.bien_sl, bg="white", activebackground="white")
            r.pack(side="left")
            self.chon_sl.append(r)
        self.nut_mua = tk.Button(phai, text="CHỜ MỞ BÁN", font=("TkDefaultFont", 14, "bold"), bg=CAM_SALE,
                                 fg="white", activebackground="#ff8533", activeforeground="white",
                                 disabledforeground="#f0f0f0", width=18, relief="raised", bd=2, command=self.mua)
        self.nut_mua.pack(anchor="w", pady=(6, 0))
        self.thong_bao = tk.Label(khung, text="Chưa kết nối", font=("TkDefaultFont", 11, "bold"), bg="#e0e0e0", pady=4)
        self.thong_bao.pack(fill="x")
        f = tk.Frame(khung, bg=NEN)
        f.pack(fill="x", pady=(4, 0))
        g = khung_nhom(f, "Đơn hàng của bạn")
        g.pack(side="left", fill="both", expand=True)
        self.ls_don = tk.Listbox(g, height=4, width=34, font=("TkFixedFont", 9), activestyle="none")
        self.ls_don.pack(fill="both", expand=True)
        g = khung_nhom(f, "Vừa mua (trực tiếp)")
        g.pack(side="left", fill="both", expand=True, padx=(6, 0))
        self.ls_gan = tk.Listbox(g, height=4, width=24, font=("TkFixedFont", 9), activestyle="none")
        self.ls_gan.pack(fill="both", expand=True)
        self._nut()

    # ------------------------------------------------------------ mua
    def mua(self, sl=None):
        if not self.kn or self.dang_cho:
            return
        if sl is not None:
            self.bien_sl.set(sl)
        sl = self.bien_sl.get()
        self.dang_cho = True
        self.nut_mua.configure(text="ĐANG XỬ LÝ…", state="disabled")
        self.nhat_ky.ghi(f"MUA NGAY {sl} sản phẩm")
        self.gui_yc("mua", so_luong=sl)

    def _ket_qua(self, loai, truong, tin, rtt):
        self.dang_cho = False
        if tin.get("ok"):
            self.don.append((tin["ma_don"], tin["so_luong"], tin["thanh_tien"], time.strftime("%H:%M:%S")))
            self._bao(f"Đặt hàng thành công! Đơn {tin['ma_don']} – {tin['so_luong']} sp", "#70d070", "black")
            self.nhat_ky.ghi(f"  đơn {tin['ma_don']}, {tien(tin['thanh_tien'])} ({rtt:.1f} ms)")
            self.ls_don.delete(0, "end")
            for ma, sl, tt, g in reversed(self.don):
                self.ls_don.insert("end", f"{g} {ma} ×{sl} {tien(tt):>11}")
        else:
            m = tin.get("ma_loi")
            text = {"chua_mo": "Chưa tới giờ mở bán!", "het_hang": "Rất tiếc, sản phẩm đã HẾT HÀNG",
                    "khong_du": f"Chỉ còn {tin.get('ton')} sản phẩm – giảm số lượng",
                    "xung_dot": "Hệ thống đang bận, vui lòng thử lại"}.get(m, tin.get("ly_do", ""))
            self._bao(text, "#ff6060" if m == "het_hang" else "#ffb347", "white" if m == "het_hang" else "black")
            self.nhat_ky.ghi(f"  từ chối: {tin.get('ly_do')}")
        self._nut()

    def _bao(self, text, nen, chu):
        self.thong_bao.configure(text=text, bg=nen, fg=chu)

    def _da_mo(self):
        return self.mo_luc is not None and time.monotonic() >= self.mo_luc

    def _nut(self):
        if self.dang_cho:
            return
        if not self.kn or not self.tt:
            self.nut_mua.configure(text="CHỜ MỞ BÁN", state="disabled", bg="#c8c8c8")
        elif self.tt["ton"] <= 0:
            self.nut_mua.configure(text="HẾT HÀNG", state="disabled", bg="#909090")
        elif not self._da_mo():
            self.nut_mua.configure(text="CHỜ MỞ BÁN", state="disabled", bg="#f5b48a")
        else:
            self.nut_mua.configure(text="MUA NGAY", state="normal", bg=CAM_SALE)

    # ------------------------------------------------------------ nhận trạng thái
    def _cap_nhat(self, tt):
        sp = tt["sp"]
        self.mo_luc = time.monotonic() + tt["mo_con_ms"] / 1000
        self.nhan_ten.configure(text=sp["ten"])
        self.nhan_gia.configure(text=tien(sp["gia_sale"]))
        self.nhan_goc.configure(text=tien(sp["gia_goc"]))
        self.nhan_gioi_han.configure(text=f"Mỗi đơn tối đa {sp['toi_da']} sản phẩm · Số lượng có hạn: {tt['ton_dau']}")
        giam = round(100 * (1 - sp["gia_sale"] / sp["gia_goc"]))
        self.cv.delete("nhan")
        self.cv.create_oval(168, 8, 222, 62, fill=DO_SALE, outline="white", width=2, tags="nhan")
        self.cv.create_text(195, 35, text=f"-{giam}%", fill="white", font=("TkDefaultFont", 12, "bold"), tags="nhan")
        if tt["ton"] <= 0:
            self.cv.create_rectangle(0, 170, 230, 205, fill="#404040", outline="", tags="nhan")
            self.cv.create_text(115, 187, text="HẾT HÀNG", fill="white", font=("TkDefaultFont", 14, "bold"),
                                tags="nhan")
        ban = tt["ton_dau"] - tt["ton"]
        ti_le = min(1.0, ban / tt["ton_dau"])
        c = self.cv_kho
        c.delete("all")
        c.create_rectangle(0, 2, 300, 22, fill="#ffd6c2", outline="")
        c.create_rectangle(0, 2, 300 * ti_le, 22, fill=DO_SALE if ti_le >= 0.8 else CAM_SALE, outline="")
        chu = ("HẾT HÀNG" if tt["ton"] <= 0 else
               f"SẮP CHÁY HÀNG – còn {tt['ton']}" if ti_le >= 0.8 else f"ĐÃ BÁN {round(ti_le * 100)}%")
        c.create_text(150, 12, text=chu, fill="white" if ti_le > 0.45 else "#a03000",
                      font=("TkDefaultFont", 10, "bold"))
        ls = [f"{x['gio']} {x['nguoi']:<6} ×{x['so_luong']}" for x in tt["don_gan"]]
        if list(self.ls_gan.get(0, "end")) != ls:
            self.ls_gan.delete(0, "end")
            for i, x in enumerate(ls):
                self.ls_gan.insert("end", x)
                if tt["don_gan"][i]["nguoi"] == self.ten:
                    self.ls_gan.itemconfigure(i, fg="#0040c0")
        if self.thong_bao["text"] == "Chưa kết nối":
            self._bao("Đón giờ vàng – nút MUA NGAY mở khi hết đếm ngược", "#e0e0e0", "black")
        self._moi_nhip()

    def _su_kien(self, text):
        super()._su_kien(text)
        if text.startswith("Đặt lại"):                   # đợt sale mới
            self.don.clear()
            self.ls_don.delete(0, "end")
            self._bao("Đợt sale mới – chờ giờ mở bán", "#e0e0e0", "black")

    def _moi_nhip(self):
        if not self.tt or self.mo_luc is None:
            return
        con = self.mo_luc - time.monotonic()
        if con > 0:
            self.nhan_trang_thai.configure(text="Bắt đầu sau")
            self.nhan_dong_ho.configure(text=dong_ho(con), bg="#1a1a1a")
        elif self.tt["ton"] <= 0:
            self.nhan_trang_thai.configure(text="Đã kết thúc")
            self.nhan_dong_ho.configure(text="CHÁY HÀNG", bg="#404040")
        else:
            self.nhan_trang_thai.configure(text="")
            self.nhan_dong_ho.configure(text="ĐANG DIỄN RA", bg=DO_DAM)
        self._nut()

    def _dong_dinh_ky(self):
        tt = self.tt
        con = max(0.0, (self.mo_luc or 0) - time.monotonic())
        return (f"tồn {tt['ton']}/{tt['ton_dau']}, " + (f"mở sau {con:.0f} s" if con > 0 else "đang bán")
                + f"; bạn có {len(self.don)} đơn")


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "An"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41752
    root = tk.Tk()
    cs = CuaSoFlashSale(root, ten, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
