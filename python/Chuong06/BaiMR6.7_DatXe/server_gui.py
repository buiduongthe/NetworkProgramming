"""Bài mở rộng MR6.7 – Bảng điều khiển máy chủ đặt xe (Tkinter).

Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ,
nhật ký) ở giao_dien_chung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": bản đồ thu nhỏ với các
xe (xe đã có khách mờ đi, ghi tên khách), số xe trống, danh sách xe và tham số đặt lại (số xe).

Chạy:  python3 server_gui.py [host] [cổng]        (mặc định 127.0.0.1 41712; 1 xe)
"""
import sys
import tkinter as tk

import mo_phong
from dat_xe_gui import BanDo
from giao_dien_chung import DO_DAM, NEN, XANH_DAM, CuaSoServerCoSo, NhatKy, nhan, o_nhap
from may_chu import MayChuDatXe


class CuaSoServerDatXe(CuaSoServerCoSo):
    TIEU_DE = "Máy chủ đặt xe MR6.7"
    TEN_TAI_NGUYEN = "Tài nguyên: các xe (mỗi xe 1 chỗ)"
    RONG, CAO = 200, 150

    def _ve_tai_nguyen(self, g):
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w")
        self.cv = tk.Canvas(f, width=self.RONG, height=self.CAO, highlightthickness=1, highlightbackground="#808080")
        self.cv.pack(side="left")
        self.ban_do = BanDo(self.cv, self.RONG, self.CAO, nho=True)
        t = tk.Frame(f, bg=NEN)
        t.pack(side="left", fill="y", padx=(8, 0))
        self.nhan_trong = nhan(t, "–", font=("TkDefaultFont", 16, "bold"))
        self.nhan_trong.pack(anchor="w")
        self.ls = tk.Listbox(t, height=6, width=26, font=("TkFixedFont", 8), activestyle="none")
        self.ls.pack(anchor="w", pady=(2, 0))
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(4, 0))
        nhan(f, "Số xe (1–50):").pack(side="left")
        self.o_so_xe = o_nhap(f, self.lop.tham_so["so_xe"], 5)
        self.o_so_xe.pack(side="left", padx=4)
        self._ve_cu = None

    def _tham_so_dat_lai(self):
        try:
            return {"so_xe": int(self.o_so_xe.get())}
        except ValueError:
            raise ValueError("số xe phải là số nguyên")

    def dat_tham_so(self, **ts):
        """Điền ô tham số (kịch bản demo) – như người dùng gõ."""
        if "so_xe" in ts:
            self.o_so_xe.delete(0, "end")
            self.o_so_xe.insert(0, str(ts["so_xe"]))

    def _cap_nhat_tai_nguyen(self, tt, tk_):
        trong = tt["trong"]
        self.nhan_trong.configure(text=f"Trống {trong}/{tt['so_xe']} xe", fg=XANH_DAM if trong else DO_DAM)
        khoa_ve = tuple((x["ma"], x["khach"]) for x in tt["xe"])
        if khoa_ve == self._ve_cu:
            return
        self._ve_cu = khoa_ve
        self.ban_do.xoa()
        for x in tt["xe"]:
            self.ban_do.xe(*x["vi_tri"], x["mau"], x["khach"] or x["tai_xe"], mo=x["khach"] is not None)
        self.ls.delete(0, "end")
        for x in tt["xe"]:
            self.ls.insert("end", f"{x['ma']:>2} {x['tai_xe']:<6}{x['bien_so']:<11}{x['khach'] or '·'}")
            if x["khach"]:
                self.ls.itemconfigure("end", fg="#a00000")


def tao_cua_so(root, host, cong):
    return CuaSoServerDatXe(root, MayChuDatXe, host, cong, mo_phong, che_do="khoa", tre_ms=2.0,
                            tham_so={"so_xe": 1}, n=500, k=200, lan=mo_phong.SO_LAN_MAC_DINH)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41712
    root = tk.Tk()
    cs = tao_cua_so(root, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
