"""Bài mở rộng MR6.9 – Bảng điều khiển máy chủ quay số (Tkinter).

Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ,
nhật ký) ở giao_dien_chung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": 10 món quà (ảnh, còn /
đã trao cho ai), số quà còn lại, số lượt đã quay, số lượt trúng theo sổ kiểm toán, tham số đặt lại
(số quà, xác suất p).

Chạy:  python3 server_gui.py [host] [cổng]        (mặc định 127.0.0.1 41732)
"""
import sys
import tkinter as tk

import mo_phong
from giao_dien_chung import DO_DAM, NEN, XANH_DAM, CuaSoServerCoSo, NhatKy, nhan, o_nhap
from may_chu import TEN_QUA, MayChuQuaySo, chu_p
from vong_quay_gui import ve_qua


class CuaSoServerQuaySo(CuaSoServerCoSo):
    TIEU_DE = "Máy chủ quay số MR6.9"
    TEN_TAI_NGUYEN = "Tài nguyên: kho quà"

    def _ve_tai_nguyen(self, g):
        self.nhan_con = nhan(g, "–", font=("TkDefaultFont", 14, "bold"), fg="#8b0000")
        self.nhan_con.pack(anchor="w")
        self.nhan_luot = nhan(g, "–", font=("TkDefaultFont", 9))
        self.nhan_luot.pack(anchor="w")
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(2, 0))
        self.o_qua = []
        for i in range(len(TEN_QUA)):
            o = tk.Frame(f, bg=NEN)
            o.grid(row=i // 5, column=i % 5, padx=1, pady=1)
            cv = tk.Canvas(o, width=40, height=34, bg="white", highlightthickness=2, highlightbackground="#808080")
            cv.pack()
            nh = nhan(o, "", font=("TkDefaultFont", 7), anchor="center", width=9)
            nh.pack()
            self.o_qua.append((cv, nh))
        self._da_ve = None
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(4, 0))
        ts = self.lop.tham_so
        nhan(f, "Số quà:").pack(side="left")
        self.o_ts = {"so_qua": o_nhap(f, ts["so_qua"], 4)}
        self.o_ts["so_qua"].pack(side="left")
        nhan(f, "  Xác suất p:").pack(side="left")
        self.o_ts["p"] = o_nhap(f, f"{ts['p']:g}", 8)
        self.o_ts["p"].pack(side="left")

    def _tham_so_dat_lai(self):
        try:
            return {"so_qua": int(self.o_ts["so_qua"].get()), "p": float(self.o_ts["p"].get())}
        except ValueError:
            raise ValueError("tham số phải là số")

    def dat_tham_so(self, **ts):
        """Điền ô tham số (kịch bản demo) – như người dùng gõ."""
        for k, v in ts.items():
            self.o_ts[k].delete(0, "end")
            self.o_ts[k].insert(0, str(v))

    def _cap_nhat_tai_nguyen(self, tt, tk_):
        self.nhan_con.configure(text=f"Còn {tt['con_lai']}/{tt['so_qua']} quà · trao {tt['so_trung']}",
                                fg=DO_DAM if tt["so_trung"] > tt["so_qua"] else "#8b0000")
        self.nhan_luot.configure(text=f"Đã quay {tt['da_quay']:,} lượt · p = {chu_p(tt['p'])}".replace(",", "."))
        ve = tuple((q["ma"], q["con"]) for q in tt["qua"])
        if ve != self._da_ve:
            self._da_ve = ve
            for i, (cv, nh) in enumerate(self.o_qua):
                cv.delete("all")
                if i < tt["so_qua"]:
                    con = tt["qua"][i]["con"]
                    ve_qua(cv, i, 20, 17, 14, xam=not con)
                    cv.configure(highlightbackground=XANH_DAM if con else "#b0b0b0",
                                 bg="white" if con else "#e4e4e4")
                else:
                    cv.configure(highlightbackground="#b0b0b0", bg="#d0d0d0")
        for i, (cv, nh) in enumerate(self.o_qua):
            if i < tt["so_qua"]:
                q = tt["qua"][i]
                nh.configure(text=q["ten"] if q["con"] else f"→{q['nguoi'] or '?'}",
                             fg="black" if q["con"] else "#a00000")
            else:
                nh.configure(text="")


def tao_cua_so(root, host, cong):
    return CuaSoServerQuaySo(root, MayChuQuaySo, host, cong, mo_phong, che_do="khoa", tre_ms=2.0,
                             tham_so={}, n=10000, k=200, lan=mo_phong.SO_LAN_MAC_DINH)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41732
    root = tk.Tk()
    cs = tao_cua_so(root, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
