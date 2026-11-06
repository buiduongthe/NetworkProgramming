"""Bài mở rộng MR6.14 – Bảng điều khiển máy chủ đặt vé xe khách theo chặng (Tkinter).

Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ,
nhật ký) ở giao_dien_chung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": sơ đồ xe thu nhỏ (mỗi chỗ
có dải vạch theo 3 chặng, ghế nóng viền đỏ), số chỗ-chặng mỗi loại, lấp đầy theo chặng và tham số đặt
lại (thời hạn giữ chỗ, loại xe giuong/ghe, ghế nóng của bộ mô phỏng).

Chạy:  python3 server_gui.py [host] [cổng]        (mặc định 127.0.0.1 41782; giữ chỗ 30 s, xe giường nằm)
"""
import sys
import tkinter as tk

import mo_phong
from dat_ve_xe_gui import DA_BAN, KHAC_GIU, SoDoXe
from giao_dien_chung import NEN, CuaSoServerCoSo, NhatKy, nhan, o_nhap
from may_chu import MA_TRAM, SO_CHANG, MayChuDatVeXe


class CuaSoServerDatVeXe(CuaSoServerCoSo):
    TIEU_DE = "Máy chủ đặt vé xe khách MR6.14"
    TEN_TAI_NGUYEN = "Tài nguyên: chỗ × chặng (Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang)"

    def _ve_tai_nguyen(self, g):
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w")
        self.cv = tk.Canvas(f, width=184, height=212, highlightthickness=0, bg="#e9efeb")
        self.cv.pack(side="left")
        self.so_do = None
        t = tk.Frame(f, bg=NEN)
        t.pack(side="left", fill="y", padx=(8, 0))
        self.nhan_xe = nhan(t, "", font=("TkDefaultFont", 9, "bold"))
        self.nhan_xe.pack(anchor="w")
        self.nhan_dem = {}
        for ma, chu, mau in (("trong", "Trống", "white"), ("giu", "Đang giữ", KHAC_GIU), ("ban", "Đã bán", DA_BAN)):
            h = tk.Frame(t, bg=NEN)
            h.pack(anchor="w")
            tk.Label(h, width=2, bg=mau, relief="solid", bd=1).pack(side="left")
            self.nhan_dem[ma] = nhan(h, f" {chu}: –", font=("TkDefaultFont", 10, "bold"))
            self.nhan_dem[ma].pack(side="left")
        nhan(t, "Đã bán theo chặng:", font=("TkDefaultFont", 8)).pack(anchor="w", pady=(4, 0))
        self.nhan_chang = nhan(t, "", font=("TkFixedFont", 8), justify="left")
        self.nhan_chang.pack(anchor="w")
        self.nhan_nong = nhan(t, "", font=("TkDefaultFont", 8), justify="left")
        self.nhan_nong.pack(anchor="w", pady=(4, 0))
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(4, 0))
        ts = self.lop.tham_so
        self.o_ts = {}
        for i, (ma, chu, rong) in enumerate([("ttl_s", "Giữ chỗ (s)", 6), ("loai_xe", "Loại xe", 7),
                                             ("ghe_nong", "Ghế nóng", 5)]):
            nhan(f, chu + ":").grid(row=0, column=i * 2, sticky="w", padx=(0 if i == 0 else 6, 2))
            self.o_ts[ma] = o_nhap(f, f"{ts[ma]:g}" if isinstance(ts[ma], float) else ts[ma], rong)
            self.o_ts[ma].grid(row=0, column=i * 2 + 1, sticky="w")
        self._ve_cu = None

    def _tham_so_dat_lai(self):
        try:
            ts = {"ttl_s": float(self.o_ts["ttl_s"].get()), "loai_xe": self.o_ts["loai_xe"].get().strip().lower()}
        except ValueError:
            raise ValueError("thời hạn giữ chỗ phải là số")
        nong = self.o_ts["ghe_nong"].get().strip().upper()
        if nong != self.lop.tham_so["ghe_nong"] or ts["loai_xe"] == self.lop.tham_so["loai_xe"]:
            ts["ghe_nong"] = nong                     # đổi loại xe mà không sửa ghế nóng: máy chủ tự chọn chỗ đầu
        return ts

    def dat_tham_so(self, **ts):
        """Điền ô tham số (kịch bản demo) – như người dùng gõ."""
        for k, v in ts.items():
            self.o_ts[k].delete(0, "end")
            self.o_ts[k].insert(0, str(v))

    def _cap_nhat_tai_nguyen(self, tt, tk_):
        for ma, chu in (("trong", "Trống"), ("giu", "Đang giữ"), ("ban", "Đã bán")):
            self.nhan_dem[ma].configure(text=f" {chu}: {tt['dem'][ma]}")
        self.nhan_xe.configure(text=f"{tt['ten_xe']}, chuyến {tt['chuyen']}")
        n = len(tt["ghe"])
        ban = [sum(d[k] == "b" for d in tt["so_do"]) for k in range(SO_CHANG)]
        self.nhan_chang.configure(text="\n".join(f"{MA_TRAM[k]}-{MA_TRAM[k + 1]} {ban[k]:2d}/{n}" for k in range(SO_CHANG)))
        nong = tt["ghe_nong"]
        if nong:
            i = tt["ghe"].index(nong)
            chu = ", ".join(f"{MA_TRAM[k]}-{MA_TRAM[k + 1]}: {tt['chu'].get(f'{nong}#{k}') or 'trống'}"
                            for k in range(SO_CHANG) if tt["so_do"][i][k] != ".") or "trống cả tuyến"
            self.nhan_nong.configure(text=f"Ghế nóng (mô phỏng): {nong}\n{chu}")
        else:
            self.nhan_nong.configure(text="Ghế nóng (mô phỏng): ngẫu nhiên")
        if self.so_do is None or self.so_do.loai_xe != tt["loai_xe"]:
            self.cv.delete("all")
            self.so_do = SoDoXe(self.cv, tt["loai_xe"], ty_le=0.5, chu=False)
            self._ve_cu = None
            self.dat_tham_so(loai_xe=tt["loai_xe"], ghe_nong=nong)   # đổi loại xe: ô ghế nóng theo máy chủ
        if (tt["so_do"], nong) == self._ve_cu:
            return
        self._ve_cu = (tt["so_do"], nong)
        self.so_do.ve(tt, None, 0, SO_CHANG, vien={nong: "#ff3030"} if nong else None)


def tao_cua_so(root, host, cong):
    return CuaSoServerDatVeXe(root, MayChuDatVeXe, host, cong, mo_phong, che_do="khoa", tre_ms=2.0,
                              tham_so={}, n=500, k=200, lan=mo_phong.SO_LAN_MAC_DINH)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41782
    root = tk.Tk()
    cs = tao_cua_so(root, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
