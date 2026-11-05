"""Bài mở rộng MR6.6 – Bảng điều khiển máy chủ đấu giá (Tkinter).

Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ,
nhật ký) ở giao_dien_chung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": món hàng, giá hiện tại,
người dẫn đầu, đồng hồ phiên, vài giá gần nhất và tham số đặt lại phiên.

Chạy:  python3 server_gui.py [host] [cổng]        (mặc định 127.0.0.1 41702; phiên mặc định 120 s)
"""
import sys
import tkinter as tk

import mo_phong
from chung import tien
from dau_gia_gui import dong_ho, ve_binh_gom
from giao_dien_chung import DO_DAM, NEN, XANH_DAM, CuaSoServerCoSo, NhatKy, nhan, o_nhap
from may_chu import MayChuDauGia


class CuaSoServerDauGia(CuaSoServerCoSo):
    TIEU_DE = "Máy chủ đấu giá MR6.6"
    TEN_TAI_NGUYEN = "Tài nguyên: phiên đấu giá"

    def _ve_tai_nguyen(self, g):
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w")
        cv = tk.Canvas(f, width=120, height=130, highlightthickness=0)
        cv.pack(side="left")
        ve_binh_gom(cv, 60, 14, 0.62)
        t = tk.Frame(f, bg=NEN)
        t.pack(side="left", fill="y", padx=(8, 0))
        self.nhan_mon = nhan(t, "–", font=("TkDefaultFont", 10, "bold"))
        self.nhan_mon.pack(anchor="w")
        self.nhan_gia = nhan(t, "–", font=("TkDefaultFont", 18, "bold"), fg="#8b0000")
        self.nhan_gia.pack(anchor="w")
        self.nhan_dan_dau = nhan(t, "Dẫn đầu: –")
        self.nhan_dan_dau.pack(anchor="w")
        self.nhan_dong_ho = nhan(t, "Còn lại: –", font=("TkFixedFont", 12, "bold"))
        self.nhan_dong_ho.pack(anchor="w")
        self.ls = tk.Listbox(t, height=3, width=30, font=("TkFixedFont", 8), activestyle="none")
        self.ls.pack(anchor="w", pady=(2, 0))
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(4, 0))
        ts = self.lop.tham_so
        self.o_ts = {}
        for i, (ma, chu, rong) in enumerate([("gia_khoi_diem", "Giá khởi điểm", 10), ("buoc_gia", "Bước giá", 10),
                                             ("thoi_gian_s", "Phiên (s, 0 = ∞)", 6), ("gia_han_s", "Gia hạn (s)", 6)]):
            nhan(f, chu + ":").grid(row=i // 2, column=(i % 2) * 2, sticky="w", padx=(0 if i % 2 == 0 else 8, 2))
            self.o_ts[ma] = o_nhap(f, f"{ts[ma]:g}" if isinstance(ts[ma], float) else ts[ma], rong)
            self.o_ts[ma].grid(row=i // 2, column=(i % 2) * 2 + 1, sticky="w")

    def _tham_so_dat_lai(self):
        try:
            return {"gia_khoi_diem": int(self.o_ts["gia_khoi_diem"].get()), "buoc_gia": int(self.o_ts["buoc_gia"].get()),
                    "thoi_gian_s": float(self.o_ts["thoi_gian_s"].get()), "gia_han_s": float(self.o_ts["gia_han_s"].get())}
        except ValueError:
            raise ValueError("tham số phải là số")

    def dat_tham_so(self, **ts):
        """Điền ô tham số (kịch bản demo) – như người dùng gõ."""
        for k, v in ts.items():
            self.o_ts[k].delete(0, "end")
            self.o_ts[k].insert(0, str(v))

    def _cap_nhat_tai_nguyen(self, tt, tk_):
        self.nhan_mon.configure(text=f"{tt['mon']['ten']} (bước {tien(tt['mon']['buoc_gia'])})")
        self.nhan_gia.configure(text=tien(tt["gia"]))
        ai = tt["nguoi_dan_dau"] or "–"
        if tt["ket_thuc"]:
            self.nhan_dan_dau.configure(text=f"THẮNG: {ai}  ({tt['so_lan']} lượt)", fg=DO_DAM)
            self.nhan_dong_ho.configure(text="Kết thúc phiên", fg="#606060")
        else:
            self.nhan_dan_dau.configure(text=f"Dẫn đầu: {ai}  ({tt['so_lan']} lượt)", fg="black")
            con = tt["con_lai_ms"]
            self.nhan_dong_ho.configure(text="Còn lại: ∞ (không giới hạn)" if con is None else
                                        f"Còn lại: {dong_ho(con / 1000)}",
                                        fg=XANH_DAM if con is None or con >= 10_000 else DO_DAM)
        ls = [f"{x['nguoi']:<6}{tien(x['gia']):>14}" + (" GH" if x["gia_han"] else "") for x in tt["lich_su"][:3]]
        if list(self.ls.get(0, "end")) != ls:
            self.ls.delete(0, "end")
            for x in ls:
                self.ls.insert("end", x)


def tao_cua_so(root, host, cong):
    return CuaSoServerDauGia(root, MayChuDauGia, host, cong, mo_phong, che_do="khoa", tre_ms=2.0,
                             tham_so={"thoi_gian_s": 120.0}, n=200, k=200, lan=mo_phong.SO_LAN_MAC_DINH)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41702
    root = tk.Tk()
    cs = tao_cua_so(root, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
