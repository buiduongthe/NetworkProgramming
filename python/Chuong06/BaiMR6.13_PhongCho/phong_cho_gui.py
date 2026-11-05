"""Bài mở rộng MR6.13 – Trang bán vé có phòng chờ ảo (ứng dụng người dùng, Tkinter).

Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở giao_dien_chung.CuaSoNguoiDungCoSo;
tệp này vẽ ba "trang" như một trang bán vé thật:
  1. Sự kiện: áp phích concert (vẽ bằng Canvas), giá vé, thanh "vé còn lại", nút "VÀO HÀNG CHỜ MUA VÉ"
     (phòng chờ tắt: "MUA VÉ NGAY");
  2. Phòng chờ: số thứ tự, THANH TIẾN TRÌNH vị trí, "Còn khoảng x người phía trước – chờ ~y s", số đang
     phục vụ, tốc độ cấp lượt; cập nhật trực tiếp theo cap_nhat (≤ 10 lần/giây);
  3. Chọn vé: "ĐẾN LƯỢT BẠN!", ĐỒNG HỒ đếm ngược thời hạn lượt (20 s, đỏ khi < 5 s), chọn số vé, thành
     tiền, nút "THANH TOÁN", kết quả ("Mua thành công…", "Hết vé", "Lượt đã hết hạn", "Hệ thống quá tải").
Tới lượt được báo bằng trả lời của yêu cầu "chờ dài" cho_luot (máy chủ giữ yêu cầu tới khi cấp lượt).

Chạy:  python3 phong_cho_gui.py <tên> [host] [cổng]       (mặc định 127.0.0.1 41772)
"""
import random
import sys
import time
import tkinter as tk
from tkinter import ttk

from chung import tien
from giao_dien_chung import DO_DAM, XANH_DAM, CuaSoNguoiDungCoSo, NhatKy, nut

TIM, HONG = "#3b0a5e", "#e0407a"


def _tron(c1, c2, t):
    a = [int(c1[i:i + 2], 16) for i in (1, 3, 5)]
    b = [int(c2[i:i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{round(x + (y - x) * t):02x}" for x, y in zip(a, b))


def ve_ap_phich(cv, ten, phu="20:00 · 20.12.2026 · Sân khấu ngoài trời"):
    """Áp phích concert: nền chuyển màu, đèn sân khấu, đám đông, tên sự kiện."""
    w, h = int(cv["width"]), int(cv["height"])
    cv.delete("all")
    for y in range(h):
        cv.create_line(0, y, w, y, fill=_tron("#12002a", "#6a1b6e", y / h))
    rnd = random.Random(7)
    for _ in range(40):                                  # sao
        x, y = rnd.randint(0, w), rnd.randint(0, h // 2)
        cv.create_oval(x, y, x + 2, y + 2, fill="#fff4c0", outline="")
    for x0, mau in ((w * 0.18, "#ffd36e"), (w * 0.5, "#7fd6ff"), (w * 0.82, "#ff8fc0")):   # đèn chiếu
        cv.create_polygon(x0, h * 0.62, x0 - 70, -10, x0 + 70, -10, fill=mau, stipple="gray25", outline="")
        cv.create_oval(x0 - 7, h * 0.62 - 7, x0 + 7, h * 0.62 + 7, fill=mau, outline="")
    cv.create_rectangle(0, h * 0.66, w, h * 0.72, fill="#2a0f3a", outline="")             # sân khấu
    for i in range(int(w // 14) + 1):                    # đám đông (bóng đầu, tay giơ)
        x = i * 14 + rnd.randint(-3, 3)
        y = h - 18 + rnd.randint(-4, 4)
        cv.create_oval(x - 7, y - 7, x + 7, y + 7, fill="#0a0012", outline="")
        cv.create_rectangle(x - 9, y + 4, x + 9, h, fill="#0a0012", outline="")
        if rnd.random() < 0.3:
            cv.create_line(x + 5, y, x + 10, y - 20, fill="#0a0012", width=4)
    cv.create_text(20, 30, text=ten.upper(), anchor="w", fill="white", font=("TkDefaultFont", 18, "bold"))
    cv.create_text(22, 58, text=phu, anchor="w", fill="#ffd9f0", font=("TkDefaultFont", 10))
    cv.create_rectangle(w - 120, 14, w - 14, 40, fill=HONG, outline="")
    cv.create_text(w - 67, 27, text="MỞ BÁN", fill="white", font=("TkDefaultFont", 10, "bold"))


def dong_ho(giay):
    giay = max(0.0, giay)
    return f"00:{int(giay):02d}" if giay >= 10 else f"00:{giay:04.1f}"


class CuaSoPhongCho(CuaSoNguoiDungCoSo):
    TIEU_DE = "Bán vé concert"

    def _ve(self, khung):
        self.trang = None
        self.so = None                                   # số thứ tự trong hàng
        self.truoc_dau = None                            # số người phía trước lúc vào hàng
        self.han = None                                  # hạn lượt (monotonic); None: không giới hạn
        self.co_luot = False
        self.so_kq = self.so_don = 0
        self.cv = tk.Canvas(khung, width=540, height=150, highlightthickness=0)
        self.cv.pack(pady=(4, 0))
        ve_ap_phich(self.cv, "Đêm nhạc")
        self.hop = tk.Frame(khung, bg="white", width=540, height=250, highlightthickness=1,
                            highlightbackground="#9090b0")
        self.hop.pack(pady=(0, 4))
        self.hop.pack_propagate(False)
        self.trang_sk = self._ve_su_kien(tk.Frame(self.hop, bg="white"))
        self.trang_cho = self._ve_cho(tk.Frame(self.hop, bg="white"))
        self.trang_mua = self._ve_mua(tk.Frame(self.hop, bg="white"))
        self._sang(self.trang_sk)

    # ------------------------------------------------------------ ba trang
    def _ve_su_kien(self, f):
        self.nhan_gia = tk.Label(f, text="–", bg="white", font=("TkDefaultFont", 11, "bold"), fg=TIM)
        self.nhan_gia.pack(anchor="w", padx=12, pady=(12, 2))
        self.nhan_con_sk = tk.Label(f, text="Vé còn lại: –", bg="white", font=("TkDefaultFont", 10))
        self.nhan_con_sk.pack(anchor="w", padx=12)
        self.thanh_con_sk = ttk.Progressbar(f, length=510, maximum=500)
        self.thanh_con_sk.pack(anchor="w", padx=12, pady=(2, 8))
        self.nhan_pc = tk.Label(f, text="", bg="white", font=("TkDefaultFont", 9), fg="#404040", wraplength=510,
                                justify="left")
        self.nhan_pc.pack(anchor="w", padx=12)
        self.nut_vao = tk.Button(f, text="VÀO HÀNG CHỜ MUA VÉ", bg=HONG, fg="white", activebackground=HONG,
                                 font=("TkDefaultFont", 13, "bold"), width=26, command=self.vao_hang)
        self.nut_vao.pack(pady=12)
        self.nhan_kq_sk = tk.Label(f, text="", bg="white", font=("TkDefaultFont", 10, "bold"), fg=DO_DAM)
        self.nhan_kq_sk.pack()
        self.nut_vao.configure(state="disabled")
        return f

    def _ve_cho(self, f):
        tk.Label(f, text="Bạn đang ở trong phòng chờ", bg="white", font=("TkDefaultFont", 13, "bold"),
                 fg=TIM).pack(anchor="w", padx=12, pady=(12, 0))
        self.nhan_so = tk.Label(f, text="Số thứ tự của bạn: –", bg="white", font=("TkDefaultFont", 11))
        self.nhan_so.pack(anchor="w", padx=12, pady=(2, 6))
        self.thanh_cho = ttk.Progressbar(f, length=510, maximum=1000)
        self.thanh_cho.pack(anchor="w", padx=12)
        self.nhan_truoc = tk.Label(f, text="–", bg="white", font=("TkDefaultFont", 12, "bold"))
        self.nhan_truoc.pack(anchor="w", padx=12, pady=(6, 2))
        self.nhan_phuc_vu = tk.Label(f, text="", bg="white", font=("TkDefaultFont", 9), fg="#404040")
        self.nhan_phuc_vu.pack(anchor="w", padx=12)
        self.nhan_con_cho = tk.Label(f, text="", bg="white", font=("TkDefaultFont", 9), fg="#404040")
        self.nhan_con_cho.pack(anchor="w", padx=12)
        tk.Label(f, text="Vui lòng không tải lại trang – bạn vẫn giữ nguyên chỗ trong hàng.", bg="#fff3c4",
                 font=("TkDefaultFont", 9)).pack(anchor="w", padx=12, pady=(10, 0), fill="x")
        return f

    def _ve_mua(self, f):
        self.bang_luot = tk.Label(f, text="ĐẾN LƯỢT BẠN!", bg="#2e9e4f", fg="white",
                                  font=("TkDefaultFont", 12, "bold"), pady=4)
        self.bang_luot.pack(fill="x")
        g = tk.Frame(f, bg="white")
        g.pack(fill="x", padx=12, pady=(6, 0))
        tk.Label(g, text="Thời gian còn lại để hoàn tất:", bg="white").pack(side="left")
        self.nhan_dong_ho = tk.Label(g, text="–", bg="white", font=("TkFixedFont", 24, "bold"), fg=XANH_DAM)
        self.nhan_dong_ho.pack(side="left", padx=8)
        g = tk.Frame(f, bg="white")
        g.pack(fill="x", padx=12, pady=4)
        tk.Label(g, text="Số vé:", bg="white", font=("TkDefaultFont", 11)).pack(side="left")
        self.bien_so_ve = tk.IntVar(value=2)
        self.o_so_ve = tk.Spinbox(g, from_=1, to=4, width=3, textvariable=self.bien_so_ve,
                                  font=("TkDefaultFont", 12, "bold"), command=self._tinh_tien)
        self.o_so_ve.pack(side="left", padx=6)
        self.nhan_tien = tk.Label(g, text="", bg="white", font=("TkDefaultFont", 10))
        self.nhan_tien.pack(side="left", padx=8)
        self.nut_mua = tk.Button(f, text="THANH TOÁN", bg="#ff8c00", fg="white", activebackground="#ff8c00",
                                 font=("TkDefaultFont", 12, "bold"), width=20, command=self.thanh_toan)
        self.nut_mua.pack(pady=6)
        self.nhan_kq = tk.Label(f, text="", bg="white", font=("TkDefaultFont", 11, "bold"), wraplength=510)
        self.nhan_kq.pack()
        self.nut_ve = nut(f, "Về trang sự kiện", "#e8eef8", self.ve_su_kien, 16)
        return f

    def _sang(self, trang):
        for t in (self.trang_sk, self.trang_cho, self.trang_mua):
            t.pack_forget()
        trang.pack(fill="both", expand=True)
        self.trang = trang

    # ------------------------------------------------------------ thao tác
    def vao_hang(self):
        self.nhan_kq_sk.configure(text="")
        if not self.tt["phong_cho"]:                    # phòng chờ tắt: vào thẳng trang mua
            self._mo_trang_mua(None)
            return
        self.nut_vao.configure(state="disabled")
        self.nhat_ky.ghi("Vào phòng chờ…")
        self.gui_yc("xep_hang")

    def _mo_trang_mua(self, han_ms):
        self.co_luot = True
        self.han = None if han_ms is None else time.monotonic() + han_ms / 1000
        self.bang_luot.configure(text="ĐẾN LƯỢT BẠN! Hãy hoàn tất trước khi hết giờ" if han_ms is not None
                                 else "MUA TRỰC TIẾP (phòng chờ tắt)", bg="#2e9e4f" if han_ms is not None else TIM)
        self.nhan_kq.configure(text="")
        self.nut_mua.configure(state="normal")
        self.nut_ve.pack_forget()
        self._tinh_tien()
        self._sang(self.trang_mua)

    def thanh_toan(self):
        n = self.bien_so_ve.get()
        self.nhat_ky.ghi(f"Thanh toán {n} vé…")
        self.nhan_kq.configure(text="Đang xử lý thanh toán…", fg="#404040")
        self.gui_yc("mua", so_ve=n)

    def ve_su_kien(self):
        self.so = self.truoc_dau = self.han = None
        self.co_luot = False
        self.nut_vao.configure(state="normal" if self.tt and not self.tt["het_ve"] else "disabled")
        self._sang(self.trang_sk)

    def _tinh_tien(self):
        if self.tt:
            n, gia = self.bien_so_ve.get(), self.tt["su_kien"]["gia_ve"]
            self.nhan_tien.configure(text=f"× {tien(gia)} = {tien(n * gia)}")

    def _ket_qua(self, loai, truong, tin, rtt):
        self.so_kq += 1
        if loai == "xep_hang":
            if not tin.get("ok"):
                self.nhat_ky.ghi(f"  không vào được hàng: {tin.get('ly_do')}")
                self.nhan_kq_sk.configure(text=f"Rất tiếc: {tin.get('ly_do')}")
                self.nut_vao.configure(state="normal")
                return
            self.so, self.truoc_dau = tin["so"], max(1, tin["truoc"])
            self.nhat_ky.ghi(f"  số thứ tự #{tin['so']}, {tin['truoc']} người phía trước, ~{tin['cho_s']:.1f} s")
            self._sang(self.trang_cho)
            self._ve_vi_tri()
            self.gui_yc("cho_luot")                     # chờ dài: máy chủ trả lời khi tới lượt
        elif loai == "cho_luot":
            if tin.get("ok"):
                self.nhat_ky.ghi(f"  ĐẾN LƯỢT sau {rtt / 1000:.1f} s chờ, còn {tin['han_ms'] / 1000:.0f} s để mua")
                self._mo_trang_mua(tin["han_ms"])
            else:
                self.nhat_ky.ghi(f"  {tin.get('ly_do')}")
                self.ve_su_kien()
                self.nhan_kq_sk.configure(text=f"Rất tiếc: {tin.get('ly_do')}")
        elif tin.get("ok"):
            self.so_don += 1
            self.nhat_ky.ghi(f"  MUA THÀNH CÔNG ({rtt:.1f} ms): đơn #{tin['ma_don']}, {tin['so_ve']} vé")
            self.nhan_kq.configure(text=f"Mua thành công {tin['so_ve']} vé – đơn #{tin['ma_don']},"
                                        f" {tien(tin['thanh_tien'])}", fg=XANH_DAM)
            self.nut_mua.configure(state="disabled")
            self.han, self.co_luot = None, False
            self.nhan_dong_ho.configure(text="✓", fg=XANH_DAM)
            self.nut_ve.pack(pady=(4, 0))
        else:
            ma = tin.get("ma_loi")
            self.nhat_ky.ghi(f"  từ chối ({rtt:.1f} ms): {tin.get('ly_do')}")
            text = {"qua_tai": "Hệ thống đang quá tải, vui lòng thử lại",
                    "het_ve": "Rất tiếc, đã bán hết vé", "da_mua": "Lượt này đã được dùng để mua"}.get(
                ma, tin.get("ly_do", ""))
            self.nhan_kq.configure(text=text, fg=DO_DAM)
            if ma in ("het_ve", "het_luot", "da_mua"):
                self.nut_mua.configure(state="disabled")
                self.nut_ve.pack(pady=(4, 0))

    def _ve_vi_tri(self):
        tt = self.tt
        if self.so is None or not tt:
            return
        truoc = max(0, self.so - tt["dang_phuc_vu"] - 1)
        self.nhan_so.configure(text=f"Số thứ tự của bạn: #{self.so}")
        self.thanh_cho["value"] = 1000 * (1 - truoc / max(self.truoc_dau, 1))
        self.nhan_truoc.configure(text=f"Còn khoảng {truoc} người phía trước – chờ ~{(truoc + 1) / tt['toc_do']:.0f} s"
                                  if truoc else "Sắp tới lượt bạn…")
        self.nhan_phuc_vu.configure(text=f"Đang phục vụ tới số #{tt['dang_phuc_vu']} · cấp lượt {tt['toc_do']:g}"
                                         f" người/s · {tt['dang_cho']} người đang chờ")
        self.nhan_con_cho.configure(text=f"Vé còn lại: {tt['con_lai']} / {tt['su_kien']['tong_ve']}")

    # ------------------------------------------------------------ nhận trạng thái
    def _cap_nhat(self, tt):
        sk = tt["su_kien"]
        if getattr(self, "_ten_cu", None) != sk["ten"]:
            self._ten_cu = sk["ten"]
            ve_ap_phich(self.cv, sk["ten"])
        self.nhan_gia.configure(text=f"Giá vé {tien(sk['gia_ve'])} · tối đa {sk['ve_toi_da']} vé/người")
        self.nhan_con_sk.configure(text=f"Vé còn lại: {tt['con_lai']} / {sk['tong_ve']}"
                                        + ("  – ĐÃ BÁN HẾT" if tt["het_ve"] else ""))
        self.thanh_con_sk.configure(maximum=sk["tong_ve"], value=max(0, tt["con_lai"]))
        self.nhan_pc.configure(
            text=(f"Phòng chờ ảo BẬT: mọi người được xếp hàng theo thứ tự đến, mỗi giây cấp lượt cho"
                  f" {tt['toc_do']:g} người; mỗi lượt có {sk['luot_s']:g} s để mua.") if tt["phong_cho"] else
            "Phòng chờ TẮT: ai cũng vào mua trực tiếp cùng lúc.")
        self.nut_vao.configure(text="VÀO HÀNG CHỜ MUA VÉ" if tt["phong_cho"] else "MUA VÉ NGAY")
        self.o_so_ve.configure(to=sk["ve_toi_da"])
        if self.trang is self.trang_sk:
            self.nut_vao.configure(state="disabled" if tt["het_ve"] else "normal")
        self._ve_vi_tri()

    def _moi_nhip(self):
        if self.trang is not self.trang_mua or not self.co_luot:
            return
        if self.han is None:
            self.nhan_dong_ho.configure(text="∞", fg=XANH_DAM)
            return
        con = self.han - time.monotonic()
        self.nhan_dong_ho.configure(text=dong_ho(con), fg=DO_DAM if con < 5 else XANH_DAM)
        if con <= 0:
            self.co_luot = False
            self.nut_mua.configure(state="disabled")
            self.nhan_kq.configure(text="Hết thời gian giữ lượt – bạn cần xếp hàng lại", fg=DO_DAM)
            self.nut_ve.pack(pady=(4, 0))
            self.nhat_ky.ghi("Lượt mua đã hết hạn")

    def _dong_dinh_ky(self):
        tt = self.tt
        trang = {id(self.trang_sk): "sự kiện", id(self.trang_cho): "phòng chờ", id(self.trang_mua): "mua"}
        vi_tri = "" if self.so is None else f", số #{self.so}, phục vụ #{tt['dang_phuc_vu']}"
        return f"trang {trang[id(self.trang)]}{vi_tri}; vé còn {tt['con_lai']}; đơn của bạn {self.so_don}"


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "An"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41772
    root = tk.Tk()
    cs = CuaSoPhongCho(root, ten, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
