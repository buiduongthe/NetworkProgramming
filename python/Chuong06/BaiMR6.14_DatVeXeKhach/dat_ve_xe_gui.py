"""Bài mở rộng MR6.14 – Ứng dụng đặt vé xe khách theo chặng (người dùng, Tkinter).

Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở giao_dien_chung.CuaSoNguoiDungCoSo;
tệp này vẽ ứng dụng đặt vé: băng rôn nhà xe, chọn "Đi từ" – "Đến" (thanh tuyến Sài Gòn – Bảo Lộc – Đà Lạt –
Nha Trang tô đoạn đã chọn), sơ đồ xe (giường nằm 2 tầng hoặc ghế ngồi 50 chỗ, có vô lăng ở đầu xe). Mỗi
chỗ tô màu theo ĐOẠN đang chọn: trống / đang chọn / người khác giữ / đã bán / bạn giữ / vé của bạn, và có
dải nhỏ ở chân chỗ, mỗi chặng một vạch, cho thấy chỗ đó bận ở chặng nào (cùng một giường có thể đã bán
Đà Lạt → Nha Trang mà vẫn trống Sài Gòn → Đà Lạt). Bấm chỗ để chọn, "GIỮ CHỖ" gửi giu_cho, đồng hồ giữ chỗ
đếm ngược (đỏ khi < 10 s), "THANH TOÁN" và "Nhả chỗ". Sơ đồ cập nhật trực tiếp từ cap_nhat (≤ 10 lần/giây).

Chạy:  python3 dat_ve_xe_gui.py <tên> [host] [cổng]       (mặc định 127.0.0.1 41782)
"""
import sys
import time
import tkinter as tk

from chung import tien
from giao_dien_chung import DO_DAM, NEN, XANH_DAM, CuaSoNguoiDungCoSo, NhatKy
from may_chu import MA_TRAM, TRAM, bo_tri, ten_doan

TRONG, CHON, KHAC_GIU, DA_BAN, TOI_GIU, VE_TOI = "#ffffff", "#b9f6ca", "#ffa726", "#9e9e9e", "#2e7d32", "#1565c0"
MAU_XE = "#0b5d3b"
CHU_GIAI = [(TRONG, "Trống"), (CHON, "Đang chọn"), (KHAC_GIU, "Người khác giữ"), (DA_BAN, "Đã bán"),
            (TOI_GIU, "Bạn giữ"), (VE_TOI, "Vé của bạn")]
MAU_VACH = {".": "#dfe6e2", "g": KHAC_GIU, "b": DA_BAN, "G": TOI_GIU, "V": VE_TOI}


def dong_ho(giay):
    giay = max(0, int(giay + 0.999))
    return f"{giay // 60:02d}:{giay % 60:02d}"


def trang_cho(tt, g, ten, tu, den):
    """Trạng thái chỗ g cho đoạn [tu, den): 'trong' | 'giu' | 'ban' | 'toi_giu' | 've'."""
    dong = tt["so_do"][tt["ghe"].index(g)]
    kq = "trong"
    for k in range(tu, den):
        x, ai = dong[k], tt["chu"].get(f"{g}#{k}")
        if x == "b" and ai != ten:
            return "ban"
        if x == "g" and ai != ten:
            kq = "giu"
        elif x == "g" and kq == "trong":
            kq = "toi_giu"
        elif x == "b" and kq in ("trong", "toi_giu"):
            kq = "ve"
    return kq


def vach(tt, g, ten):
    """Một ký tự mỗi chặng: . g b (người khác) G V (của mình)."""
    dong = tt["so_do"][tt["ghe"].index(g)]
    return "".join(({"g": "G", "b": "V"}.get(x, x) if tt["chu"].get(f"{g}#{k}") == ten else x)
                   for k, x in enumerate(dong))


class SoDoXe:
    """Vẽ sơ đồ xe lên Canvas; dùng ở ứng dụng đặt vé (to) và bảng điều khiển máy chủ (thu nhỏ, ty_le < 1)."""

    def __init__(self, cv, loai_xe, ty_le=1.0, chu=True):
        self.cv, self.loai_xe, self.k, self.chu = cv, loai_xe, ty_le, chu
        self.bo_tri = bo_tri(loai_xe)
        k = ty_le
        if loai_xe == "giuong":
            w, h, khe, rong_tang = 46 * k, 30 * k, 6 * k, 0
            rong_tang = 3 * w + 2 * khe
            self.vi_tri = {}
            for ma, tang, hang, cot in self.bo_tri:
                x = 12 * k + tang * (rong_tang + 26 * k) + cot * (w + khe)
                y = 40 * k + hang * (h + khe)
                self.vi_tri[ma] = (x, y, x + w, y + h)
            self.rong = 24 * k + 2 * rong_tang + 26 * k
            self.cao = 40 * k + 7 * (h + khe) + 4 * k
            self.tang = [(12 * k, 12 * k + rong_tang, "TẦNG DƯỚI"), (12 * k + rong_tang + 26 * k,
                                                                    12 * k + 2 * rong_tang + 26 * k, "TẦNG TRÊN")]
        else:
            w, h, khe = 34 * k, 26 * k, 5 * k
            self.vi_tri = {}
            for ma, _, hang, cot in self.bo_tri:
                x = 12 * k + cot * (w + khe)
                y = 40 * k + hang * (h + khe)
                self.vi_tri[ma] = (x, y, x + w, y + h)
            self.rong = 24 * k + 6 * (w + khe) - khe
            self.cao = 40 * k + 12 * (h + khe) + 4 * k
            self.tang = [(12 * k, 12 * k + 6 * (w + khe) - khe, "")]
        self.ve_nen()

    def ve_nen(self):
        cv, k = self.cv, self.k
        cv.delete("nen")
        cv.create_rectangle(0, 0, self.rong + 400, self.cao + 400, fill="#e9efeb", width=0, tags="nen")
        for x0, x1, ten in self.tang:
            cv.create_rectangle(x0 - 6 * k, 8 * k, x1 + 6 * k, self.cao, outline=MAU_XE, width=max(1, 2 * k),
                                fill="#f7faf8", tags="nen")
            if self.chu and ten:
                cv.create_text((x0 + x1) / 2, 20 * k, text=ten, fill=MAU_XE, font=("TkDefaultFont", 8, "bold"),
                               tags="nen")
        x0 = self.tang[0][0]
        r = 9 * k                                              # vô lăng ở đầu xe (bên trái, tầng dưới)
        cx, cy = x0 + r + 2 * k, 22 * k
        if self.chu or k >= 0.5:
            cv.create_oval(cx - r, cy - r, cx + r, cy + r, outline="#37474f", width=max(1, 2 * k), tags="nen")
            cv.create_line(cx - r, cy, cx + r, cy, fill="#37474f", width=max(1, 2 * k), tags="nen")

    def ve(self, tt, ten, tu, den, chon=(), vien=None):
        """Tô mọi chỗ theo đoạn [tu, den) và vẽ dải vạch theo chặng; vien: {chỗ: màu viền} (ghế nóng)."""
        cv, k = self.cv, self.k
        cv.delete("ghe")
        for g, (x0, y0, x1, y1) in self.vi_tri.items():
            if tt and g in tt["ghe"]:
                trang = trang_cho(tt, g, ten, tu, den)
                vach_g = vach(tt, g, ten)
            else:
                trang, vach_g = "trong", "." * (len(TRAM) - 1)
            mau = {"trong": CHON if g in chon else TRONG, "giu": KHAC_GIU, "ban": DA_BAN, "toi_giu": TOI_GIU,
                   "ve": VE_TOI}[trang]
            v = (vien or {}).get(g, "#607d74")
            cv.create_rectangle(x0, y0, x1, y1, fill=mau, outline=v, width=2 if g in (vien or {}) else 1,
                                tags=("ghe", f"g_{g}"))
            n = len(vach_g)                                    # dải vạch: mỗi chặng một ô ở chân chỗ
            cao = max(2, 5 * k)
            for i, x in enumerate(vach_g):
                a = x0 + 2 + (x1 - x0 - 4) * i / n
                b = x0 + 2 + (x1 - x0 - 4) * (i + 1) / n - 1
                cv.create_rectangle(a, y1 - cao - 1, b, y1 - 1, fill=MAU_VACH[x], width=1 if tu <= i < den else 0,
                                    outline="#263238", tags="ghe")
            if self.chu:
                toi = mau in (TOI_GIU, VE_TOI, DA_BAN)
                cv.create_text((x0 + x1) / 2, (y0 + y1) / 2 - 3 * k, text=g, fill="white" if toi else "#263238",
                               font=("TkDefaultFont", 7), tags="ghe")

    def cho_tai(self, x, y):
        for g, (x0, y0, x1, y1) in self.vi_tri.items():
            if x0 <= x <= x1 and y0 <= y <= y1:
                return g
        return None


class CuaSoDatVeXe(CuaSoNguoiDungCoSo):
    TIEU_DE = "Đặt vé xe khách"

    def _ve(self, khung):
        self.chon = set()                # chỗ đang chọn (chưa giữ)
        self.giu = set()                 # chỗ mình đang giữ (theo kq), cho đoạn self.doan_giu
        self.doan_giu = None
        self.phien_giu = self.phien_ve = 0   # số thứ tự lần ghi (kq) của lần giữ chỗ / thanh toán gần nhất
        self.ve = []                     # vé của mình: (chỗ, tu, den, mã vé)
        self.han = None                  # thời điểm (monotonic) hết hạn giữ chỗ
        self.so_giu = self.so_ve = 0
        self.tu, self.den = 0, len(TRAM) - 1
        trai = tk.Frame(khung, bg=NEN)
        trai.pack(side="left", pady=4, anchor="n")
        self.cv = tk.Canvas(trai, width=376, height=358, highlightthickness=0, bg="#e9efeb")
        self.cv.pack()
        self.so_do = None
        self.cv.bind("<Button-1>", self._bam_cho)

        phai = tk.Frame(khung, bg="white", bd=1, relief="solid")
        phai.pack(side="left", fill="both", expand=True, padx=(8, 0), pady=4)
        self.cv_xe = tk.Canvas(phai, width=256, height=62, highlightthickness=0, bg=MAU_XE)
        self.cv_xe.pack(padx=6, pady=(6, 2))
        f = tk.Frame(phai, bg="white")
        f.pack(fill="x", padx=8, pady=(4, 0))
        self.bien_tu, self.bien_den = tk.StringVar(value=TRAM[0]), tk.StringVar(value=TRAM[-1])
        tk.Label(f, text="Đi từ", bg="white", font=("TkDefaultFont", 9)).grid(row=0, column=0, sticky="w")
        self.menu_tu = tk.OptionMenu(f, self.bien_tu, *TRAM[:-1], command=lambda _: self._doi_doan())
        self.menu_tu.configure(width=9, bg="white", highlightthickness=0)
        self.menu_tu.grid(row=0, column=1, sticky="w")
        tk.Label(f, text="Đến", bg="white", font=("TkDefaultFont", 9)).grid(row=1, column=0, sticky="w")
        self.menu_den = tk.OptionMenu(f, self.bien_den, *TRAM[1:], command=lambda _: self._doi_doan())
        self.menu_den.configure(width=9, bg="white", highlightthickness=0)
        self.menu_den.grid(row=1, column=1, sticky="w")
        self.cv_tuyen = tk.Canvas(phai, width=256, height=40, highlightthickness=0, bg="white")
        self.cv_tuyen.pack(padx=6)
        self.nhan_ghe = tk.Label(phai, text="Chỗ đã chọn: –", bg="white", anchor="w", font=("TkDefaultFont", 10, "bold"))
        self.nhan_ghe.pack(fill="x", padx=8)
        self.nhan_tien = tk.Label(phai, text="Tạm tính: 0 đ", bg="white", anchor="w", font=("TkDefaultFont", 9))
        self.nhan_tien.pack(fill="x", padx=8)
        self.nut_giu = tk.Button(phai, text="GIỮ CHỖ", bg="#ff8f00", fg="white", activebackground="#ef6c00",
                                 activeforeground="white", font=("TkDefaultFont", 11, "bold"), relief="flat",
                                 command=self.giu_cho, disabledforeground="#ffe0b2")
        self.nut_giu.pack(fill="x", padx=8, pady=(4, 2))
        f = tk.Frame(phai, bg="white")
        f.pack(fill="x", padx=8)
        tk.Label(f, text="Giữ chỗ còn", bg="white", font=("TkDefaultFont", 9)).pack(side="left")
        self.nhan_dong_ho = tk.Label(f, text="--:--", bg="white", fg="#909090", font=("TkFixedFont", 18, "bold"))
        self.nhan_dong_ho.pack(side="left", padx=6)
        self.nut_tt = tk.Button(phai, text="THANH TOÁN", bg=MAU_XE, fg="white", activebackground="#06402a",
                                activeforeground="white", font=("TkDefaultFont", 11, "bold"), relief="flat",
                                command=self.thanh_toan, disabledforeground="#c8e6c9")
        self.nut_tt.pack(fill="x", padx=8, pady=2)
        self.nut_nha = tk.Button(phai, text="Nhả chỗ", bg="#f0f0f0", relief="raised", bd=1, command=self.nha)
        self.nut_nha.pack(anchor="w", padx=8, pady=2)
        self.thong_bao = tk.Label(phai, text="Chưa kết nối", font=("TkDefaultFont", 9, "bold"), bg="#e8e8e8",
                                  pady=4, wraplength=246)
        self.thong_bao.pack(fill="x", padx=8, pady=2)
        self.nhan_ve = tk.Label(phai, text="Vé của bạn: –", bg="white", anchor="w", justify="left",
                                font=("TkDefaultFont", 9), fg=VE_TOI, wraplength=246)
        self.nhan_ve.pack(fill="x", padx=8, pady=(0, 6))
        self._ve_tuyen()
        self._bat_nut()

    # ------------------------------------------------------------ băng rôn, tuyến, sơ đồ
    def _ve_bang_ron(self, tt):
        cv = self.cv_xe
        cv.delete("all")
        cv.create_rectangle(0, 44, 256, 62, fill="#0f7a4f", width=0)
        cv.create_text(10, 14, text=tt["nha_xe"].upper(), anchor="w", fill="white", font=("TkDefaultFont", 11, "bold"))
        cv.create_text(10, 33, text=f"Chuyến {tt['chuyen']} · {tt['ten_xe']}", anchor="w", fill="#d7f0e2",
                       font=("TkDefaultFont", 8))
        cv.create_text(10, 53, text=f"Giữ chỗ {tt['ttl_s']:g} s · giá theo chặng", anchor="w", fill="#ffe082",
                       font=("TkDefaultFont", 8, "bold"))

    def _ve_tuyen(self):
        cv = self.cv_tuyen
        cv.delete("all")
        n = len(TRAM)
        x = [16 + i * (224 / (n - 1)) for i in range(n)]
        cv.create_line(x[0], 12, x[-1], 12, fill="#cfd8dc", width=6)
        cv.create_line(x[self.tu], 12, x[self.den], 12, fill=MAU_XE, width=6)
        gia = self.tt["gia_chang"] if self.tt else None
        for i in range(n):
            trong_doan = self.tu <= i <= self.den
            cv.create_oval(x[i] - 6, 6, x[i] + 6, 18, fill=MAU_XE if trong_doan else "white", outline=MAU_XE, width=2)
            cv.create_text(x[i], 30, text=MA_TRAM[i], fill="#263238", font=("TkDefaultFont", 8, "bold"))
            if gia and i < n - 1:
                cv.create_text((x[i] + x[i + 1]) / 2, 30, text=f"{gia[i] // 1000}k", fill="#607d74",
                               font=("TkDefaultFont", 7))

    def _ve_cho(self):
        tt = self.tt
        if not tt:
            return
        if self.so_do is None or self.so_do.loai_xe != tt["loai_xe"]:
            self.cv.delete("all")
            self.so_do = SoDoXe(self.cv, tt["loai_xe"])
            self.cv.configure(width=max(376, self.so_do.rong + 8), height=self.so_do.cao + 62)
            y = self.so_do.cao + 10
            x = 8
            for i, (mau, chu) in enumerate(CHU_GIAI):            # chú giải màu, 2 hàng
                if i == 3:
                    x, y = 8, y + 16
                self.cv.create_rectangle(x, y, x + 14, y + 11, fill=mau, outline="#607d74")
                self.cv.create_text(x + 18, y + 6, text=chu, anchor="w", fill="#263238", font=("TkDefaultFont", 8))
                x += 30 + len(chu) * 6
            self.cv.create_text(8, y + 22, text="Vạch ở chân chỗ: từng chặng " + " | ".join(
                f"{a}-{b}" for a, b in zip(MA_TRAM, MA_TRAM[1:])), anchor="w", fill="#455a64", font=("TkDefaultFont", 7))
        self.so_do.ve(tt, self.ten, self.tu, self.den, chon=self.chon)

    # ------------------------------------------------------------ chọn đoạn, chọn chỗ
    def dat_doan(self, tu, den):
        """Chọn đoạn đường (người dùng hoặc kịch bản demo): chỉ số trạm."""
        self.bien_tu.set(TRAM[tu])
        self.bien_den.set(TRAM[den])
        self._doi_doan()

    def _doi_doan(self):
        tu, den = TRAM.index(self.bien_tu.get()), TRAM.index(self.bien_den.get())
        if den <= tu:                                          # "Đến" phải sau "Đi từ"
            den = tu + 1
            self.bien_den.set(TRAM[den])
        self.tu, self.den = tu, den
        if self.tt:
            bo = [g for g in self.chon if trang_cho(self.tt, g, self.ten, tu, den) != "trong"]
            for g in bo:
                self.chon.discard(g)
            if bo:
                self._bao(f"Chỗ {', '.join(sorted(bo))} không trống cả đoạn {ten_doan(tu, den)}", "#ffe0b2")
        self._ve_tuyen()
        self._cap_nhat_gio()

    def _bam_cho(self, e):
        g = self.so_do.cho_tai(e.x, e.y) if self.so_do else None
        if g:
            self.bam(g)

    def bam(self, g):
        """Bấm một chỗ (người dùng hoặc kịch bản demo)."""
        if not self.kn or not self.tt:
            return
        trang = trang_cho(self.tt, g, self.ten, self.tu, self.den)
        if g in self.chon:
            self.chon.discard(g)
        elif trang == "trong" and g not in self.giu:
            if len(self.chon) + len(self.giu) >= 6:
                self._bao("Tối đa 6 chỗ mỗi lần", "#ffe0b2")
                return
            self.chon.add(g)
        elif trang == "giu":
            self._bao(f"Chỗ {g} đang có người giữ trên đoạn này", "#ffe0b2")
        elif trang == "ban":
            self._bao(f"Chỗ {g} đã bán trên đoạn này", "#e0e0e0")
        self._cap_nhat_gio()

    def _gia(self, so_cho):
        return sum(self.tt["gia_chang"][self.tu:self.den]) * so_cho if self.tt else 0

    def _cap_nhat_gio(self):
        if self.giu:
            tu, den = self.doan_giu
            gia = sum(self.tt["gia_chang"][tu:den]) * len(self.giu) if self.tt else 0
            self.nhan_ghe.configure(text=f"Đang giữ: {', '.join(sorted(self.giu))}")
            self.nhan_tien.configure(text=f"{ten_doan(tu, den)} · tổng {tien(gia)}")
            self.nut_tt.configure(text=f"THANH TOÁN {tien(gia)}")
        else:
            self.nhan_ghe.configure(text=f"Chỗ đã chọn: {', '.join(sorted(self.chon)) or '–'}")
            self.nhan_tien.configure(text=f"{ten_doan(self.tu, self.den)} · tạm tính {tien(self._gia(len(self.chon)))}")
            self.nut_tt.configure(text="THANH TOÁN")
        self._bat_nut()
        self._ve_cho()

    def _bat_nut(self):
        co = self.kn is not None
        con_han = self.han is not None and time.monotonic() < self.han
        self.nut_giu.configure(state="normal" if co and self.chon and not self.giu else "disabled",
                               bg="#ff8f00" if co and self.chon and not self.giu else "#ffcc80")
        self.nut_tt.configure(state="normal" if co and self.giu and con_han else "disabled",
                              bg=MAU_XE if co and self.giu and con_han else "#8fb8a3")
        self.nut_nha.configure(state="normal" if co and self.giu else "disabled")
        menu = "disabled" if self.giu else "normal"           # đang giữ chỗ: không đổi đoạn
        self.menu_tu.configure(state=menu)
        self.menu_den.configure(state=menu)

    def _bao(self, text, nen, chu="black"):
        self.thong_bao.configure(text=text, bg=nen, fg=chu)

    # ------------------------------------------------------------ giữ / thanh toán / nhả
    def giu_cho(self):
        if not self.chon or self.giu:
            return
        ghe = sorted(self.chon)
        self.nhat_ky.ghi(f"Giữ chỗ {', '.join(ghe)} {ten_doan(self.tu, self.den)}…")
        self._bao("Đang giữ chỗ…", "#fff3c4")
        self.nut_giu.configure(state="disabled")
        self.gui_yc("giu_cho", ghe=ghe, tu=self.tu, den=self.den)

    def thanh_toan(self):
        if self.giu:
            tu, den = self.doan_giu
            self.nhat_ky.ghi(f"Thanh toán {', '.join(sorted(self.giu))}…")
            self.gui_yc("thanh_toan", ghe=sorted(self.giu), tu=tu, den=den)

    def nha(self):
        if self.giu:
            tu, den = self.doan_giu
            self.gui_yc("nha", ghe=sorted(self.giu), tu=tu, den=den)

    def _ket_qua(self, loai, truong, tin, rtt):
        ghe, tu, den = truong["ghe"], truong["tu"], truong["den"]
        if loai == "giu_cho":
            if tin.get("ok"):
                self.so_giu += 1
                self.giu, self.doan_giu, self.phien_giu = set(tin["ghe"]), (tu, den), tin.get("phien", 0)
                self.chon -= self.giu
                self.han = time.monotonic() + tin["het_han_ms"] / 1000
                self._bao(f"Đã giữ {', '.join(ghe)} – thanh toán trong {tin['het_han_ms'] / 1000:.0f} s",
                          "#c8f0d2", XANH_DAM)
                self.nhat_ky.ghi(f"  ĐÃ GIỮ {', '.join(ghe)} ({rtt:.1f} ms)")
                self._kiem_cho(self.tt)
            else:
                self.chon -= set(tin.get("ghe_ban", ghe))
                self._bao(f"Không giữ được: {tin.get('ly_do')}", "#ffd0d0", DO_DAM)
                self.nhat_ky.ghi(f"  không giữ được: {tin.get('ly_do')}")
        elif loai == "thanh_toan":
            if tin.get("ok"):
                self.so_ve += len(ghe)
                self.ve += [(g, tu, den, v) for g, v in zip(ghe, tin["ma_ve"])]
                self.phien_ve = tin["phien"]
                self.giu, self.doan_giu, self.han = set(), None, None
                self._bao(f"Đặt vé thành công! {tien(tin['tien'])}", "#c8f0d2", XANH_DAM)
                self.nhat_ky.ghi(f"  ĐẶT VÉ THÀNH CÔNG: {', '.join(tin['ma_ve'])} ({rtt:.1f} ms)")
            else:
                self._bao(f"Thanh toán bị từ chối: {tin.get('ly_do')}", "#ff6060", "white")
                self.nhat_ky.ghi(f"  thanh toán bị từ chối: {tin.get('ly_do')}")
        else:
            if tin.get("ok"):
                self.giu -= set(tin["ghe"])
                if not self.giu:
                    self.doan_giu, self.han = None, None
                self._bao(f"Đã nhả {', '.join(tin['ghe'])}", "#e8e8e8")
                self.nhat_ky.ghi(f"  đã nhả {', '.join(tin['ghe'])}")
            else:
                self.nhat_ky.ghi(f"  nhả bị từ chối: {tin.get('ly_do')}")
        self._ve_lai_ve()
        self._cap_nhat_gio()

    def _ve_lai_ve(self):
        dong = [f"{g} {MA_TRAM[tu]}→{MA_TRAM[den]}" for g, tu, den, _ in self.ve]
        self.nhan_ve.configure(text="Vé của bạn: " + (", ".join(dong) or "–")
                               + f"\nBạn: giữ được {self.so_giu} lần, mua {self.so_ve} vé")

    def _kiem_cho(self, tt):
        """Chỗ mình giữ/mua bị người khác lấy (tranh chấp) hoặc hết hạn; chỗ đang chọn bị người khác giữ.
        Chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq giữ chỗ / thanh toán)."""
        if not tt:
            return
        if tt["phien"] >= self.phien_giu and self.giu:
            tu, den = self.doan_giu
            for g in sorted(self.giu):
                chu = [tt["chu"].get(f"{g}#{k}") for k in range(tu, den)]
                if all(a == self.ten for a in chu):
                    continue
                self.giu.discard(g)
                ai = next((a for a in chu if a not in (None, self.ten)), None)
                if ai:
                    self._bao(f"Chỗ {g} đã bị {ai} lấy mất!", "#ff6060", "white")
                    self.nhat_ky.ghi(f"Chỗ {g} đã bị {ai} lấy mất!")
                else:
                    self._bao("Hết thời gian giữ chỗ – chỗ đã được nhả", "#ffd0d0", DO_DAM)
                    self.nhat_ky.ghi(f"Hết thời gian giữ chỗ: {g}")
            if not self.giu:
                self.han, self.doan_giu = None, None
        if tt["phien"] >= self.phien_ve:
            con = []
            for g, tu, den, v in self.ve:
                chu = [tt["chu"].get(f"{g}#{k}") for k in range(tu, den)]
                ai = next((a for a in chu if a != self.ten), self.ten)
                if ai == self.ten:
                    con.append((g, tu, den, v))
                else:
                    self._bao(f"Vé {g} bị ghi cho {ai or 'không ai'}!", "#ff6060", "white")
                    self.nhat_ky.ghi(f"Vé {g} bị ghi cho {ai or 'không ai'}!")
            self.ve = con
        bi_lay = [g for g in self.chon if trang_cho(tt, g, self.ten, self.tu, self.den) != "trong"]
        for g in bi_lay:
            self.chon.discard(g)
        if bi_lay:
            self._bao(f"Chỗ {', '.join(sorted(bi_lay))} vừa có người giữ", "#ffe0b2")
            self.nhat_ky.ghi(f"Chỗ {', '.join(sorted(bi_lay))} vừa có người giữ")

    # ------------------------------------------------------------ nhận trạng thái
    def _cap_nhat(self, tt):
        khoa = (tt["nha_xe"], tt["chuyen"], tt["ten_xe"], tt["ttl_s"])
        if getattr(self, "_bang_ron_cu", None) != khoa:
            self._ve_bang_ron(tt)
            self._bang_ron_cu = khoa
            self._ve_tuyen()
        if self.thong_bao["text"] == "Chưa kết nối":
            self._bao("Chọn đoạn đường, chọn chỗ rồi bấm GIỮ CHỖ", "#e8e8e8")
        if not tt["chu"] and (self.ve or self.giu) and not tt["dem"]["ban"] and not tt["dem"]["giu"]:
            self.ve.clear()                                  # máy chủ đặt lại: xe trống
        self.chon = {g for g in self.chon if g in tt["ghe"]}  # đổi loại xe: bỏ chỗ không còn
        if self.giu and self.doan_giu:
            tu, den = self.doan_giu
            mine = [tt["con_lai_ms"][f"{g}#{k}"] for g in self.giu for k in range(tu, den)
                    if tt["chu"].get(f"{g}#{k}") == self.ten and f"{g}#{k}" in tt["con_lai_ms"]]
            if mine:
                self.han = time.monotonic() + min(mine) / 1000
        self._kiem_cho(tt)
        self._ve_lai_ve()
        self._cap_nhat_gio()

    def _moi_nhip(self):
        if self.han is None:
            self.nhan_dong_ho.configure(text="--:--", fg="#909090")
            return
        con = self.han - time.monotonic()
        self.nhan_dong_ho.configure(text=dong_ho(con), fg=DO_DAM if con < 10 else XANH_DAM)
        if con <= 0 and self.nut_tt["state"] == "normal":
            self._bat_nut()
            self._bao("Hết thời gian giữ chỗ", "#ffd0d0", DO_DAM)

    def _dong_dinh_ky(self):
        d = self.tt["dem"]
        con = "–" if self.han is None else f"{max(0.0, self.han - time.monotonic()):.0f} s"
        return (f"chỗ-chặng trống {d['trong']}, giữ {d['giu']}, bán {d['ban']}; bạn giữ"
                f" {', '.join(sorted(self.giu)) or '–'} (còn {con}), vé {len(self.ve)}")


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "An"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41782
    root = tk.Tk()
    cs = CuaSoDatVeXe(root, ten, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
