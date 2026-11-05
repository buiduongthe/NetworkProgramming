"""Bài mở rộng MR6.2 – Server hội thoại âm thanh có giao diện Tkinter ("Bảng điều khiển âm thanh").

Cửa sổ chỉ là lớp vỏ: mọi việc mạng/trộn tiếng do PhongAmThanh (phong_am_thanh.py) làm trên luồng
nền. Ba loại luồng:
  - luồng giao diện (mainloop của Tk): nhận thao tác chuột, vẽ lại cửa sổ – KHÔNG chờ mạng;
  - luồng mạng của lõi: chờ kết nối TCP, phục vụ từng client, nhận UDP, trộn mỗi 20 ms;
  - luồng PortAudio (nếu có sounddevice): thu micro / phát loa của máy server.
Luồng nền báo nhật ký qua HangDoiGiaoDien; luồng giao diện cứ 100 ms đọc anh_chup() một lần để vẽ
các thanh đo mức và danh sách client.

Micro thật chỉ được mở khi người dùng bấm Start Mic (không tự bật khi khởi động); chỉ báo trong
nhóm "Micro của server" cho biết đang dùng thiết bị THẬT hay GIẢ LẬP.

Chạy:  python3 server_gui.py [host] [cổng_tcp] [cổng_udp]      (mặc định 127.0.0.1 41620 41621)
"""
import math
import sys
import threading
import time
import tkinter as tk
from tkinter import ttk

from giao_dien_chung import (CAM, DO, NEN, XANH_DUONG, XANH_LA, HangDoiGiaoDien, NhatKy, ThanhMuc,
                             TreHoan, khung_nhom, nhan, nut)
from phong_am_thanh import PhongAmThanh
from thiet_bi_am_thanh import GIA_LAP, liet_ke, mo_loa, mo_mic, nap_sounddevice

TAN_SO_SERVER = 330                       # âm giả lập của micro server (Hz)


class CuaSoServer:
    def __init__(self, root, host="127.0.0.1", cong_tcp=41620, cong_udp=41621, in_nhat_ky=False):
        self.root, self.in_nhat_ky = root, in_nhat_ky
        self.sd, ly_do_sd = nap_sounddevice()
        self.phong = None                 # PhongAmThanh khi server đang chạy
        self.mic = self.loa = None        # micro / loa của máy server đang dùng
        self.hang = {}                    # mã client -> các điều khiển của hàng đó
        self.ui = HangDoiGiaoDien(root)
        root.title("Bảng điều khiển âm thanh – Server MR6.2")
        root.configure(bg=NEN)

        tren = tk.Frame(root, bg=NEN)
        tren.pack(fill="x", padx=6, pady=4)
        # ---- Kết nối
        g = khung_nhom(tren, "Kết nối")
        g.pack(side="left", fill="y", padx=3)
        nhan(g, "IP:").grid(row=0, column=0, sticky="w")
        self.o_ip = tk.Entry(g, width=14)
        self.o_ip.insert(0, host)
        self.o_ip.grid(row=0, column=1, columnspan=3, sticky="w")
        nhan(g, "TCP:").grid(row=1, column=0, sticky="w")
        self.o_tcp = tk.Entry(g, width=6)
        self.o_tcp.insert(0, str(cong_tcp))
        self.o_tcp.grid(row=1, column=1, sticky="w")
        nhan(g, "UDP:").grid(row=1, column=2, sticky="e")
        self.o_udp = tk.Entry(g, width=6)
        self.o_udp.insert(0, str(cong_udp))
        self.o_udp.grid(row=1, column=3, sticky="w")
        self.nut_start = nut(g, "Start Server", XANH_LA, self.bat_server)
        self.nut_start.grid(row=2, column=0, columnspan=2, pady=4)
        self.nut_stop = nut(g, "Stop Server", DO, self.dung_server)
        self.nut_stop.grid(row=2, column=2, columnspan=2, pady=4)
        self.nut_stop.configure(state="disabled")
        # ---- Điều khiển chung
        g = khung_nhom(tren, "Điều khiển chung")
        g.pack(side="left", fill="y", padx=3)
        self.nut_mo_tat_ca = nut(g, "Mở mic tất cả", XANH_DUONG, lambda: self.khoa_tat_ca(False), 13)
        self.nut_mo_tat_ca.grid(row=0, column=0, padx=2)
        self.nut_tat_tat_ca = nut(g, "Tắt mic tất cả", CAM, lambda: self.khoa_tat_ca(True), 13)
        self.nut_tat_tat_ca.grid(row=0, column=1, padx=2)
        self.nhan_alc = nhan(g, "Âm lượng chung: 100%")
        self.nhan_alc.grid(row=1, column=0, columnspan=2, sticky="w", pady=(6, 0))
        self.thanh_alc = tk.Scale(g, from_=0, to=200, resolution=5, orient="horizontal", length=230,
                                  showvalue=0, bg=NEN, highlightthickness=0)
        self.thanh_alc.set(100)
        self.thanh_alc.grid(row=2, column=0, columnspan=2)
        self.thanh_alc.configure(command=self._keo_alc)
        self._gui_alc = TreHoan(root, self._dat_alc)
        # ---- Micro của server
        g = khung_nhom(tren, "Micro của server")
        g.pack(side="left", fill="y", padx=3)
        self.ds_mic = [(i, f"{i}: {ten}") for i, ten in liet_ke(self.sd, thu=True)]
        self.ds_mic.append((GIA_LAP, f"Giả lập (sin {TAN_SO_SERVER} Hz)"))
        self.chon_mic = ttk.Combobox(g, state="readonly", width=34, values=[t for _, t in self.ds_mic])
        self.chon_mic.current(0)
        self.chon_mic.grid(row=0, column=0, columnspan=3, sticky="w")
        self.nut_mic_on = nut(g, "Start Mic", XANH_LA, self.bat_mic, 9)
        self.nut_mic_on.grid(row=1, column=0, pady=3, sticky="w")
        self.nut_mic_off = nut(g, "Stop Mic", DO, self.tat_mic, 9)
        self.nut_mic_off.grid(row=1, column=1, pady=3, sticky="w")
        self.nut_mic_off.configure(state="disabled")
        self.muc_mic = ThanhMuc(g, 130)
        self.muc_mic.grid(row=1, column=2, padx=4)
        self.nhan_amv = nhan(g, "Mic Volume: 100%")
        self.nhan_amv.grid(row=2, column=0, sticky="w")
        self.thanh_amv = tk.Scale(g, from_=0, to=200, resolution=5, orient="horizontal", length=200,
                                  showvalue=0, bg=NEN, highlightthickness=0, command=self._keo_amv)
        self.thanh_amv.set(100)
        self.thanh_amv.grid(row=2, column=1, columnspan=2, sticky="w")
        self.chi_bao_mic = nhan(g, "Mic server: tắt" + ("" if self.sd else " (không có sounddevice:"
                                                         " chỉ có giả lập)"), fg="#404040")
        self.chi_bao_mic.grid(row=3, column=0, columnspan=3, sticky="w")
        # ---- Nghe
        g = khung_nhom(tren, "Nghe")
        g.pack(side="left", fill="y", padx=3)
        self.bien_nghe = tk.BooleanVar(value=False)
        tk.Checkbutton(g, text="Phát ra loa server", variable=self.bien_nghe, bg=NEN,
                       activebackground=NEN, command=self.doi_nghe).grid(row=0, column=0, sticky="w")
        self.muc_nghe = ThanhMuc(g, 150)
        self.muc_nghe.grid(row=1, column=0, pady=4)
        self.chi_bao_loa = nhan(g, "Loa server: tắt", fg="#404040", wraplength=170, justify="left")
        self.chi_bao_loa.grid(row=2, column=0, sticky="w")

        # ---- Dòng trạng thái
        tt = tk.Frame(root, bg="white")
        tt.pack(fill="x")
        self.nhan_tt = tk.Label(tt, text="Server chưa chạy", bg="white", fg="#a00000",
                                font=("TkDefaultFont", 9, "bold"))
        self.nhan_tt.pack(anchor="w", padx=8)
        self.nhan_so = tk.Label(tt, text="Số client: 0", bg="white")
        self.nhan_so.pack(anchor="w", padx=8)
        if not self.sd:
            tk.Label(tt, text=f"sounddevice: {ly_do_sd}", bg="white", fg="gray40").pack(anchor="w", padx=8)

        # ---- Danh sách client: mỗi client một hàng
        self.khung_khach = khung_nhom(root, "Client trong phòng")
        self.khung_khach.pack(fill="x", padx=6, pady=4)
        dau = tk.Frame(self.khung_khach, bg=NEN)
        dau.pack(fill="x")
        for chu, rong in (("Mã / tên", 10), ("Mic", 15), ("Mức vào", 22), ("Khóa", 10),
                          ("Độ lợi (0–200 %)", 22), ("Âm lượng nghe", 14), ("Mất / jitter", 14)):
            tk.Label(dau, text=chu, width=rong, anchor="w", bg=NEN,
                     font=("TkDefaultFont", 9, "bold")).pack(side="left")
        self.nhan_trong = nhan(self.khung_khach, "(chưa có client)", fg="gray30")
        self.nhan_trong.pack(anchor="w")

        # ---- Nhật ký
        g = khung_nhom(root, "Nhật ký sự kiện")
        g.pack(fill="both", expand=True, padx=6, pady=4)
        self.nhat_ky = NhatKy(g, cao=8, rong=110)
        self.nhat_ky.pack(fill="both", expand=True)

        root.protocol("WM_DELETE_WINDOW", self.dong)
        root.after(100, self.cap_nhat)

    # ------------------------------------------------------------ nhật ký
    def ghi(self, text):                             # luồng giao diện
        self.nhat_ky.ghi(time.strftime("[%H:%M:%S] ") + text)
        if self.in_nhat_ky:
            print(f"[server-gui] {text}", flush=True)

    def bao_tu_luong_nen(self, text):                # gọi được từ mọi luồng
        self.ui.dua_vao(self.ghi, text)

    # ------------------------------------------------------------ server
    def bat_server(self):
        host = self.o_ip.get().strip()
        try:
            cong_tcp, cong_udp = int(self.o_tcp.get()), int(self.o_udp.get())
            phong = PhongAmThanh(self.bao_tu_luong_nen)
            phong.am_luong_chung = self.thanh_alc.get() / 100
            phong.am_luong_mic_server = self.thanh_amv.get() / 100
            phong.mic_server = self.mic
            if self.bien_nghe.get():
                phong.nghe = self._phat_loa
            phong.bat_dau(host, cong_tcp, cong_udp)        # mở socket: tức thời, không chặn
        except (OSError, ValueError) as e:
            self.nhan_tt.configure(text=f"Không chạy được server: {e}", fg="#a00000")
            self.ghi(f"Lỗi: {e}")
            return
        self.phong = phong
        self.nhan_tt.configure(text=f"Server đang chạy tại {phong.dia_chi}", fg="#007000")
        self.nut_start.configure(state="disabled")
        self.nut_stop.configure(state="normal")
        for o in (self.o_ip, self.o_tcp, self.o_udp):
            o.configure(state="disabled")

    def dung_server(self):
        phong, self.phong = self.phong, None
        if phong is None:
            return
        self.nut_stop.configure(state="disabled")
        self.nhan_tt.configure(text="Đang dừng server…", fg="#a06000")

        def dung_nen():                              # chờ luồng nền kết thúc: không làm trên luồng giao diện
            phong.dung()
            self.ui.dua_vao(self._da_dung)
        threading.Thread(target=dung_nen, daemon=True).start()

    def _da_dung(self):
        self.nhan_tt.configure(text="Server đã dừng", fg="#a00000")
        self.nut_start.configure(state="normal")
        for o in (self.o_ip, self.o_tcp, self.o_udp):
            o.configure(state="normal")

    # ------------------------------------------------------------ điều khiển chung
    def khoa_tat_ca(self, khoa):
        if self.phong:
            self.phong.khoa_mic(None, khoa)

    def _keo_alc(self, v):
        self.nhan_alc.configure(text=f"Âm lượng chung: {int(float(v))}%")
        self._gui_alc(int(float(v)))

    def _dat_alc(self, v):
        if self.phong and abs(self.phong.am_luong_chung - v / 100) > 1e-9:
            self.phong.dat_am_luong_chung(v / 100)

    # ------------------------------------------------------------ micro và loa của server
    def bat_mic(self):
        chi_so, ten = self.ds_mic[self.chon_mic.current()]
        nguon, loi = mo_mic(self.sd, chi_so, ten, TAN_SO_SERVER)
        if loi:
            self.ghi(loi)
        self.mic = nguon
        if self.phong:
            self.phong.dat_mic_server(nguon)
        self.chi_bao_mic.configure(text=f"Mic server: {nguon.mo_ta}",
                                   fg="#006000" if nguon.that else "#8a4b00")
        self.ghi(f"Micro server bật: {nguon.mo_ta}")
        self.nut_mic_on.configure(state="disabled")
        self.nut_mic_off.configure(state="normal")
        self.chon_mic.configure(state="disabled")

    def tat_mic(self):
        if self.phong:
            self.phong.dat_mic_server(None)
        if self.mic:
            self.mic.dong()
        self.mic = None
        self.chi_bao_mic.configure(text="Mic server: tắt", fg="#404040")
        self.ghi("Micro server tắt")
        self.nut_mic_on.configure(state="normal")
        self.nut_mic_off.configure(state="disabled")
        self.chon_mic.configure(state="readonly")

    def _keo_amv(self, v):
        self.nhan_amv.configure(text=f"Mic Volume: {int(float(v))}%")
        if self.phong:
            self.phong.dat_am_luong_mic_server(float(v) / 100)

    def _phat_loa(self, mau):                        # gọi từ luồng trộn của lõi
        loa = self.loa
        if loa is not None:
            loa.phat(mau)

    def doi_nghe(self):
        if self.bien_nghe.get():
            self.loa, ly_do = mo_loa(self.sd)
            self.chi_bao_loa.configure(text="Loa server: THẬT (thiết bị mặc định)" if self.loa
                                       else f"Loa server: {ly_do}",
                                       fg="#006000" if self.loa else "#8a4b00")
            if self.phong:
                self.phong.dat_nghe(self._phat_loa)
            self.ghi("Server nghe: " + ("phát ra loa" if self.loa else ly_do))
        else:
            if self.phong:
                self.phong.dat_nghe(None)
            if self.loa:
                self.loa.dong()
            self.loa = None
            self.chi_bao_loa.configure(text="Loa server: tắt", fg="#404040")
            self.ghi("Server ngừng nghe")

    # ------------------------------------------------------------ hàng của từng client
    def _tao_hang(self, ma, ten):
        h = tk.Frame(self.khung_khach, bg=NEN)
        h.pack(fill="x", pady=1)
        w = {"khung": h, "do_loi": 1.0}
        tk.Label(h, text=f"#{ma} {ten}", width=10, anchor="w", bg=NEN).pack(side="left")
        w["mic"] = tk.Label(h, text="", width=15, anchor="w", bg=NEN)
        w["mic"].pack(side="left")
        w["muc"] = ThanhMuc(h, 150)
        w["muc"].pack(side="left", padx=(0, 12))
        w["khoa"] = nut(h, "Khóa", CAM, lambda: self._bam_khoa(ma), 8)
        w["khoa"].pack(side="left", padx=(0, 10))
        w["nhan_loi"] = tk.Label(h, text="100%", width=5, bg=NEN)
        w["nhan_loi"].pack(side="left")
        gui_loi = TreHoan(self.root, lambda v: self._dat_do_loi(ma, v))

        def keo(v):
            w["nhan_loi"].configure(text=f"{int(float(v))}%")
            gui_loi(int(float(v)))
        w["loi"] = tk.Scale(h, from_=0, to=200, resolution=5, orient="horizontal", length=140,
                            showvalue=0, bg=NEN, highlightthickness=0)
        w["loi"].set(100)
        w["loi"].configure(command=keo)
        w["loi"].pack(side="left", padx=(0, 10))
        w["nghe"] = tk.Label(h, text="", width=12, anchor="w", bg=NEN)
        w["nghe"].pack(side="left")
        w["mat"] = tk.Label(h, text="", width=14, anchor="w", bg=NEN)
        w["mat"].pack(side="left")
        return w

    def _bam_khoa(self, ma):
        w = self.hang.get(ma)
        if self.phong and w:
            self.phong.khoa_mic(ma, not w["dang_khoa"])

    def _dat_do_loi(self, ma, v):
        w = self.hang.get(ma)
        if self.phong and w and abs(w["do_loi"] - v / 100) > 1e-9:   # bỏ lần gọi do set() ban đầu
            self.phong.dat_do_loi(ma, v / 100)

    def cap_nhat(self):
        """~10 lần/giây: đọc ảnh chụp trạng thái của lõi rồi vẽ lại (chỉ luồng giao diện làm việc này)."""
        anh = self.phong.anh_chup() if self.phong else {"khach": [], "muc_mic_server": -math.inf,
                                                        "muc_nghe": -math.inf}
        dang_co = {k["ma"] for k in anh["khach"]}
        for ma in [m for m in self.hang if m not in dang_co]:          # client đã rời
            self.hang.pop(ma)["khung"].destroy()
        for k in anh["khach"]:
            w = self.hang.get(k["ma"]) or self.hang.setdefault(k["ma"], self._tao_hang(k["ma"], k["ten"]))
            w["do_loi"], w["dang_khoa"] = k["do_loi"], k["server_khoa"]
            if k["mic_mo"]:
                chu, mau = "mở", "#006000"
            elif k["ly_do"] == "tắt (tự tắt)":
                chu, mau = "tự tắt", "#505050"
            else:
                chu, mau = "bị khóa" + (" + tự tắt" if "cả hai" in k["ly_do"] else ""), "#b00000"
            w["mic"].configure(text=chu, fg=mau)
            w["muc"].dat(k["muc"])
            w["khoa"].configure(text="Gỡ khóa" if k["server_khoa"] else "Khóa",
                                bg=XANH_LA if k["server_khoa"] else CAM)
            w["nghe"].configure(text=f"{k['am_luong_nghe'] * 100:.0f}%")
            w["mat"].configure(text=f"{k['mat']} gói / {k['jitter_ms']:.1f} ms")
        if self.hang:
            self.nhan_trong.pack_forget()
        else:
            self.nhan_trong.pack(anchor="w")
        self.nhan_so.configure(text=f"Số client: {len(anh['khach'])}")
        self.muc_mic.dat(anh["muc_mic_server"])
        self.muc_nghe.dat(anh["muc_nghe"])
        if self.ui.song:
            self.root.after(100, self.cap_nhat)

    def dong(self):
        self.ui.song = False
        if self.phong:
            self.phong.dung()
        for tb in (self.mic, self.loa):
            if tb:
                tb.dong()
        self.root.destroy()


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong_tcp = int(sys.argv[2]) if len(sys.argv) > 2 else 41620
    cong_udp = int(sys.argv[3]) if len(sys.argv) > 3 else 41621
    root = tk.Tk()
    CuaSoServer(root, host, cong_tcp, cong_udp)
    root.mainloop()


if __name__ == "__main__":
    main()
