"""Bài mở rộng MR6.10 – Bảng điều khiển máy chủ đăng ký học phần (Tkinter).

Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ,
nhật ký) ở giao_dien_chung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": mỗi lớp một thanh
"đã đăng ký / sĩ số" theo trạng thái máy chủ, kèm số sinh viên theo sổ kiểm toán (đỏ khi vượt sĩ số),
và tham số đặt lại (số lớp, sĩ số).

Chạy:  python3 server_gui.py [host] [cổng]        (mặc định 127.0.0.1 41742)
"""
import sys
import tkinter as tk

import mo_phong
from giao_dien_chung import DO_DAM, NEN, CuaSoServerCoSo, NhatKy, nhan, o_nhap
from may_chu import DANH_MUC, MayChuDangKy

RONG = 150                                               # bề rộng thanh sĩ số (điểm ảnh)


class CuaSoServerDangKy(CuaSoServerCoSo):
    TIEU_DE = "Máy chủ đăng ký học phần MR6.10"
    TEN_TAI_NGUYEN = "Tài nguyên: lớp học phần"

    def _ve_tai_nguyen(self, g):
        self.nhan_tong = nhan(g, "–", font=("TkDefaultFont", 12, "bold"), fg="#8b0000")
        self.nhan_tong.pack(anchor="w")
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(2, 0))
        self.hang = []
        for i in range(len(DANH_MUC)):
            ma = nhan(f, "", font=("TkFixedFont", 9), width=10)
            ma.grid(row=i, column=0, sticky="w")
            cv = tk.Canvas(f, width=RONG, height=14, bg="#dddddd", highlightthickness=0)
            cv.grid(row=i, column=1, padx=4, pady=1)
            so = nhan(f, "", font=("TkDefaultFont", 9, "bold"), width=13)
            so.grid(row=i, column=2, sticky="w")
            self.hang.append((ma, cv, so))
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(4, 0))
        ts = self.lop.tham_so
        nhan(f, "Số lớp:").pack(side="left")
        self.o_ts = {"so_lop": o_nhap(f, ts["so_lop"], 3)}
        self.o_ts["so_lop"].pack(side="left")
        nhan(f, "  Sĩ số:").pack(side="left")
        self.o_ts["si_so"] = o_nhap(f, ts["si_so"], 5)
        self.o_ts["si_so"].pack(side="left")

    def _tham_so_dat_lai(self):
        try:
            return {"so_lop": int(self.o_ts["so_lop"].get()), "si_so": int(self.o_ts["si_so"].get())}
        except ValueError:
            raise ValueError("tham số phải là số nguyên")

    def dat_tham_so(self, **ts):
        """Điền ô tham số (kịch bản demo) – như người dùng gõ."""
        for k, v in ts.items():
            self.o_ts[k].delete(0, "end")
            self.o_ts[k].insert(0, str(v))

    def _cap_nhat_tai_nguyen(self, tt, tk_):
        self.nhan_tong.configure(text=tk_["tom_tat"])
        for i, (ma, cv, so) in enumerate(self.hang):
            cv.delete("all")
            if i >= len(tt["lop"]):
                ma.configure(text="")
                so.configure(text="")
                cv.configure(bg=NEN)
                continue
            x = tt["lop"][i]
            cv.configure(bg="#dddddd")
            ma.configure(text=x["ma"])
            day = x["da_dk"] >= x["si_so"]
            cv.create_rectangle(0, 0, RONG * min(1.0, x["da_dk"] / x["si_so"]), 14,
                                fill=DO_DAM if day else "#3cb44b", width=0)
            vuot = x["theo_so"] > x["si_so"]
            so.configure(text=f"{x['da_dk']}/{x['si_so']}" + (f" (sổ {x['theo_so']})" if vuot else ""),
                         fg=DO_DAM if vuot else "black")


def tao_cua_so(root, host, cong):
    return CuaSoServerDangKy(root, MayChuDangKy, host, cong, mo_phong, che_do="khoa", tre_ms=2.0,
                             tham_so={}, n=2000, k=200, lan=mo_phong.SO_LAN_MAC_DINH)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41742
    root = tk.Tk()
    cs = tao_cua_so(root, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
