"""Bài mở rộng MR6.13 – Bảng điều khiển máy chủ bán vé có phòng chờ ảo (Tkinter).

Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ,
nhật ký) ở giao_dien_chung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": VÉ CÒN LẠI (thanh), độ dài
HÀNG ĐỢI, số đang phục vụ, tốc độ cấp lượt (đổi được ngay), mức token trong thùng, số yêu cầu mua đang xử lý
so với giới hạn, số lần từ chối vì quá tải, ô chọn "Bật phòng chờ", tham số đặt lại và nút "So sánh bật/tắt
phòng chờ" (bảng hai dòng: mua được, hết vé, quá tải, bỏ cuộc, thời gian mua p99).

Chạy:  python3 server_gui.py [host] [cổng]        (mặc định 127.0.0.1 41772)
"""
import sys
import tkinter as tk
from tkinter import ttk

import mo_phong
from giao_dien_chung import CAM, DO_DAM, NEN, XANH_DAM, XANH_DUONG, CuaSoServerCoSo, NhatKy, nhan, nut, o_nhap
from may_chu import MayChuPhongCho


class CuaSoServerPhongCho(CuaSoServerCoSo):
    TIEU_DE = "Máy chủ bán vé MR6.13"
    TEN_TAI_NGUYEN = "Tài nguyên: vé và phòng chờ ảo"

    def _ve_tai_nguyen(self, g):
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w")
        nhan(f, "VÉ CÒN LẠI", font=("TkDefaultFont", 8, "bold"), fg="#404040").grid(row=0, column=0, sticky="w")
        nhan(f, "HÀNG ĐỢI", font=("TkDefaultFont", 8, "bold"), fg="#404040").grid(row=0, column=1, sticky="w",
                                                                                 padx=(16, 0))
        self.nhan_ve = nhan(f, "–", font=("TkDefaultFont", 16, "bold"), fg="#8b0000", width=10)
        self.nhan_ve.grid(row=1, column=0, sticky="w")
        self.nhan_hang = nhan(f, "–", font=("TkDefaultFont", 16, "bold"), fg="#1f4fa0", width=12)
        self.nhan_hang.grid(row=1, column=1, sticky="w", padx=(16, 0))
        self.thanh_ve = ttk.Progressbar(g, length=360, maximum=500)
        self.thanh_ve.pack(anchor="w", pady=(0, 2))
        self.nhan_ban = nhan(g, "", font=("TkDefaultFont", 9))
        self.nhan_ban.pack(anchor="w")
        self.nhan_phuc_vu = nhan(g, "", font=("TkDefaultFont", 9))
        self.nhan_phuc_vu.pack(anchor="w")
        self.nhan_xu_ly = nhan(g, "", font=("TkDefaultFont", 9, "bold"))
        self.nhan_xu_ly.pack(anchor="w")
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(2, 0))
        nhan(f, "Cấp lượt:").pack(side="left")
        self.o_toc_do = o_nhap(f, f"{self.lop.tham_so['toc_do']:g}", 6)
        self.o_toc_do.pack(side="left")
        nhan(f, "người/s").pack(side="left")
        self.nut_toc_do = nut(f, "Áp dụng ngay", XANH_DUONG, self.doi_toc_do, 11)
        self.nut_toc_do.pack(side="left", padx=4)
        self.nhan_token = nhan(f, "", font=("TkDefaultFont", 8))
        self.nhan_token.pack(side="left")
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(2, 0))
        ts = self.lop.tham_so
        self.o_ts = {}
        for i, (ma, chu, rong) in enumerate([("tong_ve", "Tổng vé", 6), ("dung_luong", "Thùng token", 5),
                                             ("luot_s", "Hạn lượt (s)", 5), ("gioi_han_xu_ly", "Giới hạn xử lý", 5)]):
            nhan(f, chu + ":").grid(row=i // 2, column=(i % 2) * 2, sticky="w", padx=(0 if i % 2 == 0 else 8, 2))
            self.o_ts[ma] = o_nhap(f, f"{ts[ma]:g}" if isinstance(ts[ma], float) else ts[ma], rong)
            self.o_ts[ma].grid(row=i // 2, column=(i % 2) * 2 + 1, sticky="w")
        self.bien_pc = tk.IntVar(value=ts["phong_cho"])
        self.o_pc = tk.Checkbutton(f, text="Bật phòng chờ ảo (có hiệu lực khi Đặt lại)", variable=self.bien_pc,
                                   bg=NEN, activebackground=NEN)
        self.o_pc.grid(row=2, column=0, columnspan=4, sticky="w")
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(2, 0))
        self.nut_so_sanh_pc = nut(f, "So sánh bật/tắt phòng chờ", CAM, self.so_sanh_pc, 24)
        self.nut_so_sanh_pc.pack(side="left")
        cot = [("pc", "Phòng chờ", 74), ("mua", "Mua được", 66), ("het", "Hết vé", 52), ("qt", "Quá tải", 58),
               ("bo", "Bỏ cuộc", 60), ("p99", "p99 ms", 58)]
        self.bang_pc = ttk.Treeview(g, columns=[c for c, _, _ in cot], show="headings", height=2)
        for c, t, w in cot:
            self.bang_pc.heading(c, text=t)
            self.bang_pc.column(c, width=w, anchor="w" if c == "pc" else "e")
        self.bang_pc.tag_configure("do", background="#ffc8c8", foreground=DO_DAM)
        self.bang_pc.tag_configure("xanh", background="#d8f5d8")
        self.bang_pc.pack(anchor="w", pady=(2, 0))
        self.hang_pc = []

    def _tham_so_dat_lai(self):
        try:
            return {"tong_ve": int(self.o_ts["tong_ve"].get()), "dung_luong": int(self.o_ts["dung_luong"].get()),
                    "luot_s": float(self.o_ts["luot_s"].get()),
                    "gioi_han_xu_ly": int(self.o_ts["gioi_han_xu_ly"].get()),
                    "toc_do": float(self.o_toc_do.get()), "phong_cho": self.bien_pc.get()}
        except ValueError:
            raise ValueError("tham số phải là số")

    def dat_tham_so(self, **ts):
        """Điền ô tham số / ô chọn (kịch bản demo) – như người dùng gõ."""
        for k, v in ts.items():
            if k == "phong_cho":
                if self.bien_pc.get() != int(v):
                    self.o_pc.invoke()
                continue
            o = self.o_toc_do if k == "toc_do" else self.o_ts[k]
            o.delete(0, "end")
            o.insert(0, str(v))

    def doi_toc_do(self):
        try:
            self.lop.dat_toc_do(float(self.o_toc_do.get()))
        except ValueError as e:
            self.nhat_ky.ghi(f"Tốc độ không hợp lệ: {e}")

    # ------------------------------------------------------------ so sánh bật/tắt phòng chờ
    def so_sanh_pc(self):
        n, k, _ = self._doc_mo_phong()
        self._chay_nen(mo_phong.so_sanh_phong_cho, n, k)

    def _chay_nen(self, ham, *ts):
        super()._chay_nen(ham, *ts)
        if self.dang_mo_phong:
            self.nut_so_sanh_pc.configure(state="disabled")

    def _xong_mo_phong(self, hang):
        self.nut_so_sanh_pc.configure(state="normal")
        if not hang or "mua_duoc" not in hang[0]:
            super()._xong_mo_phong(hang)
            return
        super()._xong_mo_phong(None)                    # mở lại nút, "xong"
        self.hang_pc = hang
        for iid in self.bang_pc.get_children():
            self.bang_pc.delete(iid)
        for h in hang:
            self.bang_pc.insert("", "end", values=(h["phong_cho"], h["mua_duoc"], h["het_ve"], h["qua_tai"],
                                                   h["bo_cuoc"], "–" if h["p99_ms"] is None else f"{h['p99_ms']:.1f}"),
                                tags=("do" if h["qua_tai"] else "xanh",))
        for d in mo_phong.bang_phong_cho(hang):
            self.nhat_ky.ghi(d.strip())

    # ------------------------------------------------------------ vẽ lại định kỳ
    def _cap_nhat_tai_nguyen(self, tt, tk_):
        sk = tt["su_kien"]
        self.nhan_ve.configure(text=f"{tt['con_lai']} / {sk['tong_ve']}")
        self.thanh_ve.configure(maximum=sk["tong_ve"], value=max(0, tt["con_lai"]))
        self.nhan_hang.configure(text=f"{tt['dang_cho']} người" if tt["phong_cho"] else "TẮT",
                                 fg="#1f4fa0" if tt["phong_cho"] else "#808080")
        vuot = tt["ban_so"] > sk["tong_ve"]
        self.nhan_ban.configure(text=f"Sổ đã bán: {tt['ban_so']} vé, {tt['so_don']} đơn"
                                     + (f" – VƯỢT {tt['ban_so'] - sk['tong_ve']} vé!" if vuot else "")
                                     + ("  [HẾT VÉ]" if tt["het_ve"] else ""), fg=DO_DAM if vuot else "black")
        self.nhan_phuc_vu.configure(text=f"Đang phục vụ tới #{tt['dang_phuc_vu']} / đã phát #{tt['so_cap']} ·"
                                         f" lượt đã cấp {tt['da_cap_luot']}, hết hạn {tt['het_han']}")
        self.nhan_xu_ly.configure(text=f"Mua đang xử lý: {tt['dang_xu_ly']} / {tt['gioi_han']} (max {tt['xu_ly_max']})"
                                       f" · từ chối quá tải: {tt['qua_tai']}",
                                  fg=DO_DAM if tt["qua_tai"] else XANH_DAM)
        self.nhan_token.configure(text=f"thùng {tt['token']}/{tt['dung_luong']} token")


def tao_cua_so(root, host, cong):
    return CuaSoServerPhongCho(root, MayChuPhongCho, host, cong, mo_phong, che_do="khoa", tre_ms=2.0,
                               n=1000, k=200, lan=mo_phong.SO_LAN_MAC_DINH)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41772
    root = tk.Tk()
    cs = tao_cua_so(root, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
