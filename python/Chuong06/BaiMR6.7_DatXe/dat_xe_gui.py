"""Bài mở rộng MR6.7 – Ứng dụng gọi xe (người dùng, Tkinter).

Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở giao_dien_chung.CuaSoNguoiDungCoSo;
tệp này vẽ ứng dụng gọi xe: bản đồ phố (Canvas: đường, công viên, hồ, tên phố), điểm đón (xanh) và
điểm đến (đỏ) – bấm chuột trái/phải lên bản đồ để chọn ngã tư –, tuyến đường, các xe (màu theo xe, xe
đã có khách mờ đi), giá cước dự kiến, nút ĐẶT XE, trạng thái "Đang tìm tài xế…" / "Đặt thành công –
tài xế …" / "Hết chỗ" / "Tài xế … đã nhận khách khác!", thẻ tài xế và nút Hủy chuyến. Đặt được thì xe
chạy hoạt hình tới điểm đón rồi tới điểm đến. Mọi số liệu về xe đến từ cap_nhat của máy chủ.

Chạy:  python3 dat_xe_gui.py <tên> [host] [cổng]       (mặc định 127.0.0.1 41712)
"""
import sys
import time
import tkinter as tk

from chung import tien
from dat_xe import diem_mac_dinh
from giao_dien_chung import DO_DAM, NEN, XANH_DAM, CuaSoNguoiDungCoSo, NhatKy, nhan, nut
from may_chu import DUONG, NGA_TU, TEN_DOC, TEN_NGANG, cuoc, km, ten_diem

MAU_XE = {"trắng": "#f4f4f4", "đỏ": "#e53935", "xanh": "#1e88e5", "bạc": "#b0bec5", "vàng": "#fdd835",
          "đen": "#37474f", "cam": "#fb8c00", "tím": "#8e24aa", "xám": "#78909c", "hồng": "#f06292"}
XANH_GRAB, XANH_TUYEN, DO_PIN = "#00a651", "#1a73e8", "#d93025"
TOC_DO = 0.28                                       # đơn vị bản đồ / giây (hoạt hình)


def duong_di(a, b):
    """Đường đi trên lưới phố từ a tới b: đi ngang trước rồi đi dọc."""
    ds = [tuple(a)]
    if a[0] != b[0] and a[1] != b[1]:
        ds.append((b[0], a[1]))
    ds.append(tuple(b))
    return ds


def do_dai(ds):
    return sum(abs(p[0] - q[0]) + abs(p[1] - q[1]) for p, q in zip(ds, ds[1:]))


def diem_tren(ds, s):
    """Điểm cách đầu đường đi một đoạn s (đơn vị bản đồ) và hướng ("ngang"/"doc")."""
    for p, q in zip(ds, ds[1:]):
        d = abs(p[0] - q[0]) + abs(p[1] - q[1])
        if s <= d or q == ds[-1]:
            t = 0 if d == 0 else min(1.0, s / d)
            return (p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t), ("ngang" if p[1] == q[1] else "doc")
        s -= d
    return ds[-1], "ngang"


def doan(ds, s0, s1):
    """Phần đường đi từ khoảng cách s0 tới s1 (danh sách điểm)."""
    kq, tich = [diem_tren(ds, s0)[0]], 0.0
    for p, q in zip(ds, ds[1:]):
        tich += abs(p[0] - q[0]) + abs(p[1] - q[1])
        if s0 < tich < s1:
            kq.append(q)
    kq.append(diem_tren(ds, s1)[0])
    return kq


class BanDo:
    """Bản đồ phố trên một Canvas (dùng cả ở ứng dụng gọi xe và bảng điều khiển máy chủ)."""

    def __init__(self, cv, rong, cao, nho=False):
        self.cv, self.w, self.h, self.nho = cv, rong, cao, nho
        self.ve_nen()

    def p(self, x, y):
        return x * self.w, y * self.h

    def _o(self, x0, y0, x1, y1, le=0.022):
        """Hình chữ nhật của khu phố giữa các đường (thụt vào khỏi lòng đường)."""
        a, b = self.p(x0 + le, y0 + le)
        c, d = self.p(x1 - le, y1 - le)
        return a, b, c, d

    def ve_nen(self):
        cv = self.cv
        cv.create_rectangle(0, 0, self.w, self.h, fill="#ebe7df", width=0)
        bien = (0.0,) + DUONG + (1.0,)
        for i, (x0, x1) in enumerate(zip(bien, bien[1:])):          # khu phố: nhà (ô nhỏ)
            for j, (y0, y1) in enumerate(zip(bien, bien[1:])):
                a, b, c, d = self._o(x0, y0, x1, y1)
                if c - a < 6 or d - b < 6:
                    continue
                cv.create_rectangle(a, b, c, d, fill="#e2ddd2", width=0)
                if not self.nho and (i + j) % 2 == 0:
                    cv.create_rectangle(a + 4, b + 4, (a + c) / 2 - 2, (b + d) / 2 - 2, fill="#d6d0c2", width=0)
        a, b, c, d = self._o(0.3, 0.7, 0.5, 0.9)                     # công viên
        cv.create_rectangle(a, b, c, d, fill="#bfe3a6", width=0)
        for k in range(5):
            x, y = a + (c - a) * (0.2 + 0.15 * k), b + (d - b) * (0.35 if k % 2 else 0.65)
            r = 3 if self.nho else 6
            cv.create_oval(x - r, y - r, x + r, y + r, fill="#7cbf5a", width=0)
        if not self.nho:
            cv.create_text((a + c) / 2, d - 7, text="Công viên", fill="#3d7a2a", font=("TkDefaultFont", 7))
        a, b, c, d = self._o(0.7, 0.1, 0.9, 0.3)                     # hồ
        cv.create_oval(a, b, c, d, fill="#a9d3f2", outline="#8cc2ea")
        if not self.nho:
            cv.create_text((a + c) / 2, (b + d) / 2, text="Hồ", fill="#2a6fa8", font=("TkDefaultFont", 7))
        rong = 7 if self.nho else 12
        for x in DUONG:                                              # đường dọc và ngang
            chinh = x == 0.5
            cv.create_line(*self.p(x, 0), *self.p(x, 1), fill="#fde293" if chinh else "#ffffff",
                           width=rong + (2 if chinh else 0))
            cv.create_line(*self.p(0, x), *self.p(1, x), fill="#fde293" if chinh else "#ffffff",
                           width=rong + (2 if chinh else 0))
        if not self.nho:
            for i, (x, ten) in enumerate(zip(DUONG, TEN_DOC)):          # tên phố: giữa hai ngã tư
                cv.create_text(x * self.w + 10, (0.4 if i % 2 else 0.6) * self.h, text=ten, anchor="center",
                               angle=-90, fill="#7a7468", font=("TkDefaultFont", 7))
            for i, (y, ten) in enumerate(zip(DUONG, TEN_NGANG)):
                cv.create_text((0.4 if i % 2 else 0.6) * self.w, y * self.h - 10, text=ten, anchor="center",
                               fill="#7a7468", font=("TkDefaultFont", 7))

    def xoa(self, the="dong"):
        self.cv.delete(the)

    def tuyen(self, ds, mau=XANH_TUYEN, net=None, the="dong"):
        if len(ds) < 2:
            return
        diem = [c for x, y in ds for c in self.p(x, y)]
        rong = 3 if self.nho else 5
        if net is None:
            self.cv.create_line(*diem, fill="#0b4fb3", width=rong + 2, capstyle="round", joinstyle="round", tags=the)
        self.cv.create_line(*diem, fill=mau, width=rong, dash=net, capstyle="round", joinstyle="round", tags=the)

    def ghim(self, x, y, mau, chu, the="dong"):
        px, py = self.p(x, y)
        r = 5 if self.nho else 9
        self.cv.create_polygon(px - r * 0.8, py - r * 1.4, px + r * 0.8, py - r * 1.4, px, py, fill=mau,
                               outline=mau, tags=the)
        self.cv.create_oval(px - r, py - r * 2.6, px + r, py - r * 0.6, fill=mau, outline="white", width=2, tags=the)
        self.cv.create_oval(px - r * 0.35, py - r * 1.95, px + r * 0.35, py - r * 1.25, fill="white", width=0,
                            tags=the)
        if chu and not self.nho:
            self.cv.create_text(px + r + 3, py - r * 1.6, text=chu, anchor="w", fill=mau,
                                font=("TkDefaultFont", 8, "bold"), tags=the)

    def xe(self, x, y, mau, nhan_xe="", huong="ngang", mo=False, noi_bat=False, the="dong"):
        px, py = self.p(x, y)
        dai, ngan = (14, 8) if self.nho else (24, 13)
        rx, ry = (dai / 2, ngan / 2) if huong == "ngang" else (ngan / 2, dai / 2)
        cv = self.cv
        if noi_bat:
            r = dai * 0.85
            cv.create_oval(px - r, py - r, px + r, py + r, fill="", outline=XANH_GRAB, width=3, tags=the)
        than = "#d0d0d0" if mo else MAU_XE.get(mau, "#f4f4f4")
        cv.create_rectangle(px - rx + 1, py - ry + 2, px + rx + 1, py + ry + 2, fill="#9a9488", width=0, tags=the)
        cv.create_rectangle(px - rx, py - ry, px + rx, py + ry, fill=than, outline="#555555" if mo else "#202020",
                            tags=the)
        if huong == "ngang":                                          # kính lái, kính sau
            cv.create_rectangle(px + rx * 0.25, py - ry + 2, px + rx * 0.55, py + ry - 2, fill="#5c6f80",
                                width=0, tags=the)
            cv.create_rectangle(px - rx * 0.7, py - ry + 2, px - rx * 0.45, py + ry - 2, fill="#5c6f80",
                                width=0, tags=the)
        else:
            cv.create_rectangle(px - rx + 2, py + ry * 0.25, px + rx - 2, py + ry * 0.55, fill="#5c6f80",
                                width=0, tags=the)
            cv.create_rectangle(px - rx + 2, py - ry * 0.7, px + rx - 2, py - ry * 0.45, fill="#5c6f80",
                                width=0, tags=the)
        if nhan_xe:
            cv.create_text(px, py - max(rx, ry) - 6, text=nhan_xe, fill="#202020" if not mo else "#707070",
                           font=("TkDefaultFont", 7 if self.nho else 8, "bold"), tags=the)


class CuaSoGoiXe(CuaSoNguoiDungCoSo):
    TIEU_DE = "Gọi xe"
    RONG, CAO = 440, 340

    def _ve(self, khung):
        self.don, self.den = diem_mac_dinh(self.ten)
        self.trang = "san_sang"          # san_sang | dang_tim | da_dat | het_cho | mat_xe | xong
        self.xe_toi = None               # mã xe được báo đặt thành công
        self.chuyen = None               # thông tin chuyến từ kq
        self.phien_toi = 0               # số thứ tự lần ghi (kq) của chuyến mình đặt
        self.hoat_hinh = None            # (đường đi, thời điểm bắt đầu, độ dài, mốc tới điểm đón)
        self.so_dat = self.so_duoc = 0
        trai = tk.Frame(khung, bg=NEN)
        trai.pack(side="left", pady=4)
        self.cv = tk.Canvas(trai, width=self.RONG, height=self.CAO, highlightthickness=1,
                            highlightbackground="#808080")
        self.cv.pack()
        self.ban_do = BanDo(self.cv, self.RONG, self.CAO)
        self.cv.bind("<Button-1>", lambda e: self._chon(e, "don"))
        self.cv.bind("<Button-3>", lambda e: self._chon(e, "den"))
        nhan(trai, "Chuột trái: chọn điểm đón · chuột phải: chọn điểm đến", font=("TkDefaultFont", 8),
             fg="#404040").pack(anchor="w")

        phai = tk.Frame(khung, bg="white", bd=1, relief="solid")
        phai.pack(side="left", fill="both", expand=True, padx=(8, 0), pady=4)
        tk.Label(phai, text="Bạn muốn đi đâu?", font=("TkDefaultFont", 12, "bold"), bg="white").pack(
            anchor="w", padx=8, pady=(6, 2))
        self.nhan_don = tk.Label(phai, text="", bg="white", fg="#202020", anchor="w", font=("TkDefaultFont", 9))
        self.nhan_don.pack(fill="x", padx=8)
        self.nhan_den = tk.Label(phai, text="", bg="white", fg="#202020", anchor="w", font=("TkDefaultFont", 9))
        self.nhan_den.pack(fill="x", padx=8)
        self.nhan_cuoc = tk.Label(phai, text="", bg="white", anchor="w", font=("TkDefaultFont", 10, "bold"))
        self.nhan_cuoc.pack(fill="x", padx=8, pady=(4, 2))
        self.nut_dat = tk.Button(phai, text="ĐẶT XE", bg=XANH_GRAB, fg="white", activebackground="#008a43",
                                 activeforeground="white", font=("TkDefaultFont", 12, "bold"), relief="flat",
                                 command=self.dat_xe, disabledforeground="#c8e8d4")
        self.nut_dat.pack(fill="x", padx=8, pady=4)
        self.nhan_tt = tk.Label(phai, text="Chưa kết nối", font=("TkDefaultFont", 10, "bold"), bg="#e8e8e8",
                                pady=5, wraplength=240)
        self.nhan_tt.pack(fill="x", padx=8, pady=2)
        g = tk.Frame(phai, bg="#f5f7f6", bd=1, relief="groove")      # thẻ tài xế
        g.pack(fill="x", padx=8, pady=4)
        self.cv_tx = tk.Canvas(g, width=44, height=44, bg="#f5f7f6", highlightthickness=0)
        self.cv_tx.pack(side="left", padx=4, pady=4)
        f = tk.Frame(g, bg="#f5f7f6")
        f.pack(side="left", fill="x")
        self.nhan_tx = tk.Label(f, text="Tài xế: –", bg="#f5f7f6", anchor="w", font=("TkDefaultFont", 10, "bold"))
        self.nhan_tx.pack(anchor="w")
        self.nhan_tx2 = tk.Label(f, text="", bg="#f5f7f6", anchor="w", font=("TkDefaultFont", 8), justify="left")
        self.nhan_tx2.pack(anchor="w")
        self.nut_huy = nut(phai, "Hủy chuyến", "#f0f0f0", self.huy, 14)
        self.nut_huy.pack(anchor="w", padx=8, pady=(0, 4))
        self.nhan_xe_trong = tk.Label(phai, text="", bg="white", anchor="w", font=("TkDefaultFont", 9))
        self.nhan_xe_trong.pack(fill="x", padx=8)
        self.nhan_cua_toi = tk.Label(phai, text="", bg="white", anchor="w", font=("TkDefaultFont", 8), fg="#505050")
        self.nhan_cua_toi.pack(fill="x", padx=8, pady=(0, 6))
        self._ve_diem()
        self._bat_nut()
        self._ve_dong()

    # ------------------------------------------------------------ chọn điểm
    def _chon(self, e, loai):
        if self.trang in ("dang_tim", "da_dat"):
            return
        x, y = e.x / self.RONG, e.y / self.CAO
        p = min(NGA_TU, key=lambda q: (q[0] - x) ** 2 + (q[1] - y) ** 2)
        if loai == "don" and p != self.den:
            self.don = p
        elif loai == "den" and p != self.don:
            self.den = p
        self._ve_diem()
        self._ve_dong()

    def _ve_diem(self):
        self.nhan_don.configure(text=f"● Đón:  {ten_diem(self.don)}", fg=XANH_GRAB)
        self.nhan_den.configure(text=f"● Đến:  {ten_diem(self.den)}", fg=DO_PIN)
        self.nhan_cuoc.configure(text=f"{km(self.don, self.den):.1f} km · cước dự kiến {tien(cuoc(self.don, self.den))}")

    def _bat_nut(self):
        co_the_dat = self.kn is not None and self.trang in ("san_sang", "het_cho", "mat_xe", "xong")
        self.nut_dat.configure(state="normal" if co_the_dat else "disabled",
                               bg=XANH_GRAB if co_the_dat else "#7fcfa5")
        self.nut_huy.configure(state="normal" if self.trang == "da_dat" else "disabled",
                               text="Hoàn tất chuyến" if self.hoat_hinh and self._da_toi() else "Hủy chuyến")

    def _bao(self, text, nen, chu="black"):
        self.nhan_tt.configure(text=text, bg=nen, fg=chu)

    # ------------------------------------------------------------ đặt / hủy
    def dat_xe(self):
        if self.kn is None or self.trang in ("dang_tim", "da_dat"):
            return
        self.trang = "dang_tim"
        self.so_dat += 1
        self.t_tim = time.monotonic()
        self._bao("Đang tìm tài xế…", "#fff3c4")
        self.nhat_ky.ghi(f"Đặt xe: {ten_diem(self.don)} -> {ten_diem(self.den)}")
        self._bat_nut()
        self.gui_yc("dat_xe", don=list(self.don), den=list(self.den))

    def huy(self):
        if self.trang == "da_dat":
            self.trang = "dang_huy"
            self._bat_nut()
            self.gui_yc("huy")

    def _ket_qua(self, loai, truong, tin, rtt):
        if loai == "dat_xe":
            if tin.get("ok"):
                self.so_duoc += 1
                self.trang, self.xe_toi, self.chuyen, self.phien_toi = "da_dat", tin["xe"], tin, tin["phien"]
                self._bao(f"Đặt thành công – tài xế {tin['tai_xe']}", "#c8f0d2", XANH_DAM)
                self.nhat_ky.ghi(f"  ĐẶT THÀNH CÔNG: {tin['tai_xe']} {tin['bien_so']}, đến sau {tin['phut']} phút"
                                 f" ({rtt:.1f} ms)")
                self._the_tai_xe(tin)
                xe = self._xe(tin["xe"])
                goc = xe["vi_tri"] if xe else self.don
                ds = duong_di(goc, self.don)
                moc = do_dai(ds)
                ds = ds + duong_di(self.don, self.den)[1:]
                self.hoat_hinh = (ds, time.monotonic(), do_dai(ds), moc)
                self._kiem_xe(self.tt)
            elif tin.get("ma_loi") == "het_cho":
                self.trang = "het_cho"
                self._bao("Hết chỗ – không còn xe trống", "#ffd0d0", DO_DAM)
                self.nhat_ky.ghi(f"  HẾT CHỖ ({rtt:.1f} ms)")
            else:
                self.trang = "het_cho"
                self._bao(f"Không đặt được: {tin.get('ly_do')}", "#ffd0d0", DO_DAM)
                self.nhat_ky.ghi(f"  từ chối: {tin.get('ly_do')}")
        else:
            if tin.get("ok"):
                xong = self.hoat_hinh is not None and self._da_toi()
                self.nhat_ky.ghi("Hoàn tất chuyến, cảm ơn bạn!" if xong else f"Đã hủy chuyến ({tin['tai_xe']})")
                self._bao("Hoàn tất chuyến – cảm ơn bạn!" if xong else "Đã hủy chuyến", "#e8e8e8")
                self.trang = "xong"
            else:
                self.nhat_ky.ghi(f"  hủy bị từ chối: {tin.get('ly_do')}")
                self.trang = "san_sang"
            self.xe_toi = self.chuyen = self.hoat_hinh = None
            self._the_tai_xe(None)
        self.nhan_cua_toi.configure(text=f"Bạn: bấm đặt {self.so_dat} lần, được báo thành công {self.so_duoc}")
        self._bat_nut()
        self._ve_dong()

    def _the_tai_xe(self, tin):
        cv = self.cv_tx
        cv.delete("all")
        if not tin:
            self.nhan_tx.configure(text="Tài xế: –")
            self.nhan_tx2.configure(text="")
            return
        cv.create_oval(4, 4, 40, 40, fill=XANH_GRAB, outline="")
        cv.create_text(22, 22, text=tin["tai_xe"][0], fill="white", font=("TkDefaultFont", 16, "bold"))
        self.nhan_tx.configure(text=f"{tin['tai_xe']}  ★ 4.9")
        self.nhan_tx2.configure(text=f"{tin['bien_so']} · xe {tin['mau']}\nđến sau {tin['phut']} phút ·"
                                     f" {tien(tin['cuoc'])}")

    def _xe(self, ma):
        if not self.tt:
            return None
        return next((x for x in self.tt["xe"] if x["ma"] == ma), None)

    def _kiem_xe(self, tt):
        """Chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq). Xe mình được báo đặt
        thành công mà lại ghi khách khác: bị "cướp" chỗ (tranh chấp); xe trống: máy chủ đặt lại."""
        if self.trang != "da_dat" or self.xe_toi is None or not tt or tt["phien"] < self.phien_toi:
            return
        xe = next((x for x in tt["xe"] if x["ma"] == self.xe_toi), None)
        if xe is not None and xe["khach"] == self.ten:
            return
        if xe is not None and xe["khach"] is not None:
            self.trang = "mat_xe"
            self.hoat_hinh = None
            self._bao(f"Tài xế {xe['tai_xe']} đã nhận khách khác ({xe['khach']})!", "#ff6060", "white")
            self.nhat_ky.ghi(f"Tài xế {xe['tai_xe']} đã nhận khách khác: {xe['khach']}")
        else:
            self.trang = "san_sang"
            self.xe_toi = self.chuyen = self.hoat_hinh = None
            self._the_tai_xe(None)
            self._bao("Chuyến đã kết thúc (máy chủ đặt lại)", "#e8e8e8")
        self._bat_nut()

    # ------------------------------------------------------------ nhận trạng thái
    def _cap_nhat(self, tt):
        self.nhan_xe_trong.configure(text=f"Xe trống quanh bạn: {tt['trong']}/{tt['so_xe']}")
        if self.trang == "dang_tim":
            pass
        elif self.nhan_tt["text"] == "Chưa kết nối":
            self._bao("Chọn điểm đón và bấm ĐẶT XE", "#e8e8e8")
        self._kiem_xe(tt)
        self._bat_nut()
        self._ve_dong()

    def _da_toi(self):
        ds, t0, dai, moc = self.hoat_hinh
        return (time.monotonic() - t0) * TOC_DO >= dai

    def _ve_dong(self):
        bd = self.ban_do
        bd.xoa()
        if self.hoat_hinh:
            ds, t0, dai, moc = self.hoat_hinh
            s = (time.monotonic() - t0) * TOC_DO
            if s < moc:                                      # xe đang tới điểm đón: tuyến nét đứt
                bd.tuyen(doan(ds, s, moc), mau="#5f8fd8", net=(4, 3))
                bd.tuyen(duong_di(self.don, self.den))
            else:
                bd.tuyen(doan(ds, s, dai))
        else:
            bd.tuyen(duong_di(self.don, self.den), mau="#8fb4ec")
        bd.ghim(*self.don, XANH_GRAB, "Đón")
        bd.ghim(*self.den, DO_PIN, "Đến")
        for xe in (self.tt or {}).get("xe", []):
            if xe["ma"] == self.xe_toi and self.hoat_hinh:
                continue
            bd.xe(*xe["vi_tri"], xe["mau"], xe["tai_xe"], mo=xe["khach"] is not None)
        if self.hoat_hinh and self.chuyen:
            ds, t0, dai, moc = self.hoat_hinh
            (x, y), huong = diem_tren(ds, min(dai, (time.monotonic() - t0) * TOC_DO))
            bd.xe(x, y, self.chuyen["mau"], self.chuyen["tai_xe"], huong, noi_bat=True)

    def _moi_nhip(self):
        if self.trang == "dang_tim":
            cham = "." * (1 + int((time.monotonic() - self.t_tim) * 5) % 3)
            self._bao(f"Đang tìm tài xế{cham}", "#fff3c4")

    def _nhip_hoat_hinh(self):
        if self.hoat_hinh:
            ds, t0, dai, moc = self.hoat_hinh
            s = (time.monotonic() - t0) * TOC_DO
            cu = getattr(self, "_pha", None)
            pha = "toi_don" if s < moc else ("di" if s < dai else "den")
            if pha != cu:
                self._pha = pha
                if pha == "di":
                    self._bao(f"Tài xế {self.chuyen['tai_xe']} đã đón bạn – đang đi", "#c8f0d2", XANH_DAM)
                    self.nhat_ky.ghi("Tài xế đã tới điểm đón")
                elif pha == "den":
                    self._bao("Đã tới nơi – bấm Hoàn tất chuyến", "#c8f0d2", XANH_DAM)
                    self.nhat_ky.ghi(f"Đã tới {ten_diem(self.den)}")
                    self._bat_nut()
            self._ve_dong()
        else:
            self._pha = None
        if self.ui.song:
            self.root.after(40, self._nhip_hoat_hinh)

    def _da_ket_noi(self, kn):
        super()._da_ket_noi(kn)
        self._bat_nut()
        if not getattr(self, "_dang_chay_hh", False):
            self._dang_chay_hh = True
            self.root.after(40, self._nhip_hoat_hinh)

    def _mat_ket_noi(self, kn, ly_do="Mất kết nối máy chủ"):
        super()._mat_ket_noi(kn, ly_do)
        if self.kn is None:
            self.trang = "san_sang"
            self.xe_toi = self.chuyen = self.hoat_hinh = None
            self._bat_nut()

    def _dong_dinh_ky(self):
        tt = self.tt
        xe = self._xe(self.xe_toi) if self.xe_toi else None
        return (f"{tt['trong']}/{tt['so_xe']} xe trống; trạng thái {self.trang}"
                + (f", xe {xe['ma']} {xe['tai_xe']}" if xe else "")
                + f"; bạn đặt {self.so_dat}, được {self.so_duoc}")


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "An"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41712
    root = tk.Tk()
    cs = CuaSoGoiXe(root, ten, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
