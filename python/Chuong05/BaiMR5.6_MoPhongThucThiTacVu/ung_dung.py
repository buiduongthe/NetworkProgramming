"""Bài mở rộng MR5.6 – Mô phỏng thực thi tác vụ: ứng dụng giao diện (Tkinter).

Chọn chế độ (đơn luồng/đa luồng × đồng bộ/bất đồng bộ) hoặc "So sánh 4 chế độ"; biểu đồ Gantt chạy theo
thời gian: mỗi hàng là một luồng (luồng hệ điều hành, vòng lặp sự kiện hay luồng thợ), màu là tác vụ
đang dùng CPU; ô nhạt có gạch chéo: luồng bị chặn chờ I/O; ô viền đứt: sẵn sàng nhưng chờ lõi CPU; ô chấm: chờ
join. Hàng "I/O chờ" (bất đồng bộ) vẽ các lần chờ I/O không giữ luồng nào. Nút "Chen ngang U ngay" đưa việc
khẩn U vào đúng thời điểm đang xem và mô phỏng lại (lịch sử trước đó không đổi vì bộ lập lịch tất định).

Toàn bộ chạy trên luồng giao diện: mô phỏng rất nhanh (vài mili giây), hoạt hình dùng root.after() – không
có luồng nền nào, không bao giờ chặn vòng lặp sự kiện của Tk.

Chạy:  python3 ung_dung.py            (Windows: py ung_dung.py)
       python3 ung_dung.py --demo     kịch bản tự thao tác, chụp cửa sổ thành ket_qua_gui.png (cần X11 +
                                      ImageMagick, ví dụ dưới xvfb-run)
"""
import subprocess
import sys
import time
import tkinter as tk
from tkinter import font as tkfont
from tkinter import ttk

import lap_lich as L

MAU = {"T1": "#2457c5", "T2": "#e07b00", "T3": "#1b9e5a", "U": "#d1344b"}
NHAT = {"T1": "#c9d8f6", "T2": "#fbdcb8", "T3": "#c3ead3", "U": "#f6c5cc"}
NEN, LUOI, CHU = "#ffffff", "#e4e7ec", "#1d2330"


class UngDung:
    def __init__(self, root):
        self.root = root
        root.title("Mô phỏng thực thi tác vụ – MR5.6")
        root.geometry("1280x860")
        self.ket_qua = []            # danh sách (chế độ, KetQua) đang hiển thị
        self.T = 0.0                 # thời điểm đang xem (nhịp, số thực để hoạt hình mượt)
        self.dang_chay = False
        self.last = None
        self.dang_dat_thanh = False      # đang tự đặt thanh kéo (không coi là người dùng kéo)
        self.tao_giao_dien()
        self.ve()
        root.after(30, self.nhip)

    # ------------------------------------------------------------------ giao diện
    def tao_giao_dien(self):
        tren = ttk.Frame(self.root, padding=6)
        tren.pack(fill="x")
        dk = ttk.LabelFrame(tren, text="Điều khiển", padding=6)
        dk.pack(side="left", fill="y")
        nut = [("Đơn luồng – đồng bộ", "don_dong_bo"), ("Đa luồng – đồng bộ", "da_dong_bo"),
               ("Đơn luồng – bất đồng bộ", "don_bat_dong_bo"), ("Đa luồng – bất đồng bộ", "da_bat_dong_bo")]
        self.nut = {}
        for i, (ten, m) in enumerate(nut):
            b = ttk.Button(dk, text=ten, width=21, command=lambda m=m: self.chon([m]))
            b.grid(row=i // 2, column=i % 2, padx=3, pady=3)
            self.nut[m] = b
        ttk.Button(dk, text="So sánh 4 chế độ", command=lambda: self.chon(list(L.CHE_DO))).grid(
            row=0, column=2, padx=3, pady=3, sticky="ew")
        ttk.Button(dk, text="Xóa", command=self.xoa).grid(row=1, column=2, padx=3, pady=3, sticky="ew")
        self.btn_chen = ttk.Button(dk, text="⚡ Chen ngang U ngay", command=self.chen_ngang)
        self.btn_chen.grid(row=0, column=3, padx=3, pady=3, sticky="ew")
        self.btn_chay = ttk.Button(dk, text="⏸ Dừng", command=self.chay_dung)
        self.btn_chay.grid(row=1, column=3, padx=3, pady=3, sticky="ew")

        ts = ttk.LabelFrame(tren, text="Thông số", padding=6)
        ts.pack(side="left", fill="y", padx=8)
        self.v = {}
        for i, (ten, k, mac) in enumerate([("T1 (s)", "t1", "4"), ("T2 (s)", "t2", "3"), ("T3 (s)", "t3", "2")]):
            ttk.Label(ts, text=ten).grid(row=i, column=0, sticky="w")
            self.v[k] = tk.StringVar(value=mac)
            ttk.Entry(ts, textvariable=self.v[k], width=5).grid(row=i, column=1, padx=3, pady=1)
        ttk.Label(ts, text="Số lõi CPU").grid(row=0, column=2, sticky="w", padx=(10, 0))
        self.v["loi"] = tk.StringVar(value="2")
        ttk.Spinbox(ts, from_=1, to=4, textvariable=self.v["loi"], width=4).grid(row=0, column=3, padx=3)
        ttk.Label(ts, text="Lát thời gian (s)").grid(row=1, column=2, sticky="w", padx=(10, 0))
        self.v["lat"] = tk.StringVar(value="0.5")
        ttk.Entry(ts, textvariable=self.v["lat"], width=5).grid(row=1, column=3, padx=3)
        ttk.Label(ts, text="Tốc độ").grid(row=2, column=2, sticky="w", padx=(10, 0))
        self.v["toc_do"] = tk.StringVar(value="× 1")
        ttk.Combobox(ts, textvariable=self.v["toc_do"], values=["× 0.25", "× 0.5", "× 1", "× 2", "× 4"],
                     width=6, state="readonly").grid(row=2, column=3, padx=3)
        self.b = {}
        for i, (ten, k) in enumerate([("T3 chờ T1, T2 (join/await)", "phu_thuoc"),
                                      ("Chia CPU cuối T1 trên 2 luồng", "chia"),
                                      ("Việc khẩn U lúc (s)", "khan")]):
            self.b[k] = tk.BooleanVar(value=True)
            ttk.Checkbutton(ts, text=ten, variable=self.b[k]).grid(row=i, column=4, sticky="w", padx=(12, 0))
        self.v["t_khan"] = tk.StringVar(value="0.3")
        ttk.Entry(ts, textvariable=self.v["t_khan"], width=5).grid(row=2, column=5, padx=3)

        self.lbl_gio = ttk.Label(tren, text="", font=("TkDefaultFont", 12, "bold"))
        self.lbl_gio.pack(side="right", padx=10)

        giua = ttk.Frame(self.root, padding=(6, 0, 6, 6))
        giua.pack(fill="both", expand=True)
        trai = ttk.Frame(giua)
        trai.pack(side="left", fill="both", expand=True)
        self.cv = tk.Canvas(trai, bg=NEN, highlightthickness=1, highlightbackground="#c9ced8")
        self.cv.pack(fill="both", expand=True)
        self.cv.bind("<Configure>", lambda e: self.ve())
        self.thanh = ttk.Scale(trai, from_=0, to=1000, orient="horizontal", command=self.keo)
        self.thanh.pack(fill="x", pady=(4, 0))
        cot = ["Chế độ", "Hoàn tất", "Luồng", "Lõi bận", "U chờ", "Chen", "Hết lát"]
        self.bang = ttk.Treeview(trai, columns=cot, show="headings", height=4)
        for c in cot:
            self.bang.heading(c, text=c)
            self.bang.column(c, width=150 if c == "Chế độ" else 80, anchor="center")
        self.bang.pack(fill="x", pady=(4, 0))
        phai = ttk.LabelFrame(giua, text="Nhật ký sự kiện", padding=4)
        phai.pack(side="right", fill="y", padx=(6, 0))
        self.nk = tk.Text(phai, width=46, height=30, font=("TkFixedFont", 9), wrap="word")
        self.nk.pack(fill="both", expand=True)

    # ------------------------------------------------------------------ thao tác
    def doc_tham_so(self):
        def so(k, mac):
            try:
                return float(self.v[k].get().replace(",", "."))
            except ValueError:
                return mac
        return L.ThamSo(max(0.4, so("t1", 4)), max(0.4, so("t2", 3)), max(0.4, so("t3", 2)),
                        int(min(4, max(1, so("loi", 2)))), max(0.1, so("lat", 0.5)),
                        self.b["phu_thuoc"].get(), self.b["chia"].get(), self.b["khan"].get(),
                        max(0.0, so("t_khan", 0.3)))

    def chon(self, ds_che_do):
        ts = self.doc_tham_so()
        self.ket_qua = [(m, L.chay(m, ts)) for m in ds_che_do]
        self.T, self.dang_chay, self.last = 0.0, True, None
        self.btn_chay.config(text="⏸ Dừng")
        self.cap_nhat_bang()
        self.ve()

    def xoa(self):
        self.ket_qua, self.T, self.dang_chay = [], 0.0, False
        self.bang.delete(*self.bang.get_children())
        self.ve()

    def chay_dung(self):
        self.dang_chay = not self.dang_chay
        self.last = None
        self.btn_chay.config(text="⏸ Dừng" if self.dang_chay else "▶ Chạy")

    def chen_ngang(self):
        """Đặt việc khẩn U đến ngay thời điểm đang xem rồi mô phỏng lại; giữ nguyên thời điểm xem."""
        if not self.ket_qua:
            return
        self.b["khan"].set(True)
        self.v["t_khan"].set(L.giay(int(self.T)))
        T = self.T
        ds = [m for m, _ in self.ket_qua]
        ts = self.doc_tham_so()
        self.ket_qua = [(m, L.chay(m, ts)) for m in ds]
        self.T = T
        self.cap_nhat_bang()
        self.ve()

    def keo(self, gia_tri):
        D = self.do_dai()
        if D and not self.dang_chay and not self.dang_dat_thanh:
            self.T = float(gia_tri) / 1000 * D
            self.ve()

    def do_dai(self):
        return max((kq.tong for _, kq in self.ket_qua), default=0)

    def cap_nhat_bang(self):
        self.bang.delete(*self.bang.get_children())
        for m, kq in self.ket_qua:
            s = L.chi_so(kq)
            u = L.giay(s["u_cho"]) + " s" if s["u_cho"] >= 0 else "–"
            self.bang.insert("", "end", values=(L.TEN_NGAN[m], L.giay(s["tong"]) + " s", s["luong"],
                                                f"{s['ban']} %", u, s["chen"], s["lat"]))

    def nhip(self):
        now = time.monotonic()
        if self.dang_chay and self.ket_qua:
            if self.last is not None:
                toc = float(self.v["toc_do"].get().split()[1])
                self.T += (now - self.last) * 10 * toc          # ×1: 1 giây mô phỏng mỗi giây thật
                if self.T >= self.do_dai():
                    self.T = float(self.do_dai())
                    self.dang_chay = False
                    self.btn_chay.config(text="▶ Chạy")
            self.last = now
            self.ve()
        self.root.after(30, self.nhip)

    # ------------------------------------------------------------------ vẽ
    def ve(self):
        cv = self.cv
        cv.delete("all")
        W, H = max(200, cv.winfo_width()), max(200, cv.winfo_height())
        D = self.do_dai()
        t = min(int(self.T), D)
        self.lbl_gio.config(text=f"t = {L.giay(t)} / {L.giay(D)} s" if D else "Chọn một chế độ")
        if D:
            self.dang_dat_thanh = True
            self.thanh.set(1000 * self.T / D)
            self.dang_dat_thanh = False
        if not self.ket_qua:
            cv.create_text(W // 2, H // 2, text="Chọn một chế độ để bắt đầu mô phỏng", fill="#667085",
                           font=("TkDefaultFont", 14))
            self.nk.delete("1.0", "end")
            return
        trai, phai = 110, W - 20
        X = lambda n: trai + (phai - trai) * n / max(1, D)
        so_hang = sum(len(kq.lan) + (1 if kq.io else 0) + 1.2 for _, kq in self.ket_qua)
        h = min(48, (H - 50) / so_hang)
        y = 8
        for m, kq in self.ket_qua:
            cv.create_text(8, y + h * 0.4, text=L.CHE_DO[m], anchor="w", fill=CHU,
                           font=("TkDefaultFont", 10, "bold"))
            y += h * 0.9
            for ten, o in kq.lan:
                self.ve_lan(ten, o, y, h, X, t)
                y += h
            if kq.io:
                self.ve_io(kq, y, h, X, t)
                y += h
            y += h * 0.3
        # trục thời gian và đường "bây giờ"
        for g in range(0, D + 1, 5):
            x = X(g)
            cv.create_line(x, 4, x, y, fill=LUOI if g % 10 else "#c9ced8", tags="luoi")
            if g % 10 == 0:
                cv.create_text(x, y + 10, text=f"{g // 10} s", fill="#667085")
        cv.tag_lower("luoi")
        xt = X(self.T)
        cv.create_line(xt, 2, xt, y, fill="#f2b705", width=2)
        self.ve_chu_giai(y + 26)
        self.ve_nhat_ky(t)

    def ve_lan(self, ten, o, y, h, X, t):
        cv = self.cv
        cv.create_text(104, y + h / 2, text=ten, anchor="e", fill=CHU)
        cv.create_rectangle(X(0), y + 2, X(len(o)), y + h - 2, fill="#f6f7f9", outline="")
        j = 0
        while j < min(len(o), t + 1):           # gộp các nhịp liên tiếp cùng trạng thái thành một khối
            k = j
            while k + 1 < len(o) and k + 1 <= t and o[k + 1] == o[j]:
                k += 1
            tt, nhan = o[j]
            x0, x1 = X(j), X(min(k + 1, self.T)) if k >= t else X(k + 1)
            if tt == L.CPU:
                cv.create_rectangle(x0, y + 3, x1, y + h - 3, fill=MAU[nhan], outline="")
                if x1 - x0 > 18:
                    cv.create_text((x0 + x1) / 2, y + h / 2, text=nhan, fill="white",
                                   font=("TkDefaultFont", 9, "bold"))
            elif tt == L.IO:
                cv.create_rectangle(x0, y + 6, x1, y + h - 6, fill=NHAT[nhan], outline=MAU[nhan],
                                    stipple="gray50")
                if x1 - x0 > 40:
                    cv.create_text((x0 + x1) / 2, y + h / 2, text=f"{nhan} chờ I/O", fill=CHU,
                                   font=("TkDefaultFont", 8))
            elif tt == L.SAN_SANG:
                cv.create_rectangle(x0, y + 5, x1, y + h - 5, fill="", outline=MAU[nhan], dash=(3, 2))
            elif tt == L.CHO:
                cv.create_rectangle(x0, y + 8, x1, y + h - 8, fill="#e9ecef", outline="#adb5bd", dash=(1, 2))
                if x1 - x0 > 50:
                    cv.create_text((x0 + x1) / 2, y + h / 2, text="chờ join", fill="#667085",
                                   font=("TkDefaultFont", 8))
            j = k + 1

    def ve_io(self, kq, y, h, X, t):
        cv = self.cv
        cv.create_text(104, y + h / 2, text="I/O chờ", anchor="e", fill=CHU)
        cv.create_rectangle(X(0), y + 2, X(kq.tong), y + h - 2, fill="#f6fbf3", outline="")
        n_max = max(kq.io) if kq.io else 1
        for j in range(min(len(kq.io), t + 1)):
            n = kq.io[j]
            if n:
                hh = (h - 6) * n / max(1, n_max)
                cv.create_rectangle(X(j), y + h - 3 - hh, X(j + 1), y + h - 3, fill="#7fb069", outline="")

    def ve_chu_giai(self, y):
        """Chú giải: mỗi mục một ô mẫu + chữ; vị trí tính theo độ rộng chữ thật để không chồng nhau."""
        cv = self.cv
        f = tkfont.nametofont("TkDefaultFont")
        muc = [(dict(fill=MAU[t], outline=""), f"{t} dùng CPU") for t in ["T1", "T2", "T3", "U"]]
        muc += [(dict(fill=NHAT["T1"], outline=MAU["T1"], stipple="gray50"), "bị chặn chờ I/O"),
                (dict(fill="", outline=MAU["T1"], dash=(3, 2)), "sẵn sàng, chờ lõi"),
                (dict(fill="#e9ecef", outline="#adb5bd", dash=(1, 2)), "chờ join"),
                (dict(fill="#7fb069", outline=""), "I/O bất đồng bộ (không giữ luồng)")]
        x, W = 20, max(200, cv.winfo_width())
        for kieu, chu in muc:
            if x + 23 + f.measure(chu) > W - 10:       # hết chỗ: xuống hàng
                x, y = 20, y + 20
            cv.create_rectangle(x, y - 6, x + 18, y + 6, **kieu)
            cv.create_text(x + 23, y, text=chu, anchor="w", fill=CHU)
            x += 23 + f.measure(chu) + 16

    def ve_nhat_ky(self, t):
        self.nk.delete("1.0", "end")
        for m, kq in self.ket_qua:
            if len(self.ket_qua) > 1:
                self.nk.insert("end", f"— {L.TEN_NGAN[m]} —\n")
            for tn, text in kq.nhat_ky:
                if tn <= t:
                    self.nk.insert("end", f"{L.giay(tn):>4} s  {text}\n")
        self.nk.see("end")


# ---------------------------------------------------------------------- kịch bản minh họa
def demo(app):
    root = app.root

    def cho(ms):
        end = time.monotonic() + ms / 1000
        while time.monotonic() < end:
            root.update()
            time.sleep(0.01)

    def chup(tep):
        root.update()
        subprocess.run(["import", "-window", root.wm_frame(), tep], check=True)

    cho(500)
    print("[gui] Bấm 'Đa luồng – đồng bộ', dừng ở 3.8 s: U đã chen ngang Luồng T1, "
          "hai nửa T1 chạy trên Luồng T1 và T1b", flush=True)
    app.chon(["da_dong_bo"])
    app.dang_chay = False
    app.T = 38.0
    app.ve()
    cho(300)
    chup(".gui_1.png")
    print("[gui] Bấm 'Đa luồng – bất đồng bộ', chạy hết: T2 chạy tiếp trên thợ khác sau I/O", flush=True)
    app.chon(["da_bat_dong_bo"])
    app.v["toc_do"].set("× 4")
    t0 = time.monotonic()
    n0 = 0
    while app.dang_chay and time.monotonic() - t0 < 10:
        cho(50)
        n0 += 1
    print(f"[gui] Hoạt hình chạy xong sau {time.monotonic() - t0:.1f} s thật, vẽ lại {n0} lần "
          f"(giao diện không đứng)", flush=True)
    print("[gui] Bấm 'So sánh 4 chế độ', tua tới cuối", flush=True)
    app.chon(list(L.CHE_DO))
    app.dang_chay = False
    app.T = float(app.do_dai())
    app.ve()
    cho(300)
    chup(".gui_2.png")
    for m, kq in app.ket_qua:
        s = L.chi_so(kq)
        u = L.giay(s["u_cho"]) + " s" if s["u_cho"] >= 0 else "–"
        print(f"[gui]   {L.TEN_NGAN[m]:<14} hoàn tất {L.giay(s['tong'])} s, luồng {s['luong']}, "
              f"U chờ {u}, chen ngang {s['chen']}", flush=True)
    print("[gui] Bỏ việc khẩn, bấm 'Đa luồng – đồng bộ', dừng ở 3.3 s rồi bấm 'Chen ngang U ngay'",
          flush=True)
    app.b["khan"].set(False)
    app.chon(["da_dong_bo"])
    app.dang_chay = False
    app.T = 33.0
    app.chen_ngang()
    s = L.chi_so(app.ket_qua[0][1])
    print(f"[gui]   U đến 3.3 s, chờ {L.giay(s['u_cho'])} s, chen ngang {s['chen']} lần, "
          f"hoàn tất {L.giay(s['tong'])} s", flush=True)
    subprocess.run(["convert", ".gui_1.png", ".gui_2.png", "-bordercolor", "#d0d4dc", "-border", "4",
                    "-append", "ket_qua_gui.png"], check=True)
    subprocess.run(["rm", "-f", ".gui_1.png", ".gui_2.png"])
    print("[gui] Đã chụp cửa sổ: ket_qua_gui.png", flush=True)
    root.destroy()


def main():
    root = tk.Tk()
    app = UngDung(root)
    if "--demo" in sys.argv:
        root.after(200, lambda: demo(app))
    root.mainloop()


if __name__ == "__main__":
    main()
