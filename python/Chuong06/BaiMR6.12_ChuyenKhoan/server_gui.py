"""Bài mở rộng MR6.12 – Bảng điều khiển máy chủ ngân hàng (Tkinter).

Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ,
nhật ký) ở giao_dien_chung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": TỔNG TIỀN HỆ THỐNG (xanh khi
khớp tổng ban đầu, đỏ khi lệch), bảng tài khoản dạng thanh số dư, số giao dịch, số vụ khóa chết, vài giao
dịch gần nhất và tham số đặt lại (số tài khoản, số dư ban đầu, ô chọn "khóa ngây thơ").

Chạy:  python3 server_gui.py [host] [cổng]        (mặc định 127.0.0.1 41762)
"""
import sys
import tkinter as tk

import mo_phong
from chung import tien
from giao_dien_chung import DO_DAM, NEN, XANH_DAM, CuaSoServerCoSo, NhatKy, nhan, o_nhap
from may_chu import MayChuNganHang, tien_dau


class CuaSoServerNganHang(CuaSoServerCoSo):
    TIEU_DE = "Máy chủ ngân hàng MR6.12"
    TEN_TAI_NGUYEN = "Tài nguyên: các tài khoản"

    def _ve_tai_nguyen(self, g):
        nhan(g, "TỔNG TIỀN HỆ THỐNG", font=("TkDefaultFont", 8, "bold"), fg="#404040").pack(anchor="w")
        self.nhan_tong = tk.Label(g, text="–", font=("TkDefaultFont", 16, "bold"), bg="#e0e0e0", width=24,
                                  anchor="w", padx=4)
        self.nhan_tong.pack(anchor="w")
        self.nhan_tong_ct = nhan(g, "", font=("TkDefaultFont", 8))
        self.nhan_tong_ct.pack(anchor="w")
        self.cv = tk.Canvas(g, width=360, height=150, bg="white", highlightthickness=1,
                            highlightbackground="#808080")
        self.cv.pack(anchor="w", pady=(2, 2))
        self.nhan_gd = nhan(g, "", font=("TkDefaultFont", 9))
        self.nhan_gd.pack(anchor="w")
        self.ls = tk.Listbox(g, height=3, width=50, font=("TkFixedFont", 8), activestyle="none")
        self.ls.pack(anchor="w", pady=(2, 0))
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(4, 0))
        ts = self.lop.tham_so
        nhan(f, "Số tài khoản:").grid(row=0, column=0, sticky="w")
        self.o_so_tk = o_nhap(f, ts["so_tk"], 4)
        self.o_so_tk.grid(row=0, column=1, sticky="w")
        nhan(f, "  Số dư ban đầu:").grid(row=0, column=2, sticky="w")
        self.o_so_du = o_nhap(f, ts["so_du_dau"], 10)
        self.o_so_du.grid(row=0, column=3, sticky="w")
        self.bien_ngay_tho = tk.IntVar(value=ts["khoa_ngay_tho"])
        self.o_ngay_tho = tk.Checkbutton(f, text="Khóa ngây thơ: nguồn rồi đích (có thể khóa chết)",
                                         variable=self.bien_ngay_tho, bg=NEN, activebackground=NEN)
        self.o_ngay_tho.grid(row=1, column=0, columnspan=4, sticky="w")

    def _tham_so_dat_lai(self):
        try:
            return {"so_tk": int(self.o_so_tk.get()), "so_du_dau": int(self.o_so_du.get()),
                    "khoa_ngay_tho": self.bien_ngay_tho.get()}
        except ValueError:
            raise ValueError("tham số phải là số")

    def dat_khoa_ngay_tho(self, bat):
        """Bấm ô chọn "khóa ngây thơ" (kịch bản demo) – có hiệu lực khi bấm Đặt lại."""
        if self.bien_ngay_tho.get() != int(bat):
            self.o_ngay_tho.invoke()

    def _cap_nhat_tai_nguyen(self, tt, tk_):
        lech = tt["tong"] - tt["tong_dau"]
        self.nhan_tong.configure(text=tien(tt["tong"]), bg="#ff6060" if lech else "#70d070",
                                 fg="white" if lech else "black")
        self.nhan_tong_ct.configure(text=f"ban đầu {tien(tt['tong_dau'])} – "
                                         + (f"LỆCH {tien_dau(lech)}" if lech else "khớp"),
                                    fg=DO_DAM if lech else XANH_DAM)
        self._ve_tai_khoan(tt)
        self.nhan_gd.configure(text=f"Giao dịch: {tt['so_gd']}   Khóa chết phát hiện: {tt['khoa_chet']}"
                                    + ("   [khóa ngây thơ]" if tt["khoa_ngay_tho"] else "   [khóa theo thứ tự]"),
                               fg=DO_DAM if tt["khoa_chet"] else "black")
        ls = [f"{g['gio']} {g['tu']}→{g['den']} {tien(g['so_tien']):>13} {g['nguoi']}" for g in tt["lich_su"][:3]]
        if list(self.ls.get(0, "end")) != ls:
            self.ls.delete(0, "end")
            for x in ls:
                self.ls.insert("end", x)

    def _ve_tai_khoan(self, tt):
        """Mỗi tài khoản một thanh số dư (≤ 10 dòng mỗi cột, 2 cột khi > 10 tài khoản)."""
        ds = tt["tai_khoan"]
        khoa = tuple((t["ma"], t["so_du"]) for t in ds)
        if khoa == getattr(self, "_ve_cu", None):
            return
        self._ve_cu = khoa
        cv = self.cv
        cv.delete("all")
        dau = tt["tong_dau"] // max(1, len(ds))
        lon = max([2 * dau] + [t["so_du"] for t in ds]) or 1
        cot = 1 if len(ds) <= 10 else 2
        rong, cao = 360 // cot, 15
        for i, t in enumerate(ds):
            x, y = (i // 10) * rong + 4, (i % 10) * cao + 3
            cv.create_text(x, y + 6, text=f"{t['ma']} {t['chu']}", anchor="w", font=("TkDefaultFont", 7))
            x0, x1 = x + 62, x + rong - 76
            cv.create_rectangle(x0, y + 2, x1, y + 11, fill="#eeeeee", outline="")
            cv.create_rectangle(x0, y + 2, x0 + (x1 - x0) * t["so_du"] / lon, y + 11,
                                fill="#4a90d9" if t["so_du"] >= dau else "#f0a030", outline="")
            cv.create_line(x0 + (x1 - x0) * dau / lon, y, x0 + (x1 - x0) * dau / lon, y + 13, fill="#808080")
            cv.create_text(x + rong - 6, y + 6, text=tien(t["so_du"]), anchor="e", font=("TkFixedFont", 7))


def tao_cua_so(root, host, cong):
    return CuaSoServerNganHang(root, MayChuNganHang, host, cong, mo_phong, che_do="khoa", tre_ms=2.0,
                               n=500, k=200, lan=mo_phong.SO_LAN_MAC_DINH)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41762
    root = tk.Tk()
    cs = tao_cua_so(root, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
