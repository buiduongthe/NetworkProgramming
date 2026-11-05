"""Bài mở rộng MR6.12 – Ứng dụng ngân hàng số (người dùng, Tkinter).

Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở giao_dien_chung.CuaSoNguoiDungCoSo;
tệp này vẽ ứng dụng ngân hàng: thẻ tài khoản (vẽ bằng Canvas: số tài khoản, chủ tài khoản, SỐ DƯ cỡ lớn),
chọn "tài khoản của tôi", form chuyển tiền (tài khoản đích, số tiền + nút chọn nhanh, nội dung), dòng
thông báo ("Chuyển thành công…", "Không đủ số dư", "Giao dịch bị hủy: khóa chết…", "+ Nhận … từ …") và
lịch sử giao dịch của tài khoản (tiền ra đỏ, tiền vào xanh). Số dư và tiền đến lấy từ cap_nhat của
máy chủ (≤ 10 lần/giây).

Chạy:  python3 ngan_hang_gui.py <tên> [host] [cổng]       (mặc định 127.0.0.1 41762)
"""
import sys
import time
import tkinter as tk
from tkinter import ttk

from chung import tien
from giao_dien_chung import DO_DAM, NEN, XANH_DAM, CuaSoNguoiDungCoSo, NhatKy, khung_nhom, nhan, nut

TEN_NGAN_HANG = "NGÂN HÀNG SỐ LTM"
NHANH = [(100_000, "100K"), (500_000, "500K"), (1_000_000, "1 triệu"), (2_000_000, "2 triệu")]


def _tron(c1, c2, t):
    a = [int(c1[i:i + 2], 16) for i in (1, 3, 5)]
    b = [int(c2[i:i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{round(x + (y - x) * t):02x}" for x, y in zip(a, b))


def ve_the(cv, ma, chu, so_du, tren="#0b3d91", duoi="#1e88e5"):
    """Vẽ thẻ tài khoản (Canvas): nền chuyển màu, góc bo, chip, số tài khoản, chủ tài khoản, số dư."""
    w, h, r = int(cv["width"]), int(cv["height"]), 16
    cv.delete("all")
    cv.create_rectangle(0, 0, w, h, fill=NEN, width=0)
    for y in range(h):                                   # nền chuyển màu từ trên xuống
        cv.create_line(r if y < r or y > h - r else 0, y, w - r if y < r or y > h - r else w, y,
                       fill=_tron(tren, duoi, y / h))
    for x0, y0, bat_dau, y_mau in ((0, 0, 90, r / 2), (w - 2 * r, 0, 0, r / 2), (0, h - 2 * r, 180, h - r / 2),
                                   (w - 2 * r, h - 2 * r, 270, h - r / 2)):
        cv.create_arc(x0, y0, x0 + 2 * r, y0 + 2 * r, start=bat_dau, extent=90, fill=_tron(tren, duoi, y_mau / h),
                      outline="")
    for y in range(r, h - r):                            # dải bên trái/phải giữa hai góc
        mau = _tron(tren, duoi, y / h)
        cv.create_line(0, y, r, y, fill=mau)
        cv.create_line(w - r, y, w, y, fill=mau)
    cv.create_oval(w - 150, -60, w + 60, 150, outline="#5c9ce6", width=2)       # họa tiết
    cv.create_oval(w - 110, -30, w + 90, 170, outline="#4a8ad8", width=1)
    cv.create_text(18, 20, text=TEN_NGAN_HANG, anchor="w", fill="white", font=("TkDefaultFont", 11, "bold"))
    cv.create_text(w - 18, 20, text="THANH TOÁN", anchor="e", fill="#cfe3ff", font=("TkDefaultFont", 8, "bold"))
    cv.create_rectangle(20, 44, 62, 74, fill="#e8c766", outline="#a8862e")      # chip
    for y in (54, 64):
        cv.create_line(20, y, 62, y, fill="#a8862e")
    cv.create_line(41, 44, 41, 74, fill="#a8862e")
    for i, rr in enumerate((8, 14, 20)):                 # biểu tượng không tiếp xúc
        cv.create_arc(70 - rr, 59 - rr, 70 + rr, 59 + rr, start=-40, extent=80, style="arc", outline="white",
                      width=2 if i == 0 else 1)
    cv.create_text(110, 50, text="Số tài khoản", anchor="w", fill="#cfe3ff", font=("TkDefaultFont", 8))
    cv.create_text(110, 68, text=f"{ma}  ••••  6125", anchor="w", fill="white", font=("TkFixedFont", 12, "bold"))
    cv.create_text(20, 98, text="Số dư khả dụng", anchor="w", fill="#cfe3ff", font=("TkDefaultFont", 9))
    cv.create_text(20, 126, text=tien(so_du), anchor="w", fill="white", font=("TkDefaultFont", 22, "bold"))
    cv.create_text(20, h - 22, text=f"CHỦ TÀI KHOẢN: {chu.upper()}", anchor="w", fill="white",
                   font=("TkDefaultFont", 9, "bold"))
    cv.create_text(w - 18, h - 22, text="VND", anchor="e", fill="#cfe3ff", font=("TkDefaultFont", 9, "bold"))


class CuaSoNganHang(CuaSoNguoiDungCoSo):
    TIEU_DE = "Ngân hàng số"

    def _ve(self, khung):
        self.tk = None                                   # mã tài khoản của tôi
        self.da_thay = set()                             # ma_gd đã đưa vào lịch sử
        self.so_chuyen = self.so_thanh_cong = self.so_nhan = self.so_kq = 0
        trai = tk.Frame(khung, bg=NEN)
        trai.pack(side="left", fill="y", pady=4)
        self.cv = tk.Canvas(trai, width=340, height=190, highlightthickness=0)
        self.cv.pack()
        ve_the(self.cv, "TK??", "–", 0)
        f = tk.Frame(trai, bg=NEN)
        f.pack(anchor="w", pady=(6, 0))
        nhan(f, "Tài khoản của tôi:").pack(side="left")
        self.chon_tk = ttk.Combobox(f, width=16, state="readonly")
        self.chon_tk.pack(side="left", padx=4)
        self.chon_tk.bind("<<ComboboxSelected>>", lambda e: self._doi_tk())
        g = khung_nhom(trai, "Lịch sử giao dịch (mới nhất trên cùng)")
        g.pack(fill="both", expand=True, pady=(6, 0))
        self.ls = tk.Listbox(g, height=9, width=48, font=("TkFixedFont", 9), activestyle="none")
        self.ls.pack(fill="both", expand=True)

        phai = tk.Frame(khung, bg=NEN)
        phai.pack(side="left", fill="both", expand=True, padx=(8, 0), pady=4)
        g = khung_nhom(phai, "Chuyển tiền")
        g.pack(fill="x")
        nhan(g, "Đến tài khoản:").grid(row=0, column=0, sticky="w")
        self.chon_den = ttk.Combobox(g, width=18, state="readonly")
        self.chon_den.grid(row=0, column=1, columnspan=2, sticky="w", pady=2)
        nhan(g, "Số tiền (đ):").grid(row=1, column=0, sticky="w")
        self.o_tien = tk.Entry(g, width=14, font=("TkDefaultFont", 11, "bold"), justify="right")
        self.o_tien.insert(0, "500.000")
        self.o_tien.grid(row=1, column=1, sticky="w", pady=2)
        f = tk.Frame(g, bg=NEN)
        f.grid(row=2, column=0, columnspan=3, sticky="w")
        for x, chu in NHANH:
            nut(f, chu, "#e8eef8", lambda x=x: self._dat_tien(x), 6).pack(side="left", padx=(0, 3))
        nhan(g, "Nội dung:").grid(row=3, column=0, sticky="w")
        self.o_noi_dung = tk.Entry(g, width=22)
        self.o_noi_dung.insert(0, "Chuyển tiền")
        self.o_noi_dung.grid(row=3, column=1, columnspan=2, sticky="w", pady=2)
        self.nut_chuyen = nut(g, "Chuyển tiền", "#ff8c00", self.bam_chuyen, 16)
        self.nut_chuyen.grid(row=4, column=0, columnspan=3, pady=(6, 2))
        self.thong_bao = tk.Label(phai, text="Chưa kết nối", font=("TkDefaultFont", 10, "bold"), bg="#e0e0e0",
                                  width=36, height=3, wraplength=330, justify="left", anchor="w", padx=6)
        self.thong_bao.pack(fill="x", pady=(6, 2))
        self.nhan_den = tk.Label(phai, text="", font=("TkDefaultFont", 9, "bold"), bg=NEN, fg="#0050a0",
                                 width=34, wraplength=290, justify="left", anchor="w", padx=6)
        self.nhan_den.pack(fill="x", pady=(0, 4))
        self.nhan_dem = nhan(phai, "Đã chuyển 0 lần · thành công 0 · nhận 0", font=("TkDefaultFont", 8))
        self.nhan_dem.pack(anchor="w")
        self.nhan_he_thong = nhan(phai, "", font=("TkDefaultFont", 8), fg="#404040", wraplength=300,
                                  justify="left")
        self.nhan_he_thong.pack(anchor="w", pady=(4, 0))
        self.nut_chuyen.configure(state="disabled")

    # ------------------------------------------------------------ tiện ích
    def _chu(self, ma):
        return next((t["chu"] for t in self.tt["tai_khoan"] if t["ma"] == ma), "?") if self.tt else "?"

    def _so_du(self):
        return next((t["so_du"] for t in self.tt["tai_khoan"] if t["ma"] == self.tk), 0)

    def _dat_tien(self, x):
        self.o_tien.delete(0, "end")
        self.o_tien.insert(0, tien(x)[:-2])

    def _bao(self, text, nen, chu="black"):
        self.thong_bao.configure(text=text, bg=nen, fg=chu)

    def _them_ls(self, g, ra, ghi_chu=""):
        """Thêm một giao dịch vào đầu lịch sử: tiền ra (đỏ, dấu −) hoặc tiền vào (xanh, dấu +)."""
        doi = g["den"] if ra else g["tu"]
        dong = (f"{g['gio']} {'−' if ra else '+'}{tien(g['so_tien']):>13} {'tới' if ra else 'từ '} {doi}"
                f" {self._chu(doi)}" + (f" · {g['noi_dung']}" if g.get("noi_dung") else "") + ghi_chu)
        self.ls.insert(0, dong)
        self.ls.itemconfigure(0, fg=DO_DAM if ra else XANH_DAM)
        if self.ls.size() > 100:
            self.ls.delete(100, "end")

    # ------------------------------------------------------------ chọn tài khoản
    def _doi_tk(self):
        ma = self.chon_tk.get().split(" ")[0]
        if ma == self.tk:
            return
        self.tk = ma
        self.ls.delete(0, "end")
        self.da_thay = {g["ma"] for g in self.tt["lich_su"]}
        self._dien_den()
        self._ve_lai_the()
        self.nhat_ky.ghi(f"Đăng nhập {ma} – {self._chu(ma)}, số dư {tien(self._so_du())}")

    def _dien_den(self):
        ds = [f"{t['ma']} – {t['chu']}" for t in self.tt["tai_khoan"] if t["ma"] != self.tk]
        self.chon_den.configure(values=ds)
        if self.chon_den.get() not in ds and ds:
            self.chon_den.set(ds[0])

    def chon_nguoi_nhan(self, ma):
        """Chọn tài khoản đích (kịch bản demo) – như người dùng chọn trong danh sách."""
        self.chon_den.set(f"{ma} – {self._chu(ma)}")

    def _ve_lai_the(self):
        the = (self.tk, self._chu(self.tk), self._so_du())
        if the != getattr(self, "_the_cu", None):         # chỉ vẽ lại khi số dư/tài khoản đổi
            self._the_cu = the
            ve_the(self.cv, *the)

    # ------------------------------------------------------------ chuyển tiền
    def bam_chuyen(self):
        den = self.chon_den.get().split(" ")[0]
        try:
            x = int(self.o_tien.get().replace(".", "").replace(" ", "").replace("đ", ""))
        except ValueError:
            self._bao("Số tiền không hợp lệ", "#ffb347")
            return
        self.so_chuyen += 1
        self.nhat_ky.ghi(f"Chuyển {tien(x)} {self.tk}→{den}…")
        self._bao(f"Đang xử lý: {tien(x)} tới {den}…", "#fff3c4")
        self.gui_yc("chuyen", tu=self.tk, den=den, so_tien=x, noi_dung=self.o_noi_dung.get().strip()[:40])

    def _ket_qua(self, loai, truong, tin, rtt):
        x, den = truong["so_tien"], truong["den"]
        self.so_kq += 1
        if tin.get("ok"):
            self.so_thanh_cong += 1
            self.da_thay.add(tin["ma_gd"])
            if truong["tu"] == self.tk:
                self._them_ls({"gio": time.strftime("%H:%M:%S"), **truong}, True)
            self.nhat_ky.ghi(f"  thành công ({rtt:.1f} ms), mã GD {tin['ma_gd']}")
            self._bao(f"Chuyển thành công {tien(x)} tới {den} – {self._chu(den)}\n"
                      f"Số dư mới: {tien(tin['so_du'])}", "#70d070")
        else:
            ly_do = tin.get("ly_do", "")
            self.nhat_ky.ghi(f"  thất bại ({rtt:.0f} ms): {ly_do}")
            ma = tin.get("ma_loi")
            if ma == "khoa_cho_lau":
                self._bao(ly_do[0].upper() + ly_do[1:], "#ff6060", "white")
            elif ma == "khong_du":
                self._bao(f"Không đủ số dư để chuyển {tien(x)}", "#ffb347")
            else:
                self._bao(f"Giao dịch thất bại: {ly_do}", "#ffb347")
        self.nhan_dem.configure(text=f"Đã chuyển {self.so_chuyen} lần · thành công {self.so_thanh_cong}"
                                     f" · nhận {self.so_nhan}")

    # ------------------------------------------------------------ nhận trạng thái
    def _cap_nhat(self, tt):
        ds = [f"{t['ma']} – {t['chu']}" for t in tt["tai_khoan"]]
        if list(self.chon_tk["values"]) != ds:
            self.chon_tk.configure(values=ds)
        if tt["so_gd"] < getattr(self, "_so_gd_cu", 0):   # máy chủ vừa đặt lại: số dư về ban đầu
            self.ls.delete(0, "end")
            self.nhan_den.configure(text="", bg=NEN)
            self._bao("Máy chủ đặt lại: số dư về ban đầu", "#e0e0e0")
            self.nhat_ky.ghi("Máy chủ đặt lại: số dư về ban đầu")
        self._so_gd_cu = tt["so_gd"]
        if self.tk is None or self.tk not in {t["ma"] for t in tt["tai_khoan"]}:
            self.tk = next((t["ma"] for t in tt["tai_khoan"] if t["chu"] == self.ten), tt["tai_khoan"][0]["ma"])
            self.da_thay = {g["ma"] for g in tt["lich_su"]}
            self.ls.delete(0, "end")
            self.nhat_ky.ghi(f"Đăng nhập {self.tk} – {self._chu(self.tk)}, số dư {tien(self._so_du())}")
            self._bao("Xin chào! Chọn người nhận và số tiền để chuyển.", "#e0e0e0")
        self.chon_tk.set(f"{self.tk} – {self._chu(self.tk)}")
        self._dien_den()
        self._ve_lai_the()
        self.nut_chuyen.configure(state="normal")
        for g in reversed(tt["lich_su"]):                # giao dịch mới liên quan tới tài khoản của tôi
            if g["ma"] in self.da_thay:
                continue
            self.da_thay.add(g["ma"])
            if g["den"] == self.tk:
                self.so_nhan += 1
                self._them_ls(g, False)
                self.nhat_ky.ghi(f"+ Nhận {tien(g['so_tien'])} từ {g['tu']} ({self._chu(g['tu'])})")
                self.nhan_den.configure(text=f"+ Nhận {tien(g['so_tien'])} từ {g['tu']} – {self._chu(g['tu'])}"
                                             f" lúc {g['gio']}", bg="#bfe8ff")
            elif g["tu"] == self.tk:                     # tiền ra do phiên khác của cùng tài khoản
                self._them_ls(g, True, f" ({g['nguoi']})")
        lech = tt["tong"] - tt["tong_dau"]
        self.nhan_he_thong.configure(
            text=f"Máy chủ: {len(tt['tai_khoan'])} tài khoản, {tt['so_gd']} giao dịch"
                 + (" – khóa ngây thơ" if tt["khoa_ngay_tho"] else "")
                 + (f"\nTổng tiền hệ thống LỆCH {tien(lech)}!" if lech else ""),
            fg=DO_DAM if lech else "#404040")
        self.nhan_dem.configure(text=f"Đã chuyển {self.so_chuyen} lần · thành công {self.so_thanh_cong}"
                                     f" · nhận {self.so_nhan}")

    def _dong_dinh_ky(self):
        return (f"{self.tk} số dư {tien(self._so_du())}; chuyển {self.so_chuyen},"
                f" thành công {self.so_thanh_cong}, nhận {self.so_nhan}")


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "An"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41762
    root = tk.Tk()
    cs = CuaSoNganHang(root, ten, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
