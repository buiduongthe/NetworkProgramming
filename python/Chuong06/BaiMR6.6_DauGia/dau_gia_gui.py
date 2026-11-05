"""Bài mở rộng MR6.6 – Phòng đấu giá (ứng dụng người dùng, Tkinter).

Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở giao_dien_chung.CuaSoNguoiDungCoSo;
tệp này vẽ phòng đấu giá: ảnh món hàng (vẽ bằng Canvas), giá hiện tại cỡ lớn, người dẫn đầu, đồng hồ
đếm ngược (đỏ khi còn < 10 s, báo GIA HẠN), các nút "+1/+2/+5 bước", ô nhập giá, lịch sử trả giá, thông
báo "Bạn bị trả giá cao hơn!" / "BẠN THẮNG!". Mọi số liệu đến từ cap_nhat của máy chủ (≤ 10 lần/giây);
đồng hồ đếm ngược chạy cục bộ từ con_lai_ms của lần cap_nhat mới nhất.

Chạy:  python3 dau_gia_gui.py <tên> [host] [cổng]       (mặc định 127.0.0.1 41702)
"""
import math
import sys
import time
import tkinter as tk

from chung import tien
from giao_dien_chung import (CAM, DO_DAM, NEN, XANH_DAM, XANH_DUONG, CuaSoNguoiDungCoSo, NhatKy, khung_nhom, nhan,
                             nut, o_nhap)

LAM = "#1f4fa0"                                         # màu men lam (gốm hoa lam)
# nửa bề rộng của bình theo chiều cao (đơn vị trước khi nhân tỉ lệ): (y, nửa rộng)
DANG_BINH = [(0, 24), (6, 22), (14, 15), (38, 16), (52, 34), (70, 50), (92, 55), (118, 47), (142, 32),
             (156, 22), (162, 25), (170, 25)]


def _nua_rong(y):
    for (y0, r0), (y1, r1) in zip(DANG_BINH, DANG_BINH[1:]):
        if y0 <= y <= y1:
            return r0 + (r1 - r0) * (y - y0) / (y1 - y0)
    return 0


def ve_binh_gom(cv, cx, tren, k=1.0, nen=True):
    """Vẽ "Bình gốm hoa lam" lên Canvas: tâm ngang cx, đỉnh miệng ở y = tren, tỉ lệ k."""
    w, h = int(cv["width"]), int(cv["height"])
    if nen:                                             # nền tối + quầng đèn chiếu
        cv.create_rectangle(0, 0, w, h, fill="#23232f", width=0)
        for i, mau in enumerate(("#2c2c3c", "#363648", "#404056", "#4a4a62")):
            r = (1.0 - i * 0.2) * w * 0.5
            cv.create_oval(cx - r, tren + 80 * k - r * 0.9, cx + r, tren + 80 * k + r * 0.9, fill=mau, width=0)
        day = tren + 170 * k
        cv.create_polygon(cx - 70 * k, day, cx + 70 * k, day, cx + 82 * k, day + 22 * k, cx - 82 * k,
                          day + 22 * k, fill="#6b4226", outline="#3e2412")
        cv.create_rectangle(cx - 82 * k, day + 22 * k, cx + 82 * k, h, fill="#4e2f1a", width=0)
        cv.create_oval(cx - 48 * k, day - 5 * k, cx + 48 * k, day + 6 * k, fill="#1a1a1a", width=0)
    trai = [(cx - r * k, tren + y * k) for y, r in DANG_BINH]
    phai = [(cx + r * k, tren + y * k) for y, r in reversed(DANG_BINH)]
    cv.create_polygon(*[c for p in trai + phai for c in p], smooth=True, fill="#f5f2e8", outline="#8c96a8")
    cv.create_oval(cx - 24 * k, tren - 4 * k, cx + 24 * k, tren + 5 * k, fill="#dcd8cc", outline="#8c96a8")

    def dai(y, day=2):                                  # đường viền men lam quanh thân ở độ cao y
        r = _nua_rong(y) * k - 1
        cv.create_line(cx - r, tren + y * k, cx + r, tren + y * k, fill=LAM, width=max(1, int(day * k)))
    for y in (10, 36, 54, 60, 138, 150):
        dai(y, 2 if y in (54, 138) else 1)
    for i in range(-3, 4):                              # cánh sen quanh cổ
        x = cx + i * 9 * k
        cv.create_arc(x - 5 * k, tren + 18 * k, x + 5 * k, tren + 38 * k, start=0, extent=180, style="arc",
                      outline=LAM, width=1)
    tam_y = tren + 96 * k                               # bông hoa giữa thân
    for i in range(8):
        g = i * math.pi / 4
        x, y = cx + 15 * k * math.cos(g), tam_y + 15 * k * math.sin(g)
        cv.create_oval(x - 7 * k, y - 7 * k, x + 7 * k, y + 7 * k, fill="#5b82c4", outline=LAM)
    cv.create_oval(cx - 8 * k, tam_y - 8 * k, cx + 8 * k, tam_y + 8 * k, fill=LAM, outline=LAM)
    for d in (-1, 1):                                   # dây lá cuốn hai bên
        cv.create_line(cx + d * 24 * k, tam_y, cx + d * 34 * k, tam_y - 16 * k, cx + d * 44 * k, tam_y - 4 * k,
                       cx + d * 40 * k, tam_y + 14 * k, cx + d * 30 * k, tam_y + 22 * k, smooth=True, fill=LAM,
                       width=2)
        cv.create_oval(cx + d * 40 * k - 4 * k, tam_y - 12 * k, cx + d * 40 * k + 4 * k, tam_y - 4 * k,
                       fill=LAM, outline=LAM)
    for i in range(-4, 5):                              # cánh sen chân bình
        x = cx + i * 8 * k
        cv.create_line(x - 4 * k, tren + 150 * k, x, tren + 140 * k, x + 4 * k, tren + 150 * k, fill=LAM)


def dong_ho(giay):
    """Đếm ngược dạng mm:ss (dưới 10 s thêm phần mười)."""
    if giay < 10:
        return f"00:{giay:04.1f}"
    giay = int(giay)
    return f"{giay // 60:02d}:{giay % 60:02d}"


class CuaSoDauGia(CuaSoNguoiDungCoSo):
    TIEU_DE = "Phòng đấu giá"

    def _ve(self, khung):
        self.het_han = None                             # thời điểm (monotonic) hết phiên theo cap_nhat
        self.so_lan_cu = 0
        self.dan_dau = False
        self.so_tra = self.so_nhan = 0
        self.han_bao_gia_han = 0.0
        trai = tk.Frame(khung, bg=NEN)
        trai.pack(side="left", fill="y", pady=4)
        self.cv = tk.Canvas(trai, width=230, height=240, highlightthickness=0)
        self.cv.pack()
        ve_binh_gom(self.cv, 115, 26)
        self.nhan_ten_mon = nhan(trai, "–", font=("TkDefaultFont", 11, "bold"))
        self.nhan_ten_mon.pack(anchor="w", pady=(4, 0))
        self.nhan_gia_kd = nhan(trai, "", font=("TkDefaultFont", 9))
        self.nhan_gia_kd.pack(anchor="w")
        self.nhan_cua_toi = nhan(trai, "Bạn: chưa trả giá", font=("TkDefaultFont", 9))
        self.nhan_cua_toi.pack(anchor="w")

        phai = tk.Frame(khung, bg=NEN)
        phai.pack(side="left", fill="both", expand=True, padx=(8, 0), pady=4)
        nhan(phai, "GIÁ HIỆN TẠI", font=("TkDefaultFont", 9, "bold"), fg="#404040").pack(anchor="w")
        self.nhan_gia = nhan(phai, "–", font=("TkDefaultFont", 24, "bold"), fg="#8b0000")
        self.nhan_gia.pack(anchor="w")
        self.nhan_dan_dau = nhan(phai, "Dẫn đầu: –", font=("TkDefaultFont", 10))
        self.nhan_dan_dau.pack(anchor="w")
        f = tk.Frame(phai, bg=NEN)
        f.pack(anchor="w", pady=(4, 0))
        nhan(f, "Còn lại ", font=("TkDefaultFont", 10)).pack(side="left")
        self.nhan_dong_ho = nhan(f, "–", font=("TkFixedFont", 20, "bold"))
        self.nhan_dong_ho.pack(side="left")
        self.nhan_gia_han = nhan(f, "", font=("TkDefaultFont", 10, "bold"), fg=DO_DAM)
        self.nhan_gia_han.pack(side="left", padx=6)
        self.thong_bao = tk.Label(phai, text="Chưa kết nối", font=("TkDefaultFont", 11, "bold"), bg="#e0e0e0",
                                  width=30, pady=4)
        self.thong_bao.pack(anchor="w", pady=4, fill="x")
        f = tk.Frame(phai, bg=NEN)
        f.pack(anchor="w")
        self.nut_buoc = []
        for n in (1, 2, 5):
            b = nut(f, f"+{n} bước", CAM if n == 1 else XANH_DUONG, lambda n=n: self.tang(n), 10)
            b.pack(side="left", padx=(0, 4))
            self.nut_buoc.append((n, b))
        f = tk.Frame(phai, bg=NEN)
        f.pack(anchor="w", pady=4)
        nhan(f, "Giá của bạn:").pack(side="left")
        self.o_gia = o_nhap(f, "", 12)
        self.o_gia.pack(side="left", padx=4)
        self.nut_tra = nut(f, "Trả giá", "#ff8c00", self.tra_o_nhap, 8)
        self.nut_tra.pack(side="left")
        g = khung_nhom(phai, "Lịch sử trả giá (mới nhất trên cùng)")
        g.pack(fill="both", expand=True)
        self.ls = tk.Listbox(g, height=8, width=40, font=("TkFixedFont", 9), activestyle="none")
        self.ls.pack(fill="both", expand=True)
        self._bat_nut(False)

    # ------------------------------------------------------------ trả giá
    def _bat_nut(self, bat):
        for _, b in self.nut_buoc:
            b.configure(state="normal" if bat else "disabled")
        self.nut_tra.configure(state="normal" if bat else "disabled")

    def tang(self, n):
        if self.tt:
            self.tra(self.tt["gia"] + n * self.tt["mon"]["buoc_gia"])

    def tra_o_nhap(self):
        try:
            gia = int(self.o_gia.get().replace(".", "").replace(" ", "").replace("đ", ""))
        except ValueError:
            self.nhat_ky.ghi("Giá phải là số nguyên (đồng)")
            return
        self.tra(gia)

    def tra(self, gia):
        self.so_tra += 1
        self.nhat_ky.ghi(f"Trả {tien(gia)}…")
        self.gui_yc("tra_gia", gia=gia)

    def _ket_qua(self, loai, truong, tin, rtt):
        if tin.get("ok"):
            self.so_nhan += 1
            self.nhat_ky.ghi(f"  chấp nhận {tien(truong['gia'])} ({rtt:.1f} ms)"
                             + (" – phiên được GIA HẠN" if tin.get("gia_han") else ""))
            self._bao("Bạn đang dẫn đầu", "#70d070", "black")
            self.dan_dau = True                          # cap_nhat sau đó báo nếu bị vượt (hoặc bị ghi đè)
        else:
            self.nhat_ky.ghi(f"  từ chối {tien(truong['gia'])}: {tin.get('ly_do')}")
            self._bao("Bị từ chối: " + ("giá đã thay đổi" if tin.get("ma_loi") == "gia_thap" else tin.get("ly_do", "")),
                      "#ffb347", "black")
        self.nhan_cua_toi.configure(text=f"Bạn: trả {self.so_tra} lần, được nhận {self.so_nhan}")

    def _bao(self, text, nen, chu):
        self.thong_bao.configure(text=text, bg=nen, fg=chu)

    # ------------------------------------------------------------ nhận trạng thái
    def _cap_nhat(self, tt):
        mon = tt["mon"]
        self.nhan_ten_mon.configure(text=mon["ten"])
        self.nhan_gia_kd.configure(text=f"Khởi điểm {tien(mon['gia_khoi_diem'])} · bước {tien(mon['buoc_gia'])}")
        self.nhan_gia.configure(text=tien(tt["gia"]))
        ai = tt["nguoi_dan_dau"]
        self.nhan_dan_dau.configure(text=f"Dẫn đầu: {ai or '–'}  ({tt['so_lan']} lượt trả)")
        for n, b in self.nut_buoc:
            b.configure(text=f"+{n} bước\n{tien(tt['gia'] + n * mon['buoc_gia'])}")
        moi = None if tt["con_lai_ms"] is None else time.monotonic() + tt["con_lai_ms"] / 1000
        if (moi is not None and self.het_han is not None and moi > self.het_han + 0.5 and not tt["ket_thuc"]
                and tt["so_lan"] > self.so_lan_cu):           # hạn lùi ra sau vì có giá mới (không phải đặt lại)
            self.han_bao_gia_han = time.monotonic() + 3.0
            self.nhat_ky.ghi(f"GIA HẠN: còn {tt['con_lai_ms'] / 1000:.1f} s")
        self.het_han, self.so_lan_cu = moi, tt["so_lan"]
        self.ls.delete(0, "end")
        for i, x in enumerate(tt["lich_su"]):
            self.ls.insert("end", f"{x['gio']} {x['nguoi']:<6} {tien(x['gia']):>14}" + (" gia hạn" if x["gia_han"] else ""))
            if x["nguoi"] == self.ten:
                self.ls.itemconfigure(i, fg="#0040c0")
        cua_toi = ai == self.ten
        if tt["ket_thuc"]:
            self._bat_nut(False)
            if cua_toi:
                self._bao(f"BẠN THẮNG! {tien(tt['gia'])}", "#ffd700", "black")
            else:
                self._bao(f"Kết thúc – {ai or 'không ai'} thắng", "#909090", "white")
        else:
            self._bat_nut(True)
            if self.dan_dau and not cua_toi:
                self._bao(f"Bạn bị trả giá cao hơn! ({ai})", "#ff6060", "white")
                self.nhat_ky.ghi(f"Bạn bị trả giá cao hơn: {ai} {tien(tt['gia'])}")
            elif cua_toi:
                self._bao("Bạn đang dẫn đầu", "#70d070", "black")
            elif tt["so_lan"] == 0 or self.thong_bao["text"] == "Chưa kết nối":
                self._bao("Phiên đang mở – mời trả giá", "#e0e0e0", "black")
        self.dan_dau = cua_toi and not tt["ket_thuc"]

    def _moi_nhip(self):
        if not self.tt:
            return
        if self.tt["ket_thuc"]:
            self.nhan_dong_ho.configure(text="00:00", fg="#606060")
        elif self.het_han is None:
            self.nhan_dong_ho.configure(text="∞", fg="black")
        else:
            con = max(0.0, self.het_han - time.monotonic())
            self.nhan_dong_ho.configure(text=dong_ho(con), fg=DO_DAM if con < 10 else XANH_DAM)
        self.nhan_gia_han.configure(text="GIA HẠN!" if time.monotonic() < self.han_bao_gia_han else "")

    def _dong_dinh_ky(self):
        tt = self.tt
        con = "∞" if self.het_han is None else f"{max(0.0, self.het_han - time.monotonic()):.1f} s"
        return (f"giá {tien(tt['gia'])}, dẫn đầu {tt['nguoi_dan_dau'] or '–'}, còn {con},"
                f" {'KẾT THÚC' if tt['ket_thuc'] else 'đang đấu'}; bạn trả {self.so_tra}, nhận {self.so_nhan}")


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "An"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41702
    root = tk.Tk()
    cs = CuaSoDauGia(root, ten, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
