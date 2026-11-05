"""Bài mở rộng MR6.10 – Cổng đăng ký học phần (ứng dụng người dùng, Tkinter).

Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở giao_dien_chung.CuaSoNguoiDungCoSo;
tệp này vẽ trang đăng ký như cổng đào tạo thật: thanh tiêu đề, thông tin sinh viên (số lớp, số tín chỉ),
bảng lớp học phần (mã lớp, tên, tín chỉ, giảng viên, lịch, thanh "đã đăng ký/sĩ số", nút Đăng ký / Hủy;
lớp đủ hiện "Đã đủ"), dòng thông báo ("Đăng ký thành công", "Lớp đã đủ", "Bạn đã đăng ký lớp này") và
bảng "Kết quả đăng ký". Số liệu đến từ cap_nhat của máy chủ (≤ 10 lần/giây).

Nếu máy chủ đã trả "thành công" mà danh sách lớp sau đó không có tên mình (mất cập nhật ở chế độ
khong_khoa), cửa sổ báo "đăng ký đã bị mất".

Chạy:  python3 dang_ky_gui.py <tên> [host] [cổng]       (mặc định 127.0.0.1 41742)
"""
import sys
import time
import tkinter as tk

from giao_dien_chung import DO_DAM, CuaSoNguoiDungCoSo, NhatKy, khung_nhom

XANH_DH = "#0b3d91"                                    # màu thanh tiêu đề cổng đào tạo
COT = [("Mã lớp HP", 10), ("Tên học phần", 15), ("TC", 3), ("Giảng viên", 17), ("Lịch học", 17),
       ("Đã ĐK / Sĩ số", 16), ("", 9)]


class CuaSoDangKy(CuaSoNguoiDungCoSo):
    TIEU_DE = "Cổng đăng ký học phần"

    def _ve(self, khung):
        self.cua_toi = {}                                # mã lớp -> giờ đăng ký (theo kết quả máy chủ)
        self.thieu_tu = {}                               # mã lớp -> lúc thấy danh sách lớp thiếu tên mình
        self.dang_cho = set()                            # mã lớp đang chờ trả lời
        self.hang = {}
        dau = tk.Frame(khung, bg=XANH_DH)
        dau.pack(fill="x", pady=(4, 0))
        tk.Label(dau, text="CỔNG ĐĂNG KÝ HỌC PHẦN", bg=XANH_DH, fg="white",
                 font=("TkDefaultFont", 14, "bold")).pack(side="left", padx=8, pady=6)
        tk.Label(dau, text="Học kỳ 1 · Năm học 2026–2027", bg=XANH_DH, fg="#ffd54f",
                 font=("TkDefaultFont", 10, "bold")).pack(side="right", padx=8)
        f = tk.Frame(khung, bg="#e8eef8")
        f.pack(fill="x")
        self.nhan_sv = tk.Label(f, text="Sinh viên: –", bg="#e8eef8", font=("TkDefaultFont", 10, "bold"))
        self.nhan_sv.pack(side="left", padx=8, pady=4)
        self.nhan_tc = tk.Label(f, text="", bg="#e8eef8", font=("TkDefaultFont", 10))
        self.nhan_tc.pack(side="right", padx=8)
        self.thong_bao = tk.Label(khung, text="Chưa kết nối", font=("TkDefaultFont", 11, "bold"), bg="#e0e0e0",
                                  pady=4)
        self.thong_bao.pack(fill="x", pady=(4, 2))
        g = khung_nhom(khung, "Danh sách lớp học phần mở đăng ký")
        g.pack(fill="x")
        self.bang = tk.Frame(g, bg="#9aa9c4")
        self.bang.pack(fill="x")
        for c, (chu, rong) in enumerate(COT):
            tk.Label(self.bang, text=chu, bg="#34558b", fg="white", width=rong, anchor="w",
                     font=("TkDefaultFont", 9, "bold"), padx=3).grid(row=0, column=c, sticky="nsew", padx=(0, 1),
                                                                     pady=(0, 1))
        g = khung_nhom(khung, "Kết quả đăng ký của bạn")
        g.pack(fill="x", pady=(4, 0))
        self.ls = tk.Listbox(g, height=4, width=80, font=("TkFixedFont", 9), activestyle="none")
        self.ls.pack(fill="x")

    # ------------------------------------------------------------ bảng lớp
    def _tao_hang(self, i, lop):
        nen = "white" if i % 2 else "#f3f6fb"
        o = []
        for c, (chu, rong) in enumerate(COT[:5]):
            nh = tk.Label(self.bang, text="", bg=nen, width=rong, anchor="w", padx=3, font=("TkDefaultFont", 9))
            nh.grid(row=i + 1, column=c, sticky="nsew", padx=(0, 1), pady=(0, 1))
            o.append(nh)
        f = tk.Frame(self.bang, bg=nen)
        f.grid(row=i + 1, column=5, sticky="nsew", padx=(0, 1), pady=(0, 1))
        cv = tk.Canvas(f, width=70, height=12, bg="#dddddd", highlightthickness=0)
        cv.pack(side="left", padx=(3, 3), pady=3)
        so = tk.Label(f, text="", bg=nen, font=("TkDefaultFont", 9, "bold"), width=7, anchor="w")
        so.pack(side="left")
        nut = tk.Button(self.bang, text="Đăng ký", width=8, relief="raised", bd=1,
                        command=lambda ma=lop["ma"]: self.bam(ma))
        nut.grid(row=i + 1, column=6, sticky="nsew", pady=(0, 1))
        self.hang[lop["ma"]] = {"o": o, "nen": nen, "cv": cv, "so": so, "nut": nut, "f": f}

    def bam(self, ma):
        """Bấm nút của hàng: Đăng ký (chưa có) hoặc Hủy (đã có)."""
        if not self.kn or ma in self.dang_cho:
            return
        loai = "huy" if ma in self.cua_toi else "dang_ky"
        self.dang_cho.add(ma)
        self.hang[ma]["nut"].configure(text="…", state="disabled")
        self.nhat_ky.ghi(("Hủy " if loai == "huy" else "Đăng ký ") + ma)
        self.gui_yc(loai, ma_lop=ma)

    def dang_ky(self, ma):
        """Kịch bản demo: bấm nút Đăng ký của lớp ma."""
        self.hang[ma]["nut"].invoke()

    def _ket_qua(self, loai, truong, tin, rtt):
        ma = truong["ma_lop"]
        self.dang_cho.discard(ma)
        if tin.get("ok"):
            if loai == "dang_ky":
                self.cua_toi[ma] = time.strftime("%H:%M:%S")
                self._bao(f"Đăng ký thành công lớp {ma}", "#70d070", "black")
            else:
                self.cua_toi.pop(ma, None)
                self._bao(f"Đã hủy đăng ký lớp {ma}", "#d0d0ff", "black")
            self.nhat_ky.ghi(f"  thành công ({tin['da_dk']}/{self.tt['si_so']}, {rtt:.1f} ms)")
        else:
            m = tin.get("ma_loi")
            text = {"lop_day": f"Lớp {ma} đã đủ sĩ số", "da_dang_ky": f"Bạn đã đăng ký lớp {ma} rồi",
                    "xung_dot": "Hệ thống bận, vui lòng thử lại"}.get(m, tin.get("ly_do", ""))
            if m == "da_dang_ky":
                self.cua_toi.setdefault(ma, time.strftime("%H:%M:%S"))
            self._bao(text, "#ffb347" if m != "lop_day" else "#ff6060", "black" if m != "lop_day" else "white")
            self.nhat_ky.ghi(f"  từ chối: {tin.get('ly_do')}")
        if self.tt:
            self._cap_nhat(self.tt, kiem_mat=False)

    def _bao(self, text, nen, chu):
        self.thong_bao.configure(text=text, bg=nen, fg=chu)

    # ------------------------------------------------------------ nhận trạng thái
    def _cap_nhat(self, tt, kiem_mat=True):
        ma_moi = [x["ma"] for x in tt["lop"]]
        if list(self.hang) != ma_moi:                    # số lớp đổi (đặt lại): vẽ lại bảng
            for h in self.hang.values():
                for w in h["o"] + [h["f"], h["nut"]]:
                    w.destroy()
            self.hang = {}
            for i, lop in enumerate(tt["lop"]):
                self._tao_hang(i, lop)
        if kiem_mat:
            self._kiem_mat(tt)
        for x in tt["lop"]:
            h = self.hang[x["ma"]]
            cua_toi = x["ma"] in self.cua_toi
            nen = "#d8f5d8" if cua_toi else h["nen"]
            for w, gt in zip(h["o"], (x["ma"], x["ten"], x["tc"], x["gv"], x["lich"])):
                w.configure(text=gt, bg=nen, font=("TkDefaultFont", 9, "bold" if cua_toi else "normal"))
            h["f"].configure(bg=nen)
            h["so"].configure(bg=nen)
            ti_le = min(1.0, x["da_dk"] / x["si_so"])
            h["cv"].delete("all")
            mau = DO_DAM if x["da_dk"] >= x["si_so"] else ("#f0a020" if ti_le >= 0.8 else "#3cb44b")
            h["cv"].create_rectangle(0, 0, 70 * ti_le, 12, fill=mau, width=0)
            h["so"].configure(text=f"{x['da_dk']}/{x['si_so']}", fg=DO_DAM if x["da_dk"] >= x["si_so"] else "black")
            if x["ma"] in self.dang_cho:
                continue
            if cua_toi:
                h["nut"].configure(text="Hủy", bg="#f08080", activebackground="#f08080", state="normal")
            elif x["da_dk"] >= x["si_so"]:
                h["nut"].configure(text="Đã đủ", bg="#c8c8c8", activebackground="#c8c8c8", state="normal")
            else:
                h["nut"].configure(text="Đăng ký", bg="#7fb3ff", activebackground="#7fb3ff", state="normal")
        tc = {x["ma"]: x["tc"] for x in tt["lop"]}
        so_tc = sum(tc.get(m, 0) for m in self.cua_toi)
        self.nhan_sv.configure(text=f"Sinh viên: {self.ten}")
        self.nhan_tc.configure(text=f"Đã đăng ký {len(self.cua_toi)} lớp · {so_tc} tín chỉ")
        ten = {x["ma"]: x["ten"] for x in tt["lop"]}
        ls = [f"{m}  {ten.get(m, '?'):<16} {tc.get(m, 0)} TC  lúc {g}" for m, g in sorted(self.cua_toi.items())]
        if list(self.ls.get(0, "end")) != ls:
            self.ls.delete(0, "end")
            for x in ls:
                self.ls.insert("end", x)
        if self.thong_bao["text"] == "Chưa kết nối":
            self._bao("Đợt đăng ký đang mở – chọn lớp và bấm Đăng ký", "#e0e0e0", "black")

    def _kiem_mat(self, tt):
        """Đối chiếu kết quả mình nhận với danh sách lớp của máy chủ. cap_nhat có thể đến trễ hơn kq,
        nên chỉ báo khi danh sách thiếu tên mình liên tục 0,5 s."""
        for x in tt["lop"]:
            ma = x["ma"]
            if ma in self.dang_cho or self.ten in x["sv"] or ma not in self.cua_toi:
                self.thieu_tu.pop(ma, None)
            if ma in self.dang_cho:
                continue
            if ma in self.cua_toi and self.ten not in x["sv"]:
                if time.monotonic() - self.thieu_tu.setdefault(ma, time.monotonic()) < 0.5:
                    continue
                self.thieu_tu.pop(ma, None)
                self.cua_toi.pop(ma)                     # máy chủ báo thành công nhưng danh sách không có tên
                self._bao(f"Đăng ký {ma} đã bị MẤT (lỗi máy chủ)!", "#c00000", "white")
                self.nhat_ky.ghi(f"CẢNH BÁO: danh sách lớp {ma} không có tên bạn")
                self._cap_nhat(tt, kiem_mat=False)
            elif ma not in self.cua_toi and self.ten in x["sv"]:
                self.cua_toi[ma] = "–"                   # (đăng ký từ thiết bị khác)

    def _moi_nhip(self):
        if self.kn and self.tt:
            self._kiem_mat(self.tt)

    def _su_kien(self, text):
        super()._su_kien(text)
        if text.startswith("Đặt lại"):                   # đợt đăng ký mới: danh sách cũ không còn
            self.cua_toi.clear()
            self.thieu_tu.clear()
            self._bao("Đợt đăng ký mới – chọn lớp và bấm Đăng ký", "#e0e0e0", "black")

    def _dong_dinh_ky(self):
        lop = ", ".join(f"{x['ma']} {x['da_dk']}/{x['si_so']}" for x in self.tt["lop"])
        return f"{lop}; bạn: {', '.join(sorted(self.cua_toi)) or 'chưa có lớp'}"


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "An"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41742
    root = tk.Tk()
    cs = CuaSoDangKy(root, ten, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
