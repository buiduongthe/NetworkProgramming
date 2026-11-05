"""Bài mở rộng MR6.8 – Bảng điều khiển máy chủ đặt vé xem phim (Tkinter).

Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ,
nhật ký) ở giao_dien_chung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": sơ đồ phòng chiếu thu
nhỏ (trống / đang giữ / đã bán, ghế nóng viền đỏ), số ghế mỗi loại và tham số đặt lại (thời hạn giữ chỗ,
ghế nóng của bộ mô phỏng, giá vé).

Chạy:  python3 server_gui.py [host] [cổng]        (mặc định 127.0.0.1 41722; giữ chỗ 30 s)
"""
import sys
import tkinter as tk

import mo_phong
from dat_ve_gui import DA_BAN, KHAC_GIU, SoDoGhe
from giao_dien_chung import NEN, CuaSoServerCoSo, NhatKy, nhan, o_nhap
from may_chu import COT, HANG, MayChuDatVe


class CuaSoServerDatVe(CuaSoServerCoSo):
    TIEU_DE = "Máy chủ đặt vé xem phim MR6.8"
    TEN_TAI_NGUYEN = "Tài nguyên: phòng chiếu 8 × 12 ghế"

    def _ve_tai_nguyen(self, g):
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w")
        self.cv = tk.Canvas(f, width=206, height=124, highlightthickness=0, bg="#1c1c24")
        self.cv.pack(side="left")
        self.so_do = SoDoGhe(self.cv, HANG, COT, o=13, khe=2, le=6, tren=16, chu=False)
        t = tk.Frame(f, bg=NEN)
        t.pack(side="left", fill="y", padx=(8, 0))
        self.nhan_dem = {}
        for ma, chu, mau in (("trong", "Trống", "white"), ("giu", "Đang giữ", KHAC_GIU), ("ban", "Đã bán", DA_BAN)):
            h = tk.Frame(t, bg=NEN)
            h.pack(anchor="w")
            tk.Label(h, width=2, bg=mau, relief="solid", bd=1).pack(side="left")
            self.nhan_dem[ma] = nhan(h, f" {chu}: –", font=("TkDefaultFont", 10, "bold"))
            self.nhan_dem[ma].pack(side="left")
        self.nhan_nong = nhan(t, "", font=("TkDefaultFont", 8))
        self.nhan_nong.pack(anchor="w", pady=(4, 0))
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(4, 0))
        ts = self.lop.tham_so
        self.o_ts = {}
        for i, (ma, chu, rong) in enumerate([("ttl_s", "Giữ chỗ (s)", 6), ("ghe_nong", "Ghế nóng", 5),
                                             ("gia_ve", "Giá vé", 8)]):
            nhan(f, chu + ":").grid(row=0, column=i * 2, sticky="w", padx=(0 if i == 0 else 6, 2))
            self.o_ts[ma] = o_nhap(f, f"{ts[ma]:g}" if isinstance(ts[ma], float) else ts[ma], rong)
            self.o_ts[ma].grid(row=0, column=i * 2 + 1, sticky="w")
        self._ve_cu = None

    def _tham_so_dat_lai(self):
        try:
            return {"ttl_s": float(self.o_ts["ttl_s"].get()), "ghe_nong": self.o_ts["ghe_nong"].get().strip().upper(),
                    "gia_ve": int(self.o_ts["gia_ve"].get())}
        except ValueError:
            raise ValueError("thời hạn và giá vé phải là số")

    def dat_tham_so(self, **ts):
        """Điền ô tham số (kịch bản demo) – như người dùng gõ."""
        for k, v in ts.items():
            self.o_ts[k].delete(0, "end")
            self.o_ts[k].insert(0, str(v))

    def _cap_nhat_tai_nguyen(self, tt, tk_):
        for ma, chu in (("trong", "Trống"), ("giu", "Đang giữ"), ("ban", "Đã bán")):
            self.nhan_dem[ma].configure(text=f" {chu}: {tt['dem'][ma]}")
        nong = tt["ghe_nong"]
        self.nhan_nong.configure(text=f"Ghế nóng (mô phỏng): {nong or 'ngẫu nhiên'}\n"
                                      f"{nong}: {tt['chu'].get(nong) or 'trống'}" if nong else
                                 "Ghế nóng (mô phỏng): ngẫu nhiên")
        if tt["so_do"] == self._ve_cu:
            return
        self._ve_cu = tt["so_do"]
        mau = {}
        for r, dong in zip(tt["hang"], tt["so_do"]):
            for c, x in enumerate(dong):
                if x != ".":
                    mau[f"{r}{c + 1}"] = KHAC_GIU if x == "g" else DA_BAN
        self.so_do.ve(mau, vien={nong: "#ff3030"} if nong else None)


def tao_cua_so(root, host, cong):
    return CuaSoServerDatVe(root, MayChuDatVe, host, cong, mo_phong, che_do="khoa", tre_ms=2.0,
                            tham_so={}, n=500, k=200, lan=mo_phong.SO_LAN_MAC_DINH)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41722
    root = tk.Tk()
    cs = tao_cua_so(root, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
