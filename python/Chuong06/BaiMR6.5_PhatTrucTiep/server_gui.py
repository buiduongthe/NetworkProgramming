"""Bài mở rộng MR6.5 – Máy chủ phát trực tiếp có giao diện Tkinter.

Lõi MayChuPhat (may_chu.py) chạy các luồng mạng; cửa sổ chỉ gọi phương thức của lõi và cứ 100 ms
đọc trang_thai() để vẽ lại. Server bật `xem_truoc`: tự giải mã MỘT bản luồng video (chỉ để hiện
ảnh xem trước, việc chuyển tiếp vẫn là nhân bản nguyên byte – không giải nén).

Chạy:  python3 server_gui.py [host] [cổng_tcp] [cổng_udp]
"""
import sys
import threading
import tkinter as tk
from tkinter import ttk

from chung import CAO, RONG
from giao_dien_chung import (CAM, DO, NEN, XANH_LA, HangDoiGiaoDien, NhatKy, anh_tk, khung_nhom, nhan,
                             nut, o_nhap)
from may_chu import MayChuPhat


class CuaSoServer:
    def __init__(self, root, host="127.0.0.1", cong_tcp=41652, cong_udp=None):
        self.root = root
        self.ui = HangDoiGiaoDien(root)
        self.lop = MayChuPhat(bao=lambda t: self.ui.dua_vao(self.nhat_ky.ghi, t))
        self.lop.xem_truoc = True
        self.anh = None
        self.so_da_ve = None
        root.title("Máy chủ phát trực tiếp (SFU) – MR6.5")
        root.configure(bg=NEN)
        root.resizable(False, False)

        tren = tk.Frame(root, bg=NEN)
        tren.pack(fill="x", padx=6, pady=4)
        g = khung_nhom(tren, "Kết nối")
        g.pack(side="left", fill="y")
        nhan(g, "IP:").grid(row=0, column=0, sticky="w")
        self.o_ip = o_nhap(g, host, 14)
        self.o_ip.grid(row=0, column=1, columnspan=3, sticky="w")
        nhan(g, "TCP:").grid(row=1, column=0, sticky="w")
        self.o_tcp = o_nhap(g, cong_tcp, 6)
        self.o_tcp.grid(row=1, column=1, sticky="w")
        nhan(g, "UDP:").grid(row=1, column=2, sticky="e")
        self.o_udp = o_nhap(g, cong_udp or cong_tcp + 1, 6)
        self.o_udp.grid(row=1, column=3, sticky="w")
        self.nut_start = nut(g, "START SERVER", XANH_LA, self.bat_dau)
        self.nut_start.grid(row=2, column=0, columnspan=2, pady=3)
        self.nut_stop = nut(g, "STOP SERVER", DO, self.dung)
        self.nut_stop.grid(row=2, column=2, columnspan=2, pady=3)
        self.nut_stop.configure(state="disabled")

        g = khung_nhom(tren, "Bộ đệm GOP (GOP cache)")
        g.pack(side="left", fill="y", padx=6)
        self.bien_bo_dem = tk.BooleanVar(value=True)
        tk.Checkbutton(g, text="Bật – người vào muộn nhận ngay\nkhung I mới nhất + các P sau nó",
                       variable=self.bien_bo_dem, bg=NEN, justify="left",
                       command=lambda: self.lop.dat_bo_dem(self.bien_bo_dem.get())).pack(anchor="w")
        self.nhan_bo_dem = nhan(g, "Đang giữ: –")
        self.nhan_bo_dem.pack(anchor="w")

        g = khung_nhom(tren, "Chỉ số (mỗi giây)")
        g.pack(side="left", fill="both", expand=True)
        self.nhan_vao = nhan(g, "Vào: –", font=("TkDefaultFont", 10, "bold"))
        self.nhan_vao.pack(anchor="w")
        self.nhan_ra = nhan(g, "Ra: –", font=("TkDefaultFont", 10, "bold"))
        self.nhan_ra.pack(anchor="w")
        self.nhan_he_so = nhan(g, "Hệ số nhân bản (ra/vào): –", fg="#003a80", font=("TkDefaultFont", 10, "bold"))
        self.nhan_he_so.pack(anchor="w")
        self.nhan_pli = nhan(g, "PLI chuyển: 0 (bỏ 0)")
        self.nhan_pli.pack(anchor="w")

        giua = tk.Frame(root, bg=NEN)
        giua.pack(fill="x", padx=6)
        g = khung_nhom(giua, "Xem trước luồng (server tự giải mã)")
        g.pack(side="left", fill="y")
        self.canvas = tk.Canvas(g, width=2 * RONG, height=2 * CAO, bg="black", highlightthickness=0)
        self.canvas.pack()
        self.muc_anh = self.canvas.create_image(0, 0, anchor="nw")
        self.chu_cho = self.canvas.create_text(RONG, CAO, text="Chưa có luồng", fill="#c0c0c0")
        self.nhan_dong_ho = nhan(g, "Đồng hồ: –", font=("TkFixedFont", 10))
        self.nhan_dong_ho.pack(anchor="w")

        g = khung_nhom(giua, "Người xem")
        g.pack(side="left", fill="both", expand=True, padx=(6, 0))
        cot = [("ten", "Tên", 60), ("udp", "IP:cổng UDP", 130), ("nhan", "Nhận", 45), ("goi", "Gói gửi", 65),
               ("kbps", "kbit/s", 55), ("dau", "Khung I đầu", 120)]
        self.bang = ttk.Treeview(g, columns=[c for c, _, _ in cot], show="headings", height=9)
        for c, t, w in cot:
            self.bang.heading(c, text=t)
            self.bang.column(c, width=w, anchor="w" if c in ("ten", "udp", "dau") else "e")
        self.bang.pack(fill="both", expand=True)
        f = tk.Frame(g, bg=NEN)
        f.pack(fill="x", pady=(4, 0))
        self.nut_ngat = nut(f, "Ngắt người chọn", CAM, self.ngat_chon, 16)
        self.nut_ngat.pack(side="left")
        self.nhan_phat = nhan(f, "  Máy phát: –")
        self.nhan_phat.pack(side="left")

        g = khung_nhom(root, "Nhật ký")
        g.pack(fill="x", padx=6, pady=4)
        self.nhat_ky = NhatKy(g, cao=7, rong=110, ten="server")
        self.nhat_ky.pack(fill="x")
        self.thanh_tt = tk.Label(root, text="Trạng thái: server chưa chạy", anchor="w", relief="sunken", bg="white")
        self.thanh_tt.pack(fill="x", padx=6, pady=(0, 6))
        root.after(100, self.cap_nhat)

    # ------------------------------------------------------------ nút
    def bat_dau(self):
        try:
            self.lop.bo_dem_bat = self.bien_bo_dem.get()
            self.lop.bat_dau(self.o_ip.get().strip(), int(self.o_tcp.get()), int(self.o_udp.get()))
        except (OSError, ValueError) as e:
            self.nhat_ky.ghi(f"Không khởi động được: {e}")
            return
        self.nut_start.configure(state="disabled")
        self.nut_stop.configure(state="normal")
        for o in (self.o_ip, self.o_tcp, self.o_udp):
            o.configure(state="disabled")

    def dung(self):
        self.nut_stop.configure(state="disabled")

        def nen():                                         # chờ luồng nền dừng: không chặn giao diện
            self.lop.dung()
            self.ui.dua_vao(self._da_dung)
        threading.Thread(target=nen, daemon=True).start()

    def _da_dung(self):
        self.nut_start.configure(state="normal")
        for o in (self.o_ip, self.o_tcp, self.o_udp):
            o.configure(state="normal")

    def ngat_chon(self):
        for iid in self.bang.selection():
            self.lop.ngat(ma=int(iid))

    def chon_nguoi_xem(self, ten):
        """Chọn dòng theo tên (dùng trong kịch bản demo)."""
        for x in self.lop.trang_thai()["nguoi_xem"]:
            if x["ten"] == ten and self.bang.exists(str(x["ma"])):
                self.bang.selection_set(str(x["ma"]))

    # ------------------------------------------------------------ vẽ lại định kỳ
    def cap_nhat(self):
        tt = self.lop.trang_thai()
        if self.lop.chay:
            self.nhan_vao.configure(text=f"Vào: {tt['kbps_vao']:.0f} kbit/s (tiếng {tt['kbps_vao_am']:.0f},"
                                         f" hình {tt['kbps_vao_hinh']:.0f})")
            self.nhan_ra.configure(text=f"Ra: {tt['kbps_ra']:.0f} kbit/s tới {len(tt['nguoi_xem'])} người xem")
            self.nhan_he_so.configure(text=f"Hệ số nhân bản (ra/vào): {tt['he_so']:.2f}")
            self.nhan_pli.configure(text=f"PLI chuyển: {tt['pli_chuyen']} (bỏ {tt['pli_bo']} do lọc 500 ms)")
            self.nhan_bo_dem.configure(
                text=(f"Đang giữ: I #{tt['bo_dem_so_i']} và các P sau: {tt['bo_dem_goi']} gói,"
                      f" {tt['bo_dem_byte'] / 1024:.0f} KB"
                      if tt["bo_dem_bat"] and tt["bo_dem_so_i"] is not None else
                      ("Đang giữ: – (chờ khung I)" if tt["bo_dem_bat"] else "Tắt: người vào muộn chờ khung I")))
            self.nhan_phat.configure(text=f"  Máy phát: {tt['ten_phat'] or '–'}"
                                          + (" (đang phát)" if tt["dang_phat"] else ""))
            self.thanh_tt.configure(text=f"Trạng thái: server chạy tại {self.lop.dia_chi} (UDP {self.lop.cong_udp})"
                                         f" – {'đang phát' if tt['dang_phat'] else 'chưa phát'},"
                                         f" {len(tt['nguoi_xem'])} người xem")
        else:
            self.thanh_tt.configure(text="Trạng thái: server chưa chạy")
        co = set()
        for x in tt["nguoi_xem"]:
            iid = str(x["ma"])
            co.add(iid)
            dau = "–" if x["khung_dau_ms"] is None else f"{x['khung_dau_ms']:.0f} ms ({x['nguon_dau']})"
            gt = (x["ten"], x["udp"], x["nhan"], x["goi_ra"], f"{x['kbps']:.0f}", dau)
            if self.bang.exists(iid):
                self.bang.item(iid, values=gt)
            else:
                self.bang.insert("", "end", iid=iid, values=gt)
        for iid in self.bang.get_children():
            if iid not in co:
                self.bang.delete(iid)
        a = tt["anh"]
        if a is not None and (a[3], a[4]) != self.so_da_ve and self.lop.chay:
            w, h, rgb, ts, so = a
            self.anh = anh_tk(rgb, w, h, 2)
            self.canvas.itemconfigure(self.muc_anh, image=self.anh)
            self.canvas.itemconfigure(self.chu_cho, text="")
            self.nhan_dong_ho.configure(text=f"Đồng hồ phương tiện: t={ts / 1000:7.3f} s, khung #{so}")
            self.so_da_ve = (ts, so)
        elif a is None:
            self.canvas.itemconfigure(self.muc_anh, image="")
            self.canvas.itemconfigure(self.chu_cho, text="Chưa có luồng")
        if self.ui.song:
            self.root.after(100, self.cap_nhat)

    def dong(self):
        self.ui.song = False
        self.lop.dung()
        self.root.destroy()


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong_tcp = int(sys.argv[2]) if len(sys.argv) > 2 else 41652
    cong_udp = int(sys.argv[3]) if len(sys.argv) > 3 else cong_tcp + 1
    root = tk.Tk()
    cs = CuaSoServer(root, host, cong_tcp, cong_udp)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
