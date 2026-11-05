"""Bài mở rộng MR6.11 – Bảng điều khiển máy chủ flash sale (Tkinter).

Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ,
nhật ký) ở giao_dien_chung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": sản phẩm, thanh tồn kho,
số đã bán theo trạng thái và theo sổ kiểm toán (đỏ khi vượt tồn đầu kỳ), đồng hồ mở bán, vài đơn gần
nhất và tham số đặt lại (tồn đầu kỳ, mở bán sau bao nhiêu giây).

Chạy:  python3 server_gui.py [host] [cổng]        (mặc định 127.0.0.1 41752)
"""
import sys
import tkinter as tk

import mo_phong
from flash_sale_gui import ve_tai_nghe
from giao_dien_chung import DO_DAM, NEN, XANH_DAM, CuaSoServerCoSo, NhatKy, nhan, o_nhap
from may_chu import MayChuFlashSale


class CuaSoServerFlashSale(CuaSoServerCoSo):
    TIEU_DE = "Máy chủ flash sale MR6.11"
    TEN_TAI_NGUYEN = "Tài nguyên: kho hàng"

    def _ve_tai_nguyen(self, g):
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w")
        cv = tk.Canvas(f, width=120, height=112, highlightthickness=0)
        cv.pack(side="left")
        ve_tai_nghe(cv, 60, 48, 0.55)
        t = tk.Frame(f, bg=NEN)
        t.pack(side="left", fill="y", padx=(8, 0))
        self.nhan_sp = nhan(t, "–", font=("TkDefaultFont", 9, "bold"))
        self.nhan_sp.pack(anchor="w")
        self.nhan_ton = nhan(t, "–", font=("TkDefaultFont", 16, "bold"), fg="#8b0000")
        self.nhan_ton.pack(anchor="w")
        self.cv_kho = tk.Canvas(t, width=180, height=14, bg="#dddddd", highlightthickness=0)
        self.cv_kho.pack(anchor="w", pady=2)
        self.nhan_ban = nhan(t, "–")
        self.nhan_ban.pack(anchor="w")
        self.nhan_mo = nhan(t, "–", font=("TkFixedFont", 11, "bold"))
        self.nhan_mo.pack(anchor="w")
        self.ls = tk.Listbox(g, height=3, width=34, font=("TkFixedFont", 8), activestyle="none")
        self.ls.pack(anchor="w", pady=(2, 0))
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(4, 0))
        ts = self.lop.tham_so
        nhan(f, "Tồn đầu kỳ:").pack(side="left")
        self.o_ts = {"ton_dau": o_nhap(f, ts["ton_dau"], 6)}
        self.o_ts["ton_dau"].pack(side="left")
        nhan(f, "  Mở bán sau (s):").pack(side="left")
        self.o_ts["mo_sau_s"] = o_nhap(f, f"{ts['mo_sau_s']:g}", 5)
        self.o_ts["mo_sau_s"].pack(side="left")

    def _tham_so_dat_lai(self):
        try:
            return {"ton_dau": int(self.o_ts["ton_dau"].get()), "mo_sau_s": float(self.o_ts["mo_sau_s"].get())}
        except ValueError:
            raise ValueError("tham số phải là số")

    def dat_tham_so(self, **ts):
        """Điền ô tham số (kịch bản demo) – như người dùng gõ."""
        for k, v in ts.items():
            self.o_ts[k].delete(0, "end")
            self.o_ts[k].insert(0, str(v))

    def _cap_nhat_tai_nguyen(self, tt, tk_):
        self.nhan_sp.configure(text=f"{tt['sp']['ten']}")
        self.nhan_ton.configure(text=f"Tồn {tt['ton']}/{tt['ton_dau']}")
        ti_le = min(1.0, (tt["ton_dau"] - tt["ton"]) / tt["ton_dau"])
        self.cv_kho.delete("all")
        self.cv_kho.create_rectangle(0, 0, 180 * ti_le, 14, fill="#e4002b" if ti_le >= 0.8 else "#ff6a00", width=0)
        vuot = tt["ban_theo_so"] > tt["ton_dau"]
        self.nhan_ban.configure(text=f"Đã bán {tt['da_ban']} (sổ: {tt['ban_theo_so']}) · {tt['so_don']} đơn",
                                fg=DO_DAM if vuot or tt["ban_theo_so"] != tt["da_ban"] else "black")
        if not tt["mo_ban"]:
            g = tt["mo_con_ms"] // 1000
            self.nhan_mo.configure(text=f"Mở bán sau 00:{g // 60:02d}:{g % 60:02d}", fg="#a06000")
        elif tt["ton"] <= 0:
            self.nhan_mo.configure(text="HẾT HÀNG", fg="#606060")
        else:
            self.nhan_mo.configure(text="ĐANG MỞ BÁN", fg=XANH_DAM)
        ls = [f"{x['gio']} {x['nguoi']:<6} ×{x['so_luong']}" for x in tt["don_gan"][:3]]
        if list(self.ls.get(0, "end")) != ls:
            self.ls.delete(0, "end")
            for x in ls:
                self.ls.insert("end", x)


def tao_cua_so(root, host, cong):
    return CuaSoServerFlashSale(root, MayChuFlashSale, host, cong, mo_phong, che_do="khoa", tre_ms=2.0,
                                tham_so={}, n=10000, k=200, lan=mo_phong.SO_LAN_MAC_DINH)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41752
    root = tk.Tk()
    cs = tao_cua_so(root, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
