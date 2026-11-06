"""Thành phần giao diện Tkinter dùng chung cho nhóm bài tranh chấp MR6.6–MR6.13 (bài đầu: MR6.6 Đấu giá).

Tệp này GIỐNG NHAU ở cả 8 bài (chép nguyên). Gồm:
  - Tiện ích (như MR6.5): HangDoiGiaoDien, NhatKy, ThongKeDinhKy, nut, khung_nhom, nhan, o_nhap.
  - CuaSoServerCoSo: bảng điều khiển máy chủ đủ mọi phần chung – START/STOP, 4 nút chọn chế độ, độ trễ
    xử lý, chỉ số trực tiếp (yc/s, thành công, từ chối, xung đột, hàng đợi, độ trễ, SAI PHẠM tô đỏ),
    khung mô phỏng tải (Bắn đồng loạt, So sánh 4 chế độ + bảng), nhật ký. Lớp con (server_gui.py) chỉ
    vẽ khung "Tài nguyên":  _ve_tai_nguyen(khung), _cap_nhat_tai_nguyen(tt, tk), _tham_so_dat_lai().
  - CuaSoNguoiDungCoSo: khung cửa sổ người dùng – kết nối/rời, nhật ký, thanh trạng thái, gửi yêu cầu;
    lớp con (ví dụ dau_gia_gui.py) vẽ phần nghiệp vụ: _ve(khung), _cap_nhat(tt), _ket_qua(...).
  - chay_kich_ban, chup, ghep_anh: dùng cho demo_gui.py (bấm nút tự động, chụp cửa sổ dưới Xvfb).

Quy tắc: CHỈ luồng giao diện (luồng chạy mainloop) chạm vào điều khiển. Luồng mạng/mô phỏng đưa việc vào
HangDoiGiaoDien; các số liệu đọc định kỳ bằng root.after(...).
"""
import os
import queue
import threading
import time
import tkinter as tk
from tkinter import ttk

import chup_anh
from chung import CHE_DO, TEN_CHE_DO
from khach import KetNoiKhach

NEN = "#b0c4de"
XANH_LA, DO, XANH_DUONG, CAM, VANG = "#90ee90", "#f08080", "#add8e6", "#ffb347", "#ffd700"
DO_DAM, XANH_DAM = "#c00000", "#006400"


class HangDoiGiaoDien:
    """Cầu nối luồng nền -> luồng giao diện: dua_vao() gọi từ bất kỳ luồng nào."""

    def __init__(self, root, chu_ky_ms=50):
        self.root, self.chu_ky, self.hang, self.song = root, chu_ky_ms, queue.Queue(), True
        root.after(chu_ky_ms, self._doc)

    def dua_vao(self, ham, *tham_so):
        self.hang.put((ham, tham_so))

    def _doc(self):
        while True:
            try:
                ham, ts = self.hang.get_nowait()
            except queue.Empty:
                break
            try:
                ham(*ts)
            except tk.TclError:                        # cửa sổ đã đóng
                pass
        if self.song:
            self.root.after(self.chu_ky, self._doc)


class NhatKy(tk.Frame):
    """Ô nhật ký cuộn (chỉ đọc), mỗi dòng có giờ. in_ra = True (chạy riêng): cũng in ra dòng lệnh."""
    in_ra = False

    def __init__(self, cha, cao=6, rong=60, ten="", **kw):
        super().__init__(cha, **kw)
        self.ten = ten
        self.van_ban = tk.Text(self, height=cao, width=rong, state="disabled", wrap="word",
                               font=("TkFixedFont", 9), bg="white")
        cuon = tk.Scrollbar(self, command=self.van_ban.yview)
        self.van_ban.configure(yscrollcommand=cuon.set)
        self.van_ban.pack(side="left", fill="both", expand=True)
        cuon.pack(side="right", fill="y")

    def ghi(self, text):
        if NhatKy.in_ra:
            print(f"{time.strftime('%H:%M:%S')} [{self.ten}] {text}", flush=True)
        self.van_ban.configure(state="normal")
        self.van_ban.insert("end", time.strftime("%H:%M:%S ") + text + "\n")
        if int(self.van_ban.index("end-1c").split(".")[0]) > 500:
            self.van_ban.delete("1.0", "2.0")
        self.van_ban.see("end")
        self.van_ban.configure(state="disabled")


class ThongKeDinhKy:
    """Khi chạy riêng (NhatKy.in_ra): cứ `chu_ky` giây in một dòng số liệu ra dòng lệnh."""

    def __init__(self, ten, chu_ky=5.0):
        self.ten, self.chu_ky, self.han = ten, chu_ky, time.monotonic() + chu_ky

    def in_neu_den_han(self, tao_dong):
        bay_gio = time.monotonic()
        if not NhatKy.in_ra or bay_gio < self.han:
            return
        self.han = bay_gio + self.chu_ky
        print(f"{time.strftime('%H:%M:%S')} [{self.ten}] {tao_dong()}", flush=True)


def nut(cha, chu, mau, lenh, rong=12):
    return tk.Button(cha, text=chu, bg=mau, activebackground=mau, width=rong, command=lenh,
                     relief="raised", bd=1)


def khung_nhom(cha, tieu_de):
    return tk.LabelFrame(cha, text=tieu_de, bg=NEN, padx=6, pady=4, font=("TkDefaultFont", 9, "bold"))


def nhan(cha, chu="", **kw):
    kw.setdefault("bg", NEN)
    kw.setdefault("anchor", "w")
    return tk.Label(cha, text=chu, **kw)


def o_nhap(cha, gia_tri, rong):
    o = tk.Entry(cha, width=rong)
    o.insert(0, str(gia_tri))
    return o


def ms(us):
    return "–" if us is None else f"{us / 1000:.1f}"


# ===================================================================== bảng điều khiển máy chủ
class CuaSoServerCoSo:
    TIEU_DE = "Máy chủ"
    TEN_TAI_NGUYEN = "Tài nguyên"

    def __init__(self, root, lop_may_chu, host, cong, mo_phong, che_do="khoa", tre_ms=2.0, tham_so=None,
                 n=200, k=200, lan=1):
        self.root, self.mo_phong = root, mo_phong
        self.ui = HangDoiGiaoDien(root)
        self.lop = lop_may_chu(che_do, tre_ms, bao=lambda t: self.ui.dua_vao(self.nhat_ky.ghi, t),
                               **(tham_so or {}))
        self.dang_mo_phong = False
        self.hang_so_sanh = []
        root.title(f"{self.TIEU_DE} – bảng điều khiển")
        root.configure(bg=NEN)
        root.resizable(False, False)

        tren = tk.Frame(root, bg=NEN)
        tren.pack(fill="x", padx=6, pady=(6, 0))
        g = khung_nhom(tren, "Kết nối")
        g.pack(side="left", fill="y")
        nhan(g, "IP:").grid(row=0, column=0, sticky="w")
        self.o_ip = o_nhap(g, host, 13)
        self.o_ip.grid(row=0, column=1, sticky="w")
        nhan(g, "Cổng:").grid(row=1, column=0, sticky="w")
        self.o_cong = o_nhap(g, cong, 7)
        self.o_cong.grid(row=1, column=1, sticky="w")
        self.nut_start = nut(g, "START SERVER", XANH_LA, self.bat_dau)
        self.nut_start.grid(row=2, column=0, columnspan=2, pady=(6, 2), sticky="we")
        self.nut_stop = nut(g, "STOP SERVER", DO, self.dung)
        self.nut_stop.grid(row=3, column=0, columnspan=2, sticky="we")
        self.nut_stop.configure(state="disabled")

        g = khung_nhom(tren, "Cách xử lý tranh chấp (đổi được khi đang chạy)")
        g.pack(side="left", fill="y", padx=6)
        self.bien_che_do = tk.StringVar(value=che_do)
        self.nut_che_do = {}
        for cd in CHE_DO:
            r = tk.Radiobutton(g, text=f"{cd:<11} {TEN_CHE_DO[cd]}", value=cd, variable=self.bien_che_do,
                               bg=NEN, anchor="w", font=("TkFixedFont", 9), command=self.doi_che_do)
            r.pack(anchor="w")
            self.nut_che_do[cd] = r
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=(4, 0))
        nhan(f, "Độ trễ xử lý: ngẫu nhiên 0 –").pack(side="left")
        self.o_tre = o_nhap(f, f"{tre_ms:g}", 4)
        self.o_tre.pack(side="left")
        nhan(f, "ms").pack(side="left")
        self.nut_tre = nut(f, "Áp dụng", XANH_DUONG, self.doi_tre, 7)
        self.nut_tre.pack(side="left", padx=4)

        g = khung_nhom(tren, "Chỉ số trực tiếp (từ lần đặt lại)")
        g.pack(side="left", fill="both", expand=True)
        self.chi_so = {}
        dong = [("ket_noi", "Kết nối / người"), ("yc_s", "Yêu cầu/giây"), ("so_yc", "Tổng yêu cầu"),
                ("thanh_cong", "Thành công"), ("tu_choi", "Từ chối"), ("xung_dot", "Xung đột / thử lại"),
                ("hang_doi", "Hàng đợi (lớn nhất)"), ("tre", "Xử lý p50/p95/p99")]
        for i, (ma, chu) in enumerate(dong):
            nhan(g, chu + ":").grid(row=i, column=0, sticky="w")
            self.chi_so[ma] = nhan(g, "–", font=("TkDefaultFont", 9, "bold"), width=30)  # rộng cố định
            self.chi_so[ma].grid(row=i, column=1, sticky="w", padx=(4, 0))
        self.nhan_sai_pham = tk.Label(g, text="SAI PHẠM: –", font=("TkDefaultFont", 12, "bold"), bg="#e0e0e0",
                                      width=24, anchor="w", padx=4)
        self.nhan_sai_pham.grid(row=0, column=2, rowspan=2, sticky="nw", padx=(12, 0))
        self.nhan_sai_pham_ct = nhan(g, "", justify="left", wraplength=260, font=("TkDefaultFont", 8))
        self.nhan_sai_pham_ct.grid(row=2, column=2, rowspan=6, sticky="nw", padx=(12, 0))

        giua = tk.Frame(root, bg=NEN)
        giua.pack(fill="x", padx=6, pady=(4, 0))
        g = khung_nhom(giua, self.TEN_TAI_NGUYEN)
        g.pack(side="left", fill="y")
        self._ve_tai_nguyen(g)
        self.nut_dat_lai = nut(g, "Đặt lại", CAM, self.dat_lai, 10)
        self.nut_dat_lai.pack(anchor="e", pady=(4, 0))

        g = khung_nhom(giua, "Mô phỏng tải (n người ảo trên k kết nối, rào xuất phát)")
        g.pack(side="left", fill="both", expand=True, padx=(6, 0))
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w")
        nhan(f, "n:").pack(side="left")
        self.o_n = o_nhap(f, n, 6)
        self.o_n.pack(side="left")
        nhan(f, "  k:").pack(side="left")
        self.o_k = o_nhap(f, k, 5)
        self.o_k.pack(side="left")
        nhan(f, "  lần/người:").pack(side="left")
        self.o_lan = o_nhap(f, lan, 3)
        self.o_lan.pack(side="left")
        f = tk.Frame(g, bg=NEN)
        f.pack(anchor="w", pady=4)
        self.nut_ban = nut(f, "Bắn đồng loạt", CAM, self.ban, 14)
        self.nut_ban.pack(side="left")
        self.nut_so_sanh = nut(f, "So sánh 4 chế độ", XANH_DUONG, self.so_sanh, 16)
        self.nut_so_sanh.pack(side="left", padx=6)
        self.nhan_mo_phong = nhan(f, "")
        self.nhan_mo_phong.pack(side="left")
        cot = [("che_do", "Chế độ", 92), ("thanh_cong", "Thành công", 76), ("sai_pham", "Sai phạm", 66),
               ("xung_dot", "Xung đột", 66), ("yc_s", "yc/s", 56), ("p99", "p99 ms", 60)]
        self.bang = ttk.Treeview(g, columns=[c for c, _, _ in cot], show="headings", height=4)
        for c, t, w in cot:
            self.bang.heading(c, text=t)
            self.bang.column(c, width=w, anchor="w" if c == "che_do" else "e")
        self.bang.tag_configure("do", background="#ffc8c8", foreground=DO_DAM)
        self.bang.tag_configure("xanh", background="#d8f5d8")
        self.bang.pack(fill="x")
        self.nhan_ket_luan = nhan(g, "", justify="left", wraplength=470, font=("TkDefaultFont", 8))
        self.nhan_ket_luan.pack(anchor="w", pady=(2, 0))

        g = khung_nhom(root, "Nhật ký")
        g.pack(fill="x", padx=6, pady=4)
        self.nhat_ky = NhatKy(g, cao=6, rong=120, ten="server")
        self.nhat_ky.pack(fill="x")
        self.dinh_ky = ThongKeDinhKy("server")
        self.thanh_tt = tk.Label(root, text="Trạng thái: máy chủ chưa chạy", anchor="w", relief="sunken",
                                 bg="white", width=1)          # width=1 + fill: chữ dài không nới cửa sổ
        self.thanh_tt.pack(fill="x", padx=6, pady=(0, 6))
        root.after(250, self.cap_nhat)

    # ------------------------------------------------------------ lớp con viết
    def _ve_tai_nguyen(self, khung):
        pass

    def _cap_nhat_tai_nguyen(self, tt, tk_):
        pass

    def _tham_so_dat_lai(self):
        return {}

    # ------------------------------------------------------------ nút
    def bat_dau(self):
        try:
            self.lop.bat_dau(self.o_ip.get().strip(), int(self.o_cong.get()))
        except (OSError, ValueError) as e:
            self.nhat_ky.ghi(f"Không khởi động được: {e}")
            return
        self.nut_start.configure(state="disabled")
        self.nut_stop.configure(state="normal")
        for o in (self.o_ip, self.o_cong):
            o.configure(state="disabled")

    def dung(self):
        self.nut_stop.configure(state="disabled")

        def nen():                                         # chờ luồng nền dừng: không chặn giao diện
            self.lop.dung()
            self.ui.dua_vao(self._da_dung)
        threading.Thread(target=nen, daemon=True).start()

    def _da_dung(self):
        self.nut_start.configure(state="normal")
        for o in (self.o_ip, self.o_cong):
            o.configure(state="normal")

    def doi_che_do(self):
        cd = self.bien_che_do.get()
        threading.Thread(target=self.lop.dat_che_do, args=(cd,), daemon=True).start()   # có thể chờ hàng đợi cạn

    def chon_che_do(self, cd):
        """Chọn chế độ như người dùng bấm nút (kịch bản demo)."""
        self.nut_che_do[cd].invoke()

    def doi_tre(self):
        try:
            self.lop.dat_tre(float(self.o_tre.get()))
        except ValueError as e:
            self.nhat_ky.ghi(f"Độ trễ không hợp lệ: {e}")

    def dat_lai(self):
        try:
            self.lop.dat_lai(**self._tham_so_dat_lai())
        except ValueError as e:
            self.nhat_ky.ghi(f"Không đặt lại được: {e}")

    def _doc_mo_phong(self):
        n, k, lan = int(self.o_n.get()), int(self.o_k.get()), int(self.o_lan.get())
        if n < 1 or k < 1 or lan < 1:
            raise ValueError("n, k, lần ≥ 1")
        return n, min(k, n), lan

    def _chay_nen(self, ham, *ts):
        """Chạy mô phỏng trên luồng nền (kết nối TCP tới chính máy chủ này như client bên ngoài)."""
        if self.dang_mo_phong or not self.lop.chay:
            self.nhat_ky.ghi("Cần START SERVER và chờ lượt mô phỏng trước xong")
            return
        self.dang_mo_phong = True
        for b in (self.nut_ban, self.nut_so_sanh):
            b.configure(state="disabled")
        self.nhan_mo_phong.configure(text="đang chạy…", fg=DO_DAM)
        host, cong = self.lop.dia_chi.rsplit(":", 1)

        def bao(text):
            self.ui.dua_vao(self.nhat_ky.ghi, "[mô phỏng] " + text)

        def nen():
            try:
                kq = ham(host, int(cong), *ts, bao)
            except Exception as e:
                bao(f"lỗi: {e}")
                kq = None
            self.ui.dua_vao(self._xong_mo_phong, kq)
        threading.Thread(target=nen, daemon=True).start()

    def ban(self):
        n, k, lan = self._doc_mo_phong()
        self._chay_nen(lambda h, c, b: [x for x in [self.mo_phong.chay_luot(h, c, n, k, lan, b)] if x])

    def so_sanh(self):
        n, k, lan = self._doc_mo_phong()
        self._chay_nen(self.mo_phong.so_sanh, n, k, lan)

    def _xong_mo_phong(self, hang):
        self.dang_mo_phong = False
        for b in (self.nut_ban, self.nut_so_sanh):
            b.configure(state="normal")
        self.nhan_mo_phong.configure(text="xong", fg=XANH_DAM)
        if not hang:
            return
        if len(hang) == len(CHE_DO):
            self.hang_so_sanh = hang
            for iid in self.bang.get_children():
                self.bang.delete(iid)
        for h in hang:
            gt = (h["che_do"], h["thanh_cong"], h["sai_pham"], h["xung_dot"], h["yc_s"],
                  "–" if h["p99_ms"] is None else f"{h['p99_ms']:.1f}")
            if self.bang.exists(h["che_do"]):
                self.bang.item(h["che_do"], values=gt, tags=("do" if h["sai_pham"] else "xanh",))
            else:
                self.bang.insert("", "end", iid=h["che_do"], values=gt, tags=("do" if h["sai_pham"] else "xanh",))
        sai = [h["che_do"] for h in hang if h["sai_pham"]]
        dung = [h["che_do"] for h in hang if not h["sai_pham"]]
        self.nhan_ket_luan.configure(
            text=(f"Sai phạm: {', '.join(sai)}. " if sai else "") + (f"Không sai phạm: {', '.join(dung)}." if dung else ""))

    # ------------------------------------------------------------ vẽ lại định kỳ (250 ms)
    def cap_nhat(self):
        if self.lop.chay:
            tk_ = self.lop.thong_ke_day_du()
            tt = self.lop.trang_thai()
            c = self.chi_so
            c["ket_noi"].configure(text=f"{tk_['so_ket_noi']} / {tk_['so_nguoi']}")
            c["yc_s"].configure(text=f"{tk_['yc_s']}  (TB {tk_['thong_luong']})")
            c["so_yc"].configure(text=str(tk_["so_yc"]))
            c["thanh_cong"].configure(text=str(tk_["thanh_cong"]))
            c["tu_choi"].configure(text=str(tk_["tu_choi"]) + (
                "  (" + ", ".join(f"{m} {s}" for m, s in sorted(tk_["ma_loi"].items())) + ")" if tk_["ma_loi"] else ""))
            c["xung_dot"].configure(text=f"{tk_['xung_dot']} / {tk_['thu_lai']} (hết lượt {tk_['het_lan_thu']})")
            c["hang_doi"].configure(text=f"{tk_['hang_doi']}  ({tk_['hang_doi_max']})")
            c["tre"].configure(text=f"{ms(tk_['p50_us'])} / {ms(tk_['p95_us'])} / {ms(tk_['p99_us'])} ms")
            sp = tk_["sai_pham_tong"]
            self.nhan_sai_pham.configure(text=f"SAI PHẠM: {sp}", bg="#ff6060" if sp else "#70d070",
                                         fg="white" if sp else "black")
            self.nhan_sai_pham_ct.configure(
                text="\n".join([", ".join(f"{s['ten']} {s['so']}" for s in tk_["sai_pham"])]
                               + [f"• {v}" for v in tk_["vi_du_sai_pham"][:2]]))
            if self.bien_che_do.get() != tk_["che_do"] and not self.dang_mo_phong:
                self.bien_che_do.set(tk_["che_do"])           # mô phỏng/quản trị đổi chế độ từ xa
            self._cap_nhat_tai_nguyen(tt, tk_)
            self.thanh_tt.configure(text=f"Trạng thái: máy chủ chạy tại {self.lop.dia_chi} – chế độ"
                                         f" {tk_['che_do']}, {tk_['so_ket_noi']} kết nối – {tk_['tom_tat']}")
            self.dinh_ky.in_neu_den_han(lambda: f"{tk_['che_do']}: {tk_['so_yc']} yc, tc {tk_['thanh_cong']},"
                                                f" {tk_['yc_s']} yc/s, sai phạm {sp}; {tk_['tom_tat']}")
        else:
            self.thanh_tt.configure(text="Trạng thái: máy chủ chưa chạy")
        if self.ui.song:
            self.root.after(250, self.cap_nhat)

    def dong(self):
        self.ui.song = False
        self.lop.dung()
        self.root.destroy()


# ===================================================================== cửa sổ người dùng
class CuaSoNguoiDungCoSo:
    """Kết nối/rời, nhật ký, thanh trạng thái; lớp con vẽ phần nghiệp vụ trong self.khung."""
    TIEU_DE = "Ứng dụng"

    def __init__(self, root, ten, host, cong):
        self.root, self.ten = root, ten
        self.ui = HangDoiGiaoDien(root)
        self.kn = None
        self.tt = None                                     # cap_nhat mới nhất
        self.rtt_cuoi = None
        root.title(f"{self.TIEU_DE} – {ten}")
        root.configure(bg=NEN)
        root.resizable(False, False)
        f = tk.Frame(root, bg=NEN)
        f.pack(fill="x", padx=6, pady=(6, 2))
        nhan(f, "Tên:").pack(side="left")
        self.o_ten = o_nhap(f, ten, 8)
        self.o_ten.pack(side="left")
        nhan(f, " IP:").pack(side="left")
        self.o_ip = o_nhap(f, host, 11)
        self.o_ip.pack(side="left")
        nhan(f, " Cổng:").pack(side="left")
        self.o_cong = o_nhap(f, cong, 6)
        self.o_cong.pack(side="left")
        self.nut_ket_noi = nut(f, "Kết nối", XANH_LA, self.ket_noi, 8)
        self.nut_ket_noi.pack(side="left", padx=(6, 2))
        self.nut_roi = nut(f, "Rời", DO, self.roi, 5)
        self.nut_roi.pack(side="left")
        self.nut_roi.configure(state="disabled")
        self.khung = tk.Frame(root, bg=NEN)
        self.khung.pack(fill="both", expand=True, padx=6)
        self._ve(self.khung)
        g = khung_nhom(root, "Nhật ký")
        g.pack(fill="x", padx=6, pady=(2, 2))
        self.nhat_ky = NhatKy(g, cao=4, rong=56, ten=ten)
        self.nhat_ky.pack(fill="x")
        self.dinh_ky = ThongKeDinhKy(ten)
        self.thanh_tt = tk.Label(root, text="Chưa kết nối", anchor="w", relief="sunken", bg="white", width=1)
        self.thanh_tt.pack(fill="x", padx=6, pady=(0, 6))
        root.after(200, self._nhip)

    # ------------------------------------------------------------ lớp con viết
    def _ve(self, khung):
        pass

    def _cap_nhat(self, tt):
        pass

    def _ket_qua(self, loai, truong, tin, rtt):
        pass

    def _su_kien(self, text):
        self.nhat_ky.ghi(f"* {text}")

    def _moi_nhip(self):
        """Gọi mỗi 200 ms trên luồng giao diện (đồng hồ đếm ngược…)."""

    def _dong_dinh_ky(self):
        return ""

    # ------------------------------------------------------------ kết nối
    def ket_noi(self):
        ten, host = self.o_ten.get().strip(), self.o_ip.get().strip()
        try:
            cong = int(self.o_cong.get())
        except ValueError:
            self.nhat_ky.ghi("Cổng không hợp lệ")
            return
        self.ten = ten
        self.nut_ket_noi.configure(state="disabled")
        kn = KetNoiKhach(khi_cap_nhat=lambda tin: self.ui.dua_vao(self._nhan_cap_nhat, tin),
                         khi_tin=lambda tin: self.ui.dua_vao(self._nhan_tin, tin),
                         khi_dong=lambda: self.ui.dua_vao(self._mat_ket_noi, kn))

        def nen():                                     # kết nối trên luồng nền: không đơ giao diện
            try:
                kn.ket_noi(host, cong, ten)
            except (OSError, ConnectionError, ValueError) as e:
                self.ui.dua_vao(self._khong_ket_noi, str(e))
                return
            self.ui.dua_vao(self._da_ket_noi, kn)
        threading.Thread(target=nen, daemon=True).start()

    def _khong_ket_noi(self, loi):
        self.nhat_ky.ghi(f"Không kết nối được: {loi}")
        self.nut_ket_noi.configure(state="normal")

    def _da_ket_noi(self, kn):
        self.kn = kn
        self.nut_roi.configure(state="normal")
        for o in (self.o_ten, self.o_ip, self.o_cong):
            o.configure(state="disabled")
        self.nhat_ky.ghi(f"Đã kết nối {kn.chao['may_chu']} (chế độ {kn.chao['che_do']})")
        self._nhan_cap_nhat(kn.trang_thai)
        kn.bat_dau_doc()

    def roi(self):
        if self.kn:
            kn, self.kn = self.kn, None
            kn.khi_dong = lambda: None
            kn.dong()
            self._mat_ket_noi(None, "Đã rời")

    def _mat_ket_noi(self, kn, ly_do="Mất kết nối máy chủ"):
        if kn is not None and kn is not self.kn:
            return
        self.kn = None
        self.nhat_ky.ghi(ly_do)
        self.thanh_tt.configure(text=ly_do)
        self.nut_ket_noi.configure(state="normal")
        self.nut_roi.configure(state="disabled")
        for o in (self.o_ten, self.o_ip, self.o_cong):
            o.configure(state="normal")

    def _nhan_cap_nhat(self, tt):
        self.tt = tt
        self._cap_nhat(tt)
        self._cap_nhat_thanh_tt()

    def _nhan_tin(self, tin):
        if tin.get("loai") == "su_kien":
            self._su_kien(tin["noi_dung"])
        elif tin.get("loai") == "loi":
            self.nhat_ky.ghi(f"Lỗi: {tin.get('ly_do')}")

    def _cap_nhat_thanh_tt(self):
        if self.kn and self.tt:
            rtt = "" if self.rtt_cuoi is None else f", khứ hồi {self.rtt_cuoi:.1f} ms"
            self.thanh_tt.configure(text=f"Kết nối {self.o_ip.get()}:{self.o_cong.get()} – máy chủ chế độ"
                                         f" {self.tt['che_do']}{rtt}")

    # ------------------------------------------------------------ gửi yêu cầu
    def gui_yc(self, loai, **truong):
        if not self.kn:
            self.nhat_ky.ghi("Chưa kết nối")
            return

        def kq(tin, rtt):                                # luồng đọc -> chuyển sang luồng giao diện
            self.ui.dua_vao(self._nhan_kq, loai, truong, tin, rtt)
        try:
            self.kn.gui_yc(loai, kq, **truong)
        except OSError as e:
            self.nhat_ky.ghi(f"Gửi lỗi: {e}")

    def _nhan_kq(self, loai, truong, tin, rtt):
        self.rtt_cuoi = rtt
        self._ket_qua(loai, truong, tin, rtt)
        self._cap_nhat_thanh_tt()

    def _nhip(self):
        self._moi_nhip()
        if self.kn:
            self.dinh_ky.in_neu_den_han(self._dong_dinh_ky)
        if self.ui.song:
            self.root.after(200, self._nhip)

    def dong(self):
        self.ui.song = False
        if self.kn:
            self.kn.khi_dong = lambda: None
            self.kn.dong()
        self.root.destroy()


# ===================================================================== kịch bản demo (Xvfb)
def chay_kich_ban(root, gen, bao, het_gio=30.0):
    """Bộ điều phối kịch bản: `yield giây` = chờ; `yield hàm` = chờ tới khi hàm() đúng (tối đa het_gio).
    Mọi bước chạy trên luồng giao diện (root.after) nên kịch bản bấm nút như người dùng."""
    def buoc():
        try:
            v = next(gen)
        except StopIteration:
            return
        if callable(v):
            cho(v, het_gio)
        else:
            root.after(int(v * 1000), buoc)

    def cho(dk, con_lai):
        if dk():
            buoc()
        elif con_lai <= 0:
            bao("Kịch bản chờ quá lâu – dừng")
            root.destroy()
        else:
            root.after(50, cho, dk, con_lai - 0.05)
    root.after(0, buoc)


def chup(cua_so, duong):
    """Chụp một cửa sổ Tk (cả khung viền): ImageMagick `import` trên Linux, Pillow trên Windows (chup_anh.py)."""
    cua_so.update()
    chup_anh.chup(cua_so, duong)


def ghep_anh(hang, ra):
    """hang: danh sách các hàng, mỗi hàng là danh sách tệp ảnh -> ghép thành một ảnh (như convert)."""
    lenh = []
    for h in hang:
        lenh += ["(", *h, "-background", "#d0d0d0", "-gravity", "north", "+append", ")"]
    lenh += ["-background", "#d0d0d0", "-gravity", "west", "-append", "-bordercolor", "#d0d0d0",
             "-border", "4", ra]
    chup_anh.convert(lenh)
    for h in hang:
        for f in h:
            os.remove(f)
