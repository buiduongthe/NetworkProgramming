"""Bài mở rộng MR6.8 – Ứng dụng đặt vé xem phim (người dùng, Tkinter).

Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở giao_dien_chung.CuaSoNguoiDungCoSo;
tệp này vẽ ứng dụng đặt vé: áp phích phim (Canvas), sơ đồ phòng chiếu 8 × 12 ghế với MÀN HÌNH, lối đi,
nhãn hàng A–H; màu ghế: trống / đang chọn / người khác giữ / đã bán / bạn giữ / vé của bạn. Bấm ghế để
chọn, "GIỮ CHỖ" gửi giu_cho, đồng hồ giữ chỗ đếm ngược (đỏ khi < 10 s), "THANH TOÁN" và "Nhả ghế".
Sơ đồ cập nhật trực tiếp từ cap_nhat (≤ 10 lần/giây) khi người khác giữ/mua ghế; ghế đang chọn bị
người khác giữ thì tự bỏ chọn và báo.

Chạy:  python3 dat_ve_gui.py <tên> [host] [cổng]       (mặc định 127.0.0.1 41722)
"""
import sys
import time
import tkinter as tk

from chung import tien
from giao_dien_chung import DO_DAM, NEN, XANH_DAM, CuaSoNguoiDungCoSo, NhatKy

TRONG, CHON, KHAC_GIU, DA_BAN, TOI_GIU, VE_TOI = "#ffffff", "#b9f6ca", "#ffa726", "#9e9e9e", "#2e7d32", "#1565c0"
DO_RAP = "#e71a0f"
CHU_GIAI = [(TRONG, "Trống"), (CHON, "Đang chọn"), (KHAC_GIU, "Người khác giữ"), (DA_BAN, "Đã bán"),
            (TOI_GIU, "Bạn giữ"), (VE_TOI, "Vé của bạn")]


def dong_ho(giay):
    giay = max(0, int(giay + 0.999))
    return f"{giay // 60:02d}:{giay % 60:02d}"


class SoDoGhe:
    """Vẽ sơ đồ ghế lên Canvas; dùng ở ứng dụng đặt vé (ô = 30 px) và bảng điều khiển máy chủ (ô nhỏ)."""

    def __init__(self, cv, hang, cot, o=30, khe=4, le=24, tren=46, loi_di=(2, 10), chu=True):
        self.cv, self.hang, self.cot = cv, hang, cot
        self.o, self.khe, self.le, self.tren, self.loi_di, self.chu = o, khe, le, tren, loi_di, chu
        self.rong = le * 2 + cot * (o + khe) + len(loi_di) * o * 0.5
        self.cao = tren + len(hang) * (o * 0.86 + khe) + 4
        self.vi_tri = {}
        for i, r in enumerate(hang):
            y = tren + i * (o * 0.86 + khe)
            for c in range(1, cot + 1):
                x = le + (c - 1) * (o + khe) + sum(o * 0.5 for a in loi_di if c > a)
                self.vi_tri[f"{r}{c}"] = (x, y, x + o, y + o * 0.86)
        self.ve_nen()

    def ve_nen(self):
        cv, w = self.cv, self.rong
        cv.create_rectangle(0, 0, w + 40, self.cao + 200, fill="#1c1c24", width=0)
        lech = self.tren * 0.55
        cv.create_line(self.le, lech, w * 0.5, lech * 0.35, w - self.le, lech, smooth=True,
                       fill="#e8e8f0", width=4 if self.chu else 2)
        if self.chu:
            cv.create_text(w / 2, lech + 10, text="MÀN HÌNH", fill="#b0b0c0", font=("TkDefaultFont", 8, "bold"))
        for i, r in enumerate(self.hang):
            if self.chu:
                y = (self.vi_tri[f"{r}1"][1] + self.vi_tri[f"{r}1"][3]) / 2
                cv.create_text(self.le / 2, y, text=r, fill="#c0c0d0", font=("TkDefaultFont", 9, "bold"))
                cv.create_text(w - self.le / 2, y, text=r, fill="#c0c0d0", font=("TkDefaultFont", 9, "bold"))

    def ve(self, mau_ghe, vien=None):
        """mau_ghe: {ghế: màu}; vien: {ghế: màu viền} (ghế nóng ở máy chủ)."""
        cv = self.cv
        cv.delete("ghe")
        for g, (x0, y0, x1, y1) in self.vi_tri.items():
            mau = mau_ghe.get(g, TRONG)
            v = (vien or {}).get(g, "#606070")
            cv.create_rectangle(x0, y0, x1, y1, fill=mau, outline=v, width=2 if g in (vien or {}) else 1,
                                tags=("ghe", f"g_{g}"))
            if self.chu:                                         # tay vịn: vạch tối ở chân ghế
                cv.create_rectangle(x0 + 2, y1 - 4, x1 - 2, y1 - 1, fill="#707080",
                                    width=0, tags="ghe")
                toi = mau in (TOI_GIU, VE_TOI, DA_BAN)
                cv.create_text((x0 + x1) / 2, (y0 + y1) / 2 - 2, text=g[1:], fill="white" if toi else "#303040",
                               font=("TkDefaultFont", 7), tags="ghe")
                if mau == DA_BAN:
                    cv.create_line(x0 + 4, y0 + 3, x1 - 4, y1 - 6, fill="#e0e0e0", tags="ghe")

    def ghe_tai(self, x, y):
        for g, (x0, y0, x1, y1) in self.vi_tri.items():
            if x0 <= x <= x1 and y0 <= y <= y1:
                return g
        return None


class CuaSoDatVe(CuaSoNguoiDungCoSo):
    TIEU_DE = "Đặt vé xem phim"

    def _ve(self, khung):
        self.chon = set()                # ghế đang chọn (chưa giữ)
        self.giu = set()                 # ghế mình đang giữ (theo kq)
        self.phien_giu = self.phien_ve = 0   # số thứ tự lần ghi (kq) của lần giữ chỗ / thanh toán gần nhất
        self.ve = set()                  # vé của mình
        self.han = None                  # thời điểm (monotonic) hết hạn giữ chỗ
        self.so_giu = self.so_ve = 0
        trai = tk.Frame(khung, bg=NEN)
        trai.pack(side="left", pady=4)
        self.cv = tk.Canvas(trai, width=486, height=322, highlightthickness=0, bg="#1c1c24")
        self.cv.pack()
        self.so_do = SoDoGhe(self.cv, "ABCDEFGH", 12)
        y = self.so_do.cao + 14
        x = 10
        for mau, chu in CHU_GIAI:                              # chú giải màu
            self.cv.create_rectangle(x, y, x + 14, y + 12, fill=mau, outline="#606070")
            self.cv.create_text(x + 18, y + 6, text=chu, anchor="w", fill="#d0d0e0", font=("TkDefaultFont", 8))
            x += 22 + len(chu) * 6
        self.cv.bind("<Button-1>", self._bam_ghe)

        phai = tk.Frame(khung, bg="white", bd=1, relief="solid")
        phai.pack(side="left", fill="both", expand=True, padx=(8, 0), pady=4)
        self.cv_phim = tk.Canvas(phai, width=250, height=96, highlightthickness=0, bg="#2b1640")
        self.cv_phim.pack(padx=6, pady=(6, 2))
        self.nhan_ghe = tk.Label(phai, text="Ghế đã chọn: –", bg="white", anchor="w", font=("TkDefaultFont", 10, "bold"))
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
        self.nut_tt = tk.Button(phai, text="THANH TOÁN", bg=DO_RAP, fg="white", activebackground="#b71c1c",
                                activeforeground="white", font=("TkDefaultFont", 11, "bold"), relief="flat",
                                command=self.thanh_toan, disabledforeground="#ffcdd2")
        self.nut_tt.pack(fill="x", padx=8, pady=2)
        self.nut_nha = tk.Button(phai, text="Nhả ghế", bg="#f0f0f0", relief="raised", bd=1, command=self.nha)
        self.nut_nha.pack(anchor="w", padx=8, pady=2)
        self.thong_bao = tk.Label(phai, text="Chưa kết nối", font=("TkDefaultFont", 9, "bold"), bg="#e8e8e8",
                                  pady=4, wraplength=240)
        self.thong_bao.pack(fill="x", padx=8, pady=2)
        self.nhan_ve = tk.Label(phai, text="Vé của bạn: –", bg="white", anchor="w", justify="left",
                                font=("TkDefaultFont", 9), fg=VE_TOI, wraplength=240)
        self.nhan_ve.pack(fill="x", padx=8, pady=(0, 6))
        self._ve_ghe()
        self._bat_nut()

    # ------------------------------------------------------------ áp phích, sơ đồ
    def _ve_phim(self, tt):
        cv = self.cv_phim
        cv.delete("all")
        for i in range(12):                                    # nền chuyển màu tím -> xanh
            r, g, b = 43 + i * 2, 22 + i * 6, 64 + i * 9
            cv.create_rectangle(0, i * 8, 250, i * 8 + 8, fill=f"#{r:02x}{g:02x}{b:02x}", width=0)
        cv.create_oval(196, 8, 242, 54, fill="#4fc3f7", outline="#b3e5fc", width=2)
        cv.create_arc(196, 8, 242, 54, start=200, extent=140, fill="#43a047", outline="")
        for k in range(14):
            x, y = (k * 37) % 170 + 6, (k * 23) % 60 + 6
            cv.create_oval(x, y, x + 2, y + 2, fill="white", width=0)
        cv.create_text(10, 40, text=tt["phim"].upper(), anchor="w", fill="white", font=("TkDefaultFont", 12, "bold"))
        cv.create_text(10, 66, text=f"2D Phụ đề · Suất {tt['suat']} · Phòng 3", anchor="w", fill="#e0d8f0",
                       font=("TkDefaultFont", 8))
        cv.create_text(10, 82, text=f"{tien(tt['gia_ve'])}/ghế · giữ chỗ {tt['ttl_s']:g} s", anchor="w",
                       fill="#ffd54f", font=("TkDefaultFont", 8, "bold"))

    def _ve_ghe(self):
        tt, mau = self.tt, {}
        if tt:
            for r, dong in zip(tt["hang"], tt["so_do"]):
                for c, x in enumerate(dong):
                    g = f"{r}{c + 1}"
                    cua_toi = tt["chu"].get(g) == self.ten
                    if x == "b":
                        mau[g] = VE_TOI if cua_toi else DA_BAN
                    elif x == "g":
                        mau[g] = TOI_GIU if cua_toi else KHAC_GIU
        for g in self.chon:
            mau.setdefault(g, CHON)
        self.so_do.ve(mau)

    def _trang_ghe(self, g):
        """'trong' | 'giu' (người khác) | 'ban' | 'toi_giu' | 've' theo cap_nhat mới nhất."""
        if not self.tt:
            return "trong"
        r, c = g[0], int(g[1:])
        x = self.tt["so_do"][self.tt["hang"].index(r)][c - 1]
        cua_toi = self.tt["chu"].get(g) == self.ten
        return {".": "trong", "g": "toi_giu" if cua_toi else "giu", "b": "ve" if cua_toi else "ban"}[x]

    def _bam_ghe(self, e):
        g = self.so_do.ghe_tai(e.x, e.y)
        if g:
            self.bam(g)

    def bam(self, g):
        """Bấm một ghế (người dùng hoặc kịch bản demo)."""
        if not self.kn:
            return
        trang = self._trang_ghe(g)
        if g in self.chon:
            self.chon.discard(g)
        elif trang == "trong" and g not in self.giu:
            if len(self.chon) + len(self.giu) >= 8:
                self._bao("Tối đa 8 ghế mỗi lần", "#ffe0b2")
                return
            self.chon.add(g)
        elif trang == "giu":
            self._bao(f"Ghế {g} đang có người giữ", "#ffe0b2")
        elif trang == "ban":
            self._bao(f"Ghế {g} đã bán", "#e0e0e0")
        self._cap_nhat_gio()

    def _cap_nhat_gio(self):
        gia = self.tt["gia_ve"] if self.tt else 0
        if self.giu:
            self.nhan_ghe.configure(text=f"Đang giữ: {', '.join(sorted(self.giu))}")
            self.nhan_tien.configure(text=f"Tổng: {tien(gia * len(self.giu))}")
            self.nut_tt.configure(text=f"THANH TOÁN {tien(gia * len(self.giu))}")
        else:
            self.nhan_ghe.configure(text=f"Ghế đã chọn: {', '.join(sorted(self.chon)) or '–'}")
            self.nhan_tien.configure(text=f"Tạm tính: {tien(gia * len(self.chon))}")
            self.nut_tt.configure(text="THANH TOÁN")
        self._bat_nut()
        self._ve_ghe()

    def _bat_nut(self):
        co = self.kn is not None
        con_han = self.han is not None and time.monotonic() < self.han
        self.nut_giu.configure(state="normal" if co and self.chon and not self.giu else "disabled",
                               bg="#ff8f00" if co and self.chon and not self.giu else "#ffcc80")
        self.nut_tt.configure(state="normal" if co and self.giu and con_han else "disabled",
                              bg=DO_RAP if co and self.giu and con_han else "#ef9a9a")
        self.nut_nha.configure(state="normal" if co and self.giu else "disabled")

    def _bao(self, text, nen, chu="black"):
        self.thong_bao.configure(text=text, bg=nen, fg=chu)

    # ------------------------------------------------------------ giữ / thanh toán / nhả
    def giu_cho(self):
        if not self.chon or self.giu:
            return
        ghe = sorted(self.chon)
        self.nhat_ky.ghi(f"Giữ chỗ {', '.join(ghe)}…")
        self._bao("Đang giữ chỗ…", "#fff3c4")
        self.nut_giu.configure(state="disabled")
        self.gui_yc("giu_cho", ghe=ghe)

    def thanh_toan(self):
        if self.giu:
            self.nhat_ky.ghi(f"Thanh toán {', '.join(sorted(self.giu))}…")
            self.gui_yc("thanh_toan", ghe=sorted(self.giu))

    def nha(self):
        if self.giu:
            self.gui_yc("nha", ghe=sorted(self.giu))

    def _ket_qua(self, loai, truong, tin, rtt):
        ghe = truong["ghe"]
        if loai == "giu_cho":
            if tin.get("ok"):
                self.so_giu += 1
                self.giu, self.phien_giu = set(tin["ghe"]), tin.get("phien", 0)
                self.chon -= self.giu
                self.han = time.monotonic() + tin["het_han_ms"] / 1000
                self._bao(f"Đã giữ {', '.join(ghe)} – thanh toán trong {tin['het_han_ms'] / 1000:.0f} s",
                          "#c8f0d2", XANH_DAM)
                self.nhat_ky.ghi(f"  ĐÃ GIỮ {', '.join(ghe)} ({rtt:.1f} ms)")
                self._kiem_ghe(self.tt)
            else:
                self.chon -= set(tin.get("ghe_ban", ghe))
                self._bao(f"Không giữ được: {tin.get('ly_do')}", "#ffd0d0", DO_DAM)
                self.nhat_ky.ghi(f"  không giữ được: {tin.get('ly_do')}")
        elif loai == "thanh_toan":
            if tin.get("ok"):
                self.so_ve += len(ghe)
                self.ve |= set(ghe)
                self.phien_ve = tin["phien"]
                self.giu -= set(ghe)
                self.han = None
                self._bao(f"Đặt vé thành công! {tien(tin['tien'])}", "#c8f0d2", XANH_DAM)
                self.nhat_ky.ghi(f"  ĐẶT VÉ THÀNH CÔNG: {', '.join(tin['ma_ve'])} ({rtt:.1f} ms)")
            else:
                self._bao(f"Thanh toán bị từ chối: {tin.get('ly_do')}", "#ff6060", "white")
                self.nhat_ky.ghi(f"  thanh toán bị từ chối: {tin.get('ly_do')}")
        else:
            if tin.get("ok"):
                self.giu -= set(tin["ghe"])
                self.han = None if not self.giu else self.han
                self._bao(f"Đã nhả {', '.join(tin['ghe'])}", "#e8e8e8")
                self.nhat_ky.ghi(f"  đã nhả {', '.join(tin['ghe'])}")
            else:
                self.nhat_ky.ghi(f"  nhả bị từ chối: {tin.get('ly_do')}")
        self._ve_lai_ve()
        self._cap_nhat_gio()

    def _ve_lai_ve(self):
        self.nhan_ve.configure(text="Vé của bạn: " + (", ".join(sorted(self.ve)) or "–")
                               + f"\nBạn: giữ được {self.so_giu} lần, mua {self.so_ve} vé")

    def _kiem_ghe(self, tt):
        """Ghế mình giữ/mua bị người khác lấy (tranh chấp) hoặc hết hạn; ghế đang chọn bị người khác giữ.
        Chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq giữ chỗ / thanh toán)."""
        if not tt:
            return
        if tt["phien"] >= self.phien_giu:
            for g in sorted(self.giu):
                ai = tt["chu"].get(g)
                if ai == self.ten:
                    continue
                self.giu.discard(g)
                if ai is not None:
                    self._bao(f"Ghế {g} đã bị {ai} lấy mất!", "#ff6060", "white")
                    self.nhat_ky.ghi(f"Ghế {g} đã bị {ai} lấy mất!")
                else:
                    self._bao("Hết thời gian giữ chỗ – ghế đã được nhả", "#ffd0d0", DO_DAM)
                    self.nhat_ky.ghi(f"Hết thời gian giữ chỗ: {g}")
            if not self.giu:
                self.han = None
        if tt["phien"] >= self.phien_ve:
            for g in sorted(self.ve):
                ai = tt["chu"].get(g)
                if ai != self.ten:
                    self.ve.discard(g)
                    self._bao(f"Vé {g} bị ghi cho {ai or 'không ai'}!", "#ff6060", "white")
                    self.nhat_ky.ghi(f"Vé {g} bị ghi cho {ai or 'không ai'}!")
        bi_lay = [g for g in self.chon if self._trang_ghe(g) != "trong"]
        for g in bi_lay:
            self.chon.discard(g)
        if bi_lay:
            self._bao(f"Ghế {', '.join(sorted(bi_lay))} vừa có người giữ", "#ffe0b2")
            self.nhat_ky.ghi(f"Ghế {', '.join(sorted(bi_lay))} vừa có người giữ")

    # ------------------------------------------------------------ nhận trạng thái
    def _cap_nhat(self, tt):
        if not getattr(self, "_da_ve_phim", False) or self._phim_cu != (tt["phim"], tt["ttl_s"], tt["gia_ve"]):
            self._ve_phim(tt)
            self._da_ve_phim, self._phim_cu = True, (tt["phim"], tt["ttl_s"], tt["gia_ve"])
        if self.thong_bao["text"] == "Chưa kết nối":
            self._bao("Chọn ghế rồi bấm GIỮ CHỖ", "#e8e8e8")
        if not any(tt["chu"].values()) and (self.ve or self.giu) and not tt["dem"]["ban"] and not tt["dem"]["giu"]:
            self.ve.clear()                                  # máy chủ đặt lại: rạp trống
        mine = [tt["con_lai_ms"][g] for g in self.giu if tt["chu"].get(g) == self.ten and g in tt["con_lai_ms"]]
        if mine:
            self.han = time.monotonic() + min(mine) / 1000
        self._kiem_ghe(tt)
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
        return (f"trống {d['trong']}, giữ {d['giu']}, bán {d['ban']}; bạn giữ"
                f" {', '.join(sorted(self.giu)) or '–'} (còn {con}), vé {', '.join(sorted(self.ve)) or '–'}")


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "An"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41722
    root = tk.Tk()
    cs = CuaSoDatVe(root, ten, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
