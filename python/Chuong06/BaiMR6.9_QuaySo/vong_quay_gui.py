"""Bài mở rộng MR6.9 – Vòng quay may mắn (ứng dụng người dùng, Tkinter).

Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở giao_dien_chung.CuaSoNguoiDungCoSo;
tệp này vẽ vòng quay: bánh xe chia ô (mỗi món quà một ô màu, xen giữa là ô "Chúc may mắn lần sau"),
kim chỉ ở đỉnh, đèn viền nhấp nháy khi quay, nút QUAY. Bấm QUAY: bánh xe quay tự do ngay, yêu cầu
{"loai":"quay"} đi tới máy chủ; khi có kết quả, bánh xe giảm tốc (ease-out) và dừng đúng ô kết quả
(món trúng, hoặc một ô "may mắn lần sau"). Bên phải: số quà còn lại, ảnh từng món (vẽ bằng Canvas) với
trạng thái còn / đã trao cho ai, danh sách người trúng trực tiếp (từ cap_nhat ≤ 10 lần/giây).

Chạy:  python3 vong_quay_gui.py <tên> [host] [cổng]       (mặc định 127.0.0.1 41732)
"""
import math
import random
import sys
import time
import tkinter as tk

from giao_dien_chung import DO_DAM, NEN, XANH_DAM, CuaSoNguoiDungCoSo, NhatKy, khung_nhom, nhan
from may_chu import TEN_QUA, chu_p

MAU_QUA = ["#e6194b", "#f58231", "#ffe119", "#3cb44b", "#42d4f4", "#4363d8", "#911eb4", "#f032e6",
           "#9a6324", "#469990"]
TEN_NGAN = ["Xe đạp", "Điện thoại", "Laptop", "Tai nghe", "Đồng hồ", "Máy ảnh", "Ba lô", "Nồi cơm",
            "Loa", "500k"]


# ===================================================================== ảnh quà (vẽ đơn giản)
def ve_qua(cv, i, cx, cy, s, xam=False):
    """Vẽ món quà i quanh tâm (cx, cy), nửa cạnh s. xam=True: đã trao (vẽ màu xám)."""
    m = "#9a9a9a" if xam else MAU_QUA[i]
    den = "#7a7a7a" if xam else "#303030"
    if i == 0:                                           # xe đạp
        for dx in (-0.55, 0.55):
            cv.create_oval(cx + (dx - 0.4) * s, cy - 0.1 * s, cx + (dx + 0.4) * s, cy + 0.7 * s, outline=den,
                           width=2)
        cv.create_line(cx - 0.55 * s, cy + 0.3 * s, cx - 0.1 * s, cy - 0.35 * s, cx + 0.4 * s, cy - 0.35 * s,
                       cx + 0.55 * s, cy + 0.3 * s, fill=m, width=3)
        cv.create_line(cx - 0.1 * s, cy - 0.35 * s, cx, cy + 0.3 * s, cx + 0.4 * s, cy - 0.35 * s, fill=m, width=3)
        cv.create_line(cx - 0.25 * s, cy - 0.5 * s, cx + 0.05 * s, cy - 0.5 * s, fill=den, width=3)
        cv.create_line(cx + 0.35 * s, cy - 0.6 * s, cx + 0.55 * s, cy - 0.6 * s, fill=den, width=3)
    elif i == 1:                                         # điện thoại
        cv.create_rectangle(cx - 0.42 * s, cy - 0.85 * s, cx + 0.42 * s, cy + 0.85 * s, fill=den, outline=den)
        cv.create_rectangle(cx - 0.34 * s, cy - 0.68 * s, cx + 0.34 * s, cy + 0.6 * s, fill=m, outline="")
        cv.create_oval(cx - 0.07 * s, cy + 0.66 * s, cx + 0.07 * s, cy + 0.8 * s, fill="#d0d0d0", outline="")
    elif i == 2:                                         # laptop
        cv.create_rectangle(cx - 0.7 * s, cy - 0.65 * s, cx + 0.7 * s, cy + 0.3 * s, fill=den, outline=den)
        cv.create_rectangle(cx - 0.6 * s, cy - 0.55 * s, cx + 0.6 * s, cy + 0.2 * s, fill=m, outline="")
        cv.create_polygon(cx - 0.7 * s, cy + 0.35 * s, cx + 0.7 * s, cy + 0.35 * s, cx + 0.9 * s, cy + 0.6 * s,
                          cx - 0.9 * s, cy + 0.6 * s, fill="#b0b0b0", outline=den)
    elif i == 3:                                         # tai nghe
        cv.create_arc(cx - 0.65 * s, cy - 0.75 * s, cx + 0.65 * s, cy + 0.55 * s, start=0, extent=180,
                      style="arc", outline=den, width=3)
        for dx in (-0.65, 0.65):
            cv.create_rectangle(cx + (dx - 0.18) * s, cy - 0.1 * s, cx + (dx + 0.18) * s, cy + 0.6 * s, fill=m,
                                outline=den)
    elif i == 4:                                         # đồng hồ
        cv.create_rectangle(cx - 0.25 * s, cy - 0.9 * s, cx + 0.25 * s, cy + 0.9 * s, fill="#6b4226", outline="")
        cv.create_oval(cx - 0.55 * s, cy - 0.55 * s, cx + 0.55 * s, cy + 0.55 * s, fill="white", outline=m,
                       width=3)
        cv.create_line(cx, cy, cx, cy - 0.38 * s, fill=den, width=2)
        cv.create_line(cx, cy, cx + 0.28 * s, cy + 0.1 * s, fill=den, width=2)
    elif i == 5:                                         # máy ảnh
        cv.create_rectangle(cx - 0.3 * s, cy - 0.65 * s, cx + 0.15 * s, cy - 0.4 * s, fill=den, outline="")
        cv.create_rectangle(cx - 0.85 * s, cy - 0.45 * s, cx + 0.85 * s, cy + 0.6 * s, fill=m, outline=den)
        cv.create_oval(cx - 0.42 * s, cy - 0.33 * s, cx + 0.42 * s, cy + 0.5 * s, fill=den, outline="white")
        cv.create_oval(cx - 0.2 * s, cy - 0.12 * s, cx + 0.2 * s, cy + 0.28 * s, fill="#5080c0", outline="")
    elif i == 6:                                         # ba lô
        cv.create_arc(cx - 0.4 * s, cy - 0.95 * s, cx + 0.4 * s, cy - 0.45 * s, start=0, extent=180,
                      style="arc", outline=den, width=2)
        cv.create_rectangle(cx - 0.6 * s, cy - 0.7 * s, cx + 0.6 * s, cy + 0.85 * s, fill=m, outline=den)
        cv.create_rectangle(cx - 0.4 * s, cy + 0.15 * s, cx + 0.4 * s, cy + 0.65 * s, fill=den, outline="")
    elif i == 7:                                         # nồi cơm điện
        cv.create_oval(cx - 0.8 * s, cy - 0.55 * s, cx + 0.8 * s, cy - 0.05 * s, fill="#e0e0e0", outline=den)
        cv.create_rectangle(cx - 0.8 * s, cy - 0.3 * s, cx + 0.8 * s, cy + 0.7 * s, fill=m, outline=den)
        cv.create_rectangle(cx - 0.15 * s, cy - 0.75 * s, cx + 0.15 * s, cy - 0.5 * s, fill=den, outline="")
        cv.create_rectangle(cx - 0.25 * s, cy + 0.1 * s, cx + 0.25 * s, cy + 0.4 * s, fill="white", outline="")
    elif i == 8:                                         # loa
        cv.create_rectangle(cx - 0.55 * s, cy - 0.9 * s, cx + 0.55 * s, cy + 0.9 * s, fill=m, outline=den)
        cv.create_oval(cx - 0.18 * s, cy - 0.72 * s, cx + 0.18 * s, cy - 0.36 * s, fill=den, outline="")
        cv.create_oval(cx - 0.42 * s, cy - 0.15 * s, cx + 0.42 * s, cy + 0.69 * s, fill=den, outline="white")
    else:                                                # phiếu mua hàng 500k
        cv.create_rectangle(cx - 0.9 * s, cy - 0.5 * s, cx + 0.9 * s, cy + 0.5 * s, fill=m, outline=den)
        cv.create_line(cx + 0.4 * s, cy - 0.5 * s, cx + 0.4 * s, cy + 0.5 * s, fill="white", dash=(2, 2))
        cv.create_text(cx - 0.25 * s, cy, text="500K", fill="white", font=("TkDefaultFont", max(6, int(s * 0.38)),
                                                                            "bold"))


def bo_tri_o(so_qua):
    """Các ô của bánh xe: mã quà hoặc None ("may mắn lần sau"); quà rải đều, tối thiểu 8 ô."""
    n = max(2 * so_qua, 8)
    o = [None] * n
    for i in range(so_qua):
        o[round(i * n / so_qua)] = i
    return o


# ===================================================================== cửa sổ
class CuaSoVongQuay(CuaSoNguoiDungCoSo):
    TIEU_DE = "Vòng quay may mắn"
    R = 168                                              # bán kính bánh xe

    def _ve(self, khung):
        self.goc = 90.0                                  # góc xoay bánh xe (độ, ngược chiều kim đồng hồ)
        self.pha = None                                  # None | "tu_do" | "dung"
        self.tt_ve = None                                # trạng thái dùng để vẽ bánh xe
        self.cho_kq = None                               # kết quả chờ đến lúc bánh xe dừng
        self.ket_qua = None                              # dòng thông báo kết quả cuối
        self.nhip = 0
        trai = tk.Frame(khung, bg=NEN)
        trai.pack(side="left", fill="y", pady=4)
        self.cv = tk.Canvas(trai, width=2 * self.R + 40, height=2 * self.R + 44, bg="#2b1055", highlightthickness=0)
        self.cv.pack()
        f = tk.Frame(trai, bg=NEN)
        f.pack(fill="x", pady=(6, 0))
        self.nut_quay = tk.Button(f, text="QUAY", font=("TkDefaultFont", 16, "bold"), bg="#d62828", fg="white",
                                  activebackground="#ef4444", activeforeground="white", width=10,
                                  command=self.quay, relief="raised", bd=3)
        self.nut_quay.pack(side="left")
        self.thong_bao = tk.Label(f, text="Chưa kết nối", font=("TkDefaultFont", 11, "bold"), bg="#e0e0e0",
                                  width=26, height=2, wraplength=250)
        self.thong_bao.pack(side="left", padx=(8, 0), fill="x")

        phai = tk.Frame(khung, bg=NEN)
        phai.pack(side="left", fill="both", expand=True, padx=(10, 0), pady=4)
        nhan(phai, "VÒNG QUAY MAY MẮN", font=("TkDefaultFont", 15, "bold"), fg="#7a1fa2").pack(anchor="w")
        self.nhan_luat = nhan(phai, "Mỗi người 1 lượt", font=("TkDefaultFont", 9))
        self.nhan_luat.pack(anchor="w")
        self.nhan_con = nhan(phai, "–", font=("TkDefaultFont", 12, "bold"), fg=DO_DAM)
        self.nhan_con.pack(anchor="w", pady=(2, 0))
        self.nhan_da_quay = nhan(phai, "–", font=("TkDefaultFont", 9))
        self.nhan_da_quay.pack(anchor="w")
        g = khung_nhom(phai, "Quà tặng")
        g.pack(fill="x", pady=(4, 0))
        self.o_qua = []
        for i in range(len(TEN_QUA)):
            o = tk.Frame(g, bg=NEN)
            o.grid(row=i // 2, column=i % 2, sticky="w", padx=(0, 6), pady=1)
            cv = tk.Canvas(o, width=36, height=36, bg="white", highlightthickness=1, highlightbackground="#808080")
            cv.pack(side="left")
            nh = nhan(o, TEN_QUA[i], font=("TkDefaultFont", 8), justify="left", width=14)
            nh.pack(side="left", padx=(3, 0))
            self.o_qua.append((cv, nh))
        self._da_ve_qua = None
        g = khung_nhom(phai, "Người trúng thưởng (trực tiếp)")
        g.pack(fill="both", expand=True, pady=(4, 0))
        self.ls = tk.Listbox(g, height=7, width=32, font=("TkFixedFont", 9), activestyle="none")
        self.ls.pack(fill="both", expand=True)
        self._ve_banh_xe()
        self.nut_quay.configure(state="disabled")

    # ------------------------------------------------------------ bánh xe
    def _ve_banh_xe(self):
        cv, R = self.cv, self.R
        cx, cy = R + 20, R + 26
        cv.delete("all")
        tt = self.tt_ve
        so_qua = tt["so_qua"] if tt else 10
        con = {q["ma"]: q["con"] for q in tt["qua"]} if tt else {}
        o = bo_tri_o(so_qua)
        ext = 360 / len(o)
        cv.create_oval(cx - R - 14, cy - R - 14, cx + R + 14, cy + R + 14, fill="#f4b400", outline="#a86b00", width=3)
        for j, ma in enumerate(o):
            bat_dau = self.goc + j * ext
            if ma is None:
                mau, chu, mau_chu = ("#fff7e6" if j % 2 else "#ffe8f0"), "May mắn\nlần sau", "#7a1fa2"
            elif con.get(ma, True):
                mau, chu, mau_chu = MAU_QUA[ma], TEN_NGAN[ma], "white" if ma not in (2, 4) else "#202020"
            else:
                mau, chu, mau_chu = "#bdbdbd", TEN_NGAN[ma] + "\n(đã trao)", "#606060"
            cv.create_arc(cx - R, cy - R, cx + R, cy + R, start=bat_dau, extent=ext, fill=mau, outline="white",
                          width=2)
            giua = math.radians(bat_dau + ext / 2)
            r = R * 0.62
            x, y = cx + r * math.cos(giua), cy - r * math.sin(giua)
            goc_chu = (bat_dau + ext / 2) % 360
            if 90 < goc_chu < 270:                       # chữ ở nửa trái: lật cho dễ đọc
                goc_chu -= 180
            cv.create_text(x, y, text=chu, angle=goc_chu, fill=mau_chu, justify="center",
                           font=("TkDefaultFont", 8 if ma is None or len(o) > 12 else 9, "bold"))
        for k in range(24):                              # đèn viền: nhấp nháy khi quay
            a = math.radians(k * 15)
            x, y = cx + (R + 7) * math.cos(a), cy - (R + 7) * math.sin(a)
            sang = (k + self.nhip // 3) % 2 == 0 if self.pha else k % 2 == 0
            cv.create_oval(x - 4, y - 4, x + 4, y + 4, fill="#fffde0" if sang else "#c08000", outline="")
        cv.create_oval(cx - 30, cy - 30, cx + 30, cy + 30, fill="#d62828", outline="white", width=3)
        cv.create_text(cx, cy, text="MAY\nMẮN", fill="white", font=("TkDefaultFont", 8, "bold"), justify="center")
        cv.create_polygon(cx - 15, cy - R - 24, cx + 15, cy - R - 24, cx, cy - R + 12, fill="#d62828",
                          outline="white", width=2)    # kim chỉ ở đỉnh

    def quay(self):
        if not self.kn or self.pha:
            return
        self.tt_ve = self.tt
        self.pha, self.t_bam, self.cho_kq = "tu_do", time.monotonic(), None
        self.nut_quay.configure(state="disabled", text="ĐANG QUAY")
        self._bao("Đang quay…", "#fff3c4", "black")
        self.nhat_ky.ghi("Bấm QUAY")
        self.gui_yc("quay")
        self._khung_hinh()

    def _ket_qua(self, loai, truong, tin, rtt):
        self.nhat_ky.ghi(f"  máy chủ trả lời sau {rtt:.1f} ms: " + (
            (f"trúng {tin['ten_qua']}" if tin.get("trung") else "không trúng") if tin.get("ok") else
            f"từ chối – {tin.get('ly_do')}"))
        self.cho_kq = tin
        if not self.pha:                                 # (không xảy ra khi bấm nút; phòng khi gọi trực tiếp)
            self._ket_thuc_quay()

    def _bat_dau_dung(self):
        tin = self.cho_kq
        o = bo_tri_o(self.tt_ve["so_qua"])
        if tin.get("ok") and tin.get("trung") and tin["qua"] in o:
            j = o.index(tin["qua"])
        else:
            j = random.choice([k for k, ma in enumerate(o) if ma is None])
        ext = 360 / len(o)
        goc_dich = 90 - (j + 0.5) * ext + random.uniform(-0.3, 0.3) * ext
        m = math.ceil((goc_dich - (self.goc - 540)) / 360)
        self.goc0, self.goc_dich = self.goc, goc_dich - 360 * m
        self.t_dung = time.monotonic()
        self.pha = "dung"

    def _khung_hinh(self):
        if not self.pha:
            return
        self.nhip += 1
        if self.pha == "tu_do":
            self.goc -= 20                               # quay theo chiều kim đồng hồ
            if self.cho_kq is not None and time.monotonic() - self.t_bam > 0.6:
                self._bat_dau_dung()
        else:
            t = min(1.0, (time.monotonic() - self.t_dung) / 2.6)
            self.goc = self.goc0 + (self.goc_dich - self.goc0) * (1 - (1 - t) ** 3)   # giảm tốc (ease-out)
            if t >= 1.0:
                self.pha = None
                self._ve_banh_xe()
                self._ket_thuc_quay()
                return
        self._ve_banh_xe()
        self.root.after(30, self._khung_hinh)

    def _ket_thuc_quay(self):
        tin = self.cho_kq
        if tin.get("ok") and tin.get("trung"):
            self.ket_qua = ("CHÚC MỪNG! Bạn trúng " + tin["ten_qua"], "#ffd700", "black")
        elif tin.get("ok"):
            self.ket_qua = ("Chúc bạn may mắn lần sau", "#d0d0ff", "black")
        elif tin.get("ma_loi") == "da_quay":
            self.ket_qua = ("Bạn đã quay rồi – mỗi người 1 lượt", "#ffb347", "black")
        else:
            self.ket_qua = ("Lỗi: " + tin.get("ly_do", ""), "#ff6060", "white")
        self._bao(*self.ket_qua)
        self.nhat_ky.ghi("Kết quả: " + self.ket_qua[0])
        self.nut_quay.configure(text="ĐÃ QUAY" if tin.get("ok") or tin.get("ma_loi") == "da_quay" else "QUAY",
                                state="disabled" if tin.get("ok") or tin.get("ma_loi") == "da_quay" else "normal")
        if self.tt:
            self.tt_ve = self.tt
            self._ve_banh_xe()

    def _bao(self, text, nen, chu):
        self.thong_bao.configure(text=text, bg=nen, fg=chu)

    # ------------------------------------------------------------ nhận trạng thái
    def _cap_nhat(self, tt):
        if not self.pha:
            moi_dat_lai = self.tt_ve is not None and tt["da_quay"] < self.tt_ve["da_quay"]
            self.tt_ve = tt
            self._ve_banh_xe()
            if moi_dat_lai or self.ket_qua is None:     # đặt lại chương trình: được quay lại
                self.ket_qua = None
                self.nut_quay.configure(text="QUAY", state="normal")
                self._bao("Mời bạn bấm QUAY", "#e0e0e0", "black")
        self.nhan_luat.configure(text=f"Mỗi người 1 lượt · xác suất trúng {chu_p(tt['p'])} mỗi lượt")
        self.nhan_con.configure(text=f"Còn {tt['con_lai']}/{tt['so_qua']} quà",
                                fg=DO_DAM if tt["con_lai"] else "#606060")
        self.nhan_da_quay.configure(text=f"Đã quay: {tt['da_quay']:,} lượt · {tt['so_trung']} lượt trúng"
                                    .replace(",", "."))
        ve = tuple((q["ma"], q["con"]) for q in tt["qua"])
        if ve != self._da_ve_qua:
            self._da_ve_qua = ve
            for i, (cv, nh) in enumerate(self.o_qua):
                cv.delete("all")
                if i >= tt["so_qua"]:                    # chương trình ít quà hơn: ẩn ô thừa
                    cv.configure(bg=NEN, highlightthickness=0)
                    continue
                q = tt["qua"][i]
                ve_qua(cv, i, 18, 18, 15, xam=not q["con"])
                cv.configure(bg="white" if q["con"] else "#e8e8e8", highlightthickness=1)
        for i, (cv, nh) in enumerate(self.o_qua):
            if i < tt["so_qua"]:
                q = tt["qua"][i]
                nh.configure(text=q["ten"] + ("\ncòn" if q["con"] else f"\n→ {q['nguoi'] or '?'}"),
                             fg=XANH_DAM if q["con"] else "#a00000")
            else:
                nh.configure(text="")
        ls = [f"{x['gio']} {x['nguoi']:<6} {TEN_QUA[x['qua']]}" for x in tt["nguoi_trung"]]
        if list(self.ls.get(0, "end")) != ls:
            self.ls.delete(0, "end")
            for i, x in enumerate(ls):
                self.ls.insert("end", x)
                if tt["nguoi_trung"][i]["nguoi"] == self.ten:
                    self.ls.itemconfigure(i, fg="#0040c0")

    def _dong_dinh_ky(self):
        tt = self.tt
        kq = self.ket_qua[0] if self.ket_qua else "chưa quay"
        return f"còn {tt['con_lai']}/{tt['so_qua']} quà, đã quay {tt['da_quay']} lượt; bạn: {kq}"


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "An"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41732
    root = tk.Tk()
    cs = CuaSoVongQuay(root, ten, host, cong)
    root.protocol("WM_DELETE_WINDOW", cs.dong)
    root.mainloop()


if __name__ == "__main__":
    NhatKy.in_ra = True                     # chạy riêng: nhật ký cũng ra dòng lệnh
    main()
