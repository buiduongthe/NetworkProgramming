"""Bài mở rộng MR6.2 – Client hội thoại âm thanh có giao diện Tkinter.

Hai lớp tách bạch:
  - KetNoiAmThanh: phần mạng + âm thanh, KHÔNG chạm giao diện. Kết nối TCP (chạy trên luồng nền vì
    connect() có thể chờ tới 5 s), luồng đọc TCP, luồng nhận UDP và một vòng 20 ms (thu micro ->
    gửi RTP; lấy khung từ bộ đệm jitter -> nhân âm lượng nghe -> đo mức -> phát loa) – giống vòng
    lặp của khach_am_thanh.py.
  - CuaSoKhach: cửa sổ. Thao tác của người dùng gọi phương thức của KetNoiAmThanh; thông báo từ luồng
    nền đến qua HangDoiGiaoDien; mức âm được đọc 10 lần/giây để vẽ thanh đo.

Quy ước với server (không đổi so với bản dòng lệnh, chỉ thêm "do_loi"):
  - Kết nối xong mà người dùng CHƯA bấm Start Mic -> client gửi "tu_tat_mic" ngay (server mặc định
    coi mic mở). Micro thật chỉ được mở khi bấm Start Mic.
  - Start Mic: mở thiết bị (thật hoặc giả lập) rồi gửi "tu_bat_mic"; server có thể từ chối nếu đang
    khóa (quy tắc 2). Stop Mic: gửi "tu_tat_mic" (có hiệu lực ngay tại client) rồi đóng thiết bị.
  - Thanh âm lượng nghe áp dụng tại client và báo server bằng "am_luong_nghe" (quy tắc 5).
  - Server gửi {"loai": "do_loi", "gia_tri": g} -> nhãn "Server Vol: g×100 %".

Chạy:  python3 khach_gui.py [tên] [host] [cổng_tcp] [--tan-so 440]
"""
import argparse
import array
import json
import math
import socket
import threading
import time
import tkinter as tk
from tkinter import ttk

from chung import (KHUNG_MS, MAU_MOI_KHUNG, PT_AM_THANH, PT_GIU_KET_NOI, TAN_SO_MAU, BoDemJitter,
                   byte_sang_pcm, cat_ngon, dong_goi_rtp, gui_json, muc_dbfs, pcm_sang_byte, tach_rtp)
from giao_dien_chung import (DO, NEN, XANH_LA, HangDoiGiaoDien, ThanhMuc, TreHoan, khung_nhom,
                             nhan, nut)
from thiet_bi_am_thanh import GIA_LAP, liet_ke, mo_loa, mo_mic, nap_sounddevice


class KetNoiAmThanh:
    """Phần mạng và âm thanh của client; `bao(ham, *tham_so)` chuyển thông báo về luồng giao diện."""

    def __init__(self, ten, bao):
        self.ten, self.bao = ten, bao
        self.khoa = threading.Lock()
        self.khoa_tcp = threading.Lock()
        self.tcp = self.udp = None
        self.da_ket_noi = False
        self.ma = None
        self.mic_mo_server = False                  # theo thông điệp "mic" của server
        self.ly_do = "chưa kết nối"
        self.mic = None                             # nguồn đang thu; None = chưa Start Mic
        self.am_luong_mic = self.am_luong_nghe = 1.0
        self.loa, self.phat_loa = None, False
        self.server_vol = None
        self.muc_nhan = self.muc_mic = -math.inf
        self.nhan_gan_day = array.array("h")        # 1 s gần nhất đã nghe (đo theo tần số trong demo)
        self.bo_dem = BoDemJitter(do_sau=3)
        self.goi_gui = self.goi_nhan = 0
        self.song = True
        threading.Thread(target=self._vong_20ms, daemon=True).start()

    # ------------------------------------------------------------ kết nối
    def ket_noi(self, host, cong):
        """Gọi trên luồng nền (connect và chờ "chao" có thể mất vài giây). Lỗi -> ném OSError."""
        tcp = socket.create_connection((host, cong), timeout=5)
        try:
            tcp.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            f = tcp.makefile("r", encoding="utf-8")
            gui_json(tcp, {"loai": "vao", "ten": self.ten})
            chao = json.loads(f.readline() or "{}")
            if chao.get("loai") != "chao":
                raise OSError("server không trả lời 'chao'")
            tcp.settimeout(None)
            udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            udp.connect((host, chao["cong_udp"]))          # chỉ nhận gói từ đúng server
        except (OSError, ValueError, KeyError) as e:
            tcp.close()
            raise OSError(str(e)) from None
        with self.khoa:
            self.tcp, self.udp, self.ma = tcp, udp, chao["ma"]
            self.bo_dem = BoDemJitter(do_sau=3)
            self.seq = self.ts = 0
            self.goi_gui = self.goi_nhan = 0
            self.server_vol = None
            self.da_ket_noi = True
            mic_dang_bat = self.mic is not None
        udp.send(dong_goi_rtp(PT_GIU_KET_NOI, 0, 0, self.ma))   # đăng ký địa chỉ UDP ngay
        if not mic_dang_bat:
            self.gui({"loai": "tu_tat_mic"})            # chưa Start Mic: báo server mic đang tắt
        if self.am_luong_nghe != 1.0:
            self.gui({"loai": "am_luong_nghe", "gia_tri": self.am_luong_nghe})
        threading.Thread(target=self._doc_tcp, args=(f,), daemon=True).start()
        threading.Thread(target=self._nhan_udp, args=(udp,), daemon=True).start()
        return self.ma

    def ngat(self):
        with self.khoa:
            tcp, udp, self.da_ket_noi = self.tcp, self.udp, False
            self.tcp = self.udp = None
            self.mic_mo_server = False
            self.ly_do = "chưa kết nối"
        for s in (tcp, udp):
            if s:
                try:
                    if s is tcp:
                        s.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                s.close()

    def gui(self, obj):
        tcp = self.tcp
        if tcp is None:
            return
        try:
            with self.khoa_tcp:
                gui_json(tcp, obj)
        except OSError:
            pass

    def _doc_tcp(self, f):
        try:
            for dong in f:
                tin = json.loads(dong)
                loai = tin.get("loai")
                if loai == "mic":
                    with self.khoa:
                        self.mic_mo_server, self.ly_do = tin["mo"], tin["ly_do"]
                    self.bao("mic", tin["mo"], tin["ly_do"])
                elif loai == "do_loi":                    # thông điệp thêm
                    with self.khoa:
                        self.server_vol = float(tin["gia_tri"])
                    self.bao("do_loi", float(tin["gia_tri"]))
                elif loai == "su_kien":
                    self.bao("su_kien", tin["noi_dung"])
                elif loai == "tu_choi":
                    self.bao("tu_choi", tin["ly_do"])
        except (OSError, ValueError):
            pass
        if self.da_ket_noi:                               # server đóng kết nối (không phải mình ngắt)
            self.ngat()
            self.bao("mat_ket_noi", "server đã đóng kết nối")

    def _nhan_udp(self, udp):
        while True:
            try:
                goi = udp.recv(2048)
            except OSError:
                return                                    # socket đã đóng
            r = tach_rtp(goi)
            if r and r[0] == PT_AM_THANH:
                with self.khoa:
                    self.bo_dem.nhan_goi(r[1], r[2], r[4], time.monotonic())
                    self.goi_nhan += 1

    # ------------------------------------------------------------ micro, loa, âm lượng
    def bat_mic(self, nguon):
        with self.khoa:
            self.mic = nguon
        self.gui({"loai": "tu_bat_mic"})                  # xin server; có thể bị từ chối

    def tat_mic(self):
        with self.khoa:
            nguon, self.mic = self.mic, None
            self.mic_mo_server = False                    # tự tắt: hiệu lực ngay tại client
        self.gui({"loai": "tu_tat_mic"})
        if nguon:
            nguon.dong()

    def dat_loa(self, loa, phat):
        with self.khoa:
            cu, self.loa, self.phat_loa = self.loa, loa, phat
        if cu and cu is not loa:
            cu.dong()

    def dat_am_luong_nghe(self, v, bao_server=True):
        self.am_luong_nghe = v
        if bao_server:
            self.gui({"loai": "am_luong_nghe", "gia_tri": v})

    # ------------------------------------------------------------ vòng 20 ms
    def _vong_20ms(self):
        n = 0
        ke_tiep = time.monotonic()
        while self.song:
            n += 1
            with self.khoa:
                mic, mic_mo, udp = self.mic, self.mic_mo_server, self.udp
                loa, phat = self.loa, self.phat_loa
            alm, aln = self.am_luong_mic, self.am_luong_nghe
            # 1. Thu (có micro thì luôn đo mức, kể cả khi chưa kết nối) và gửi
            if mic is not None:
                khung = mic.lay_khung()
                if alm != 1.0:
                    khung = [cat_ngon(round(x * alm)) for x in khung]
                self.muc_mic = muc_dbfs(khung)
            else:
                self.muc_mic = -math.inf
            if udp is not None:
                try:
                    if mic is not None and mic_mo:
                        udp.send(dong_goi_rtp(PT_AM_THANH, self.seq, self.ts, self.ma, pcm_sang_byte(khung)))
                        self.seq += 1
                        self.goi_gui += 1
                    elif n % 50 == 0:
                        udp.send(dong_goi_rtp(PT_GIU_KET_NOI, 0, 0, self.ma))   # giữ kết nối
                except OSError:
                    pass
                self.ts += MAU_MOI_KHUNG
            # 2. Nhận, nhân âm lượng nghe, đo mức, phát
            with self.khoa:
                p = self.bo_dem.lay() if udp is not None else None
            mau = byte_sang_pcm(p) if p else array.array("h", bytes(2 * MAU_MOI_KHUNG))
            if aln != 1.0:
                mau = array.array("h", (cat_ngon(round(x * aln)) for x in mau))
            self.muc_nhan = muc_dbfs(mau)
            with self.khoa:
                self.nhan_gan_day.extend(mau)
                del self.nhan_gan_day[:-TAN_SO_MAU]
            if loa is not None and phat:
                loa.phat(mau)
            ke_tiep += KHUNG_MS / 1000
            time.sleep(max(0.0, ke_tiep - time.monotonic()))

    def dong(self):
        self.song = False
        self.ngat()
        for tb in (self.mic, self.loa):
            if tb:
                tb.dong()


class CuaSoKhach:
    def __init__(self, root, ten="A", host="127.0.0.1", cong=41620, tan_so=440.0):
        self.root, self.tan_so = root, tan_so
        self.sd, ly_do_sd = nap_sounddevice()
        self.ui = HangDoiGiaoDien(root)
        self.lop = None                              # KetNoiAmThanh, tạo khi biết tên
        root.title(f"Client âm thanh – MR6.2 ({ten})")
        root.configure(bg=NEN)
        root.resizable(False, False)

        # ---- Kết nối
        g = khung_nhom(root, "Kết nối")
        g.pack(fill="x", padx=8, pady=4)
        nhan(g, "IP:").grid(row=0, column=0, sticky="w")
        self.o_ip = tk.Entry(g, width=15)
        self.o_ip.insert(0, host)
        self.o_ip.grid(row=0, column=1, sticky="w")
        nhan(g, "Cổng:").grid(row=0, column=2, sticky="e", padx=(8, 0))
        self.o_cong = tk.Entry(g, width=7)
        self.o_cong.insert(0, str(cong))
        self.o_cong.grid(row=0, column=3, sticky="w")
        nhan(g, "Tên:").grid(row=0, column=4, sticky="e", padx=(8, 0))
        self.o_ten = tk.Entry(g, width=8)
        self.o_ten.insert(0, ten)
        self.o_ten.grid(row=0, column=5, sticky="w")
        self.nut_ket_noi = nut(g, "Connect", XANH_LA, self.bam_ket_noi, 14)
        self.nut_ket_noi.grid(row=1, column=0, columnspan=6, pady=6)

        # ---- Nhận âm thanh
        g = khung_nhom(root, "Nhận âm thanh")
        g.pack(fill="x", padx=8, pady=4)
        self.nhan_aln = nhan(g, "Âm lượng nghe: 100%", width=20, anchor="w")
        self.nhan_aln.grid(row=0, column=0, sticky="w")
        self.thanh_aln = tk.Scale(g, from_=0, to=200, resolution=5, orient="horizontal", length=250,
                                  showvalue=0, bg=NEN, highlightthickness=0)
        self.thanh_aln.set(100)
        self.thanh_aln.grid(row=0, column=1, columnspan=2, sticky="w")
        self.thanh_aln.configure(command=self._keo_aln)
        self._gui_aln = TreHoan(root, self._bao_aln)
        self.nut_audio_on = nut(g, "Start Audio", XANH_LA, self.bat_audio)
        self.nut_audio_on.grid(row=1, column=0, pady=4, sticky="w")
        self.nut_audio_off = nut(g, "Stop Audio", DO, self.tat_audio)
        self.nut_audio_off.grid(row=1, column=1, pady=4, sticky="w")
        self.nut_audio_off.configure(state="disabled")
        self.nhan_server_vol = nhan(g, "Server Vol: --", font=("TkDefaultFont", 9, "bold"))
        self.nhan_server_vol.grid(row=1, column=2, sticky="e")
        nhan(g, "Mức nhận:").grid(row=2, column=0, sticky="w")
        self.muc_nhan = ThanhMuc(g, 250)
        self.muc_nhan.grid(row=2, column=1, columnspan=2, sticky="w")
        self.chi_bao_loa = nhan(g, "Loa: tắt", fg="#404040")
        self.chi_bao_loa.grid(row=3, column=0, columnspan=3, sticky="w")

        # ---- Micro
        g = khung_nhom(root, "Micro")
        g.pack(fill="x", padx=8, pady=4)
        self.ds_mic = [(i, f"{i}: {t}") for i, t in liet_ke(self.sd, thu=True)]
        self.ds_mic.append((GIA_LAP, f"Giả lập (sin {tan_so:g} Hz)"))
        self.chon_mic = ttk.Combobox(g, state="readonly", width=44, values=[t for _, t in self.ds_mic])
        self.chon_mic.current(0)
        self.chon_mic.grid(row=0, column=0, columnspan=3, sticky="w")
        self.nut_mic_on = nut(g, "Start Mic", XANH_LA, self.bat_mic)
        self.nut_mic_on.grid(row=1, column=0, pady=4, sticky="w")
        self.nut_mic_off = nut(g, "Stop Mic", DO, self.tat_mic)
        self.nut_mic_off.grid(row=1, column=1, pady=4, sticky="w")
        self.nut_mic_off.configure(state="disabled")
        self.nhan_mic_server = nhan(g, "Mic (server): --")
        self.nhan_mic_server.grid(row=1, column=2, sticky="e")
        self.nhan_amv = nhan(g, "Mic Volume: 100%", width=20, anchor="w")
        self.nhan_amv.grid(row=2, column=0, sticky="w")
        self.thanh_amv = tk.Scale(g, from_=0, to=200, resolution=5, orient="horizontal", length=250,
                                  showvalue=0, bg=NEN, highlightthickness=0, command=self._keo_amv)
        self.thanh_amv.set(100)
        self.thanh_amv.grid(row=2, column=1, columnspan=2, sticky="w")
        nhan(g, "Mức micro:").grid(row=3, column=0, sticky="w")
        self.muc_mic = ThanhMuc(g, 250)
        self.muc_mic.grid(row=3, column=1, columnspan=2, sticky="w")
        self.chi_bao_mic = nhan(g, "Micro: tắt" + ("" if self.sd else " (không có sounddevice: chỉ có"
                                                   " giả lập)"), fg="#404040")
        self.chi_bao_mic.grid(row=4, column=0, columnspan=3, sticky="w")

        # ---- Trạng thái và sự kiện
        self.nhan_tt = tk.Label(root, text="Chưa kết nối", bg="white", fg="#a00000", anchor="w",
                                font=("TkDefaultFont", 9, "bold"))
        self.nhan_tt.pack(fill="x", padx=8, pady=(4, 2))
        self.su_kien = tk.Listbox(root, height=5, font=("TkFixedFont", 9))
        self.su_kien.pack(fill="x", padx=8, pady=(0, 8))
        if not self.sd:
            self.su_kien.insert("end", f"sounddevice: {ly_do_sd[:60]}")

        root.protocol("WM_DELETE_WINDOW", self.dong)
        root.after(100, self.cap_nhat)

    # ------------------------------------------------------------ cầu nối luồng nền -> giao diện
    def _bao(self, loai, *ts):                       # gọi từ luồng nền của KetNoiAmThanh
        self.ui.dua_vao(self._xu_ly_bao, loai, *ts)

    def _xu_ly_bao(self, loai, *ts):                 # luồng giao diện
        if loai == "mic":
            mo, ly_do = ts
            self.nhan_mic_server.configure(text=f"Mic (server): {ly_do}",
                                           fg="#006000" if mo else "#b00000")
            self._trang_thai(f"Đã kết nối – mã {self.lop.ma}; mic {ly_do}", "#007000")
        elif loai == "do_loi":
            self.nhan_server_vol.configure(text=f"Server Vol: {ts[0] * 100:.0f}%")
        elif loai == "su_kien":
            self.ghi_su_kien(ts[0])
        elif loai == "tu_choi":
            self._trang_thai(f"Bị từ chối: {ts[0]}", "#b00000")
            self.ghi_su_kien(f"Bị từ chối: {ts[0]}")
        elif loai == "mat_ket_noi":
            self._da_ngat(f"Mất kết nối: {ts[0]}")

    def ghi_su_kien(self, text):
        self.su_kien.insert("end", time.strftime("%H:%M:%S ") + text)
        self.su_kien.see("end")

    def _trang_thai(self, text, mau):
        self.nhan_tt.configure(text=text, fg=mau)

    # ------------------------------------------------------------ kết nối
    def bam_ket_noi(self):
        if self.lop and self.lop.da_ket_noi:
            self.lop.ngat()
            self._da_ngat("Đã ngắt kết nối")
            return
        ten = self.o_ten.get().strip() or "?"
        host = self.o_ip.get().strip()
        try:
            cong = int(self.o_cong.get())
        except ValueError:
            self._trang_thai("Cổng không hợp lệ", "#b00000")
            return
        if self.lop is None or self.lop.ten != ten:
            if self.lop:
                self.lop.dong()
            self.lop = KetNoiAmThanh(ten, self._bao)
            self.lop.am_luong_nghe = self.thanh_aln.get() / 100
            self.lop.am_luong_mic = self.thanh_amv.get() / 100
        self.nut_ket_noi.configure(state="disabled", text="Đang kết nối…")
        self._trang_thai(f"Đang kết nối {host}:{cong}…", "#a06000")
        lop = self.lop

        def ket_noi_nen():                            # connect() có thể chờ: chạy trên luồng nền
            try:
                ma = lop.ket_noi(host, cong)
                self.ui.dua_vao(self._da_ket_noi, ma)
            except OSError as e:
                self.ui.dua_vao(self._da_ngat, f"Không kết nối được: {e}")
        threading.Thread(target=ket_noi_nen, daemon=True).start()

    def _da_ket_noi(self, ma):
        self.nut_ket_noi.configure(state="normal", text="Disconnect", bg=XANH_LA)
        self._trang_thai(f"Đã kết nối – mã {ma}; mic {self.lop.ly_do}", "#007000")
        for o in (self.o_ip, self.o_cong, self.o_ten):
            o.configure(state="disabled")

    def _da_ngat(self, text):
        self.nut_ket_noi.configure(state="normal", text="Connect")
        self._trang_thai(text, "#b00000")
        self.nhan_server_vol.configure(text="Server Vol: --")
        self.nhan_mic_server.configure(text="Mic (server): --", fg="black")
        for o in (self.o_ip, self.o_cong, self.o_ten):
            o.configure(state="normal")

    def _can_lop(self):
        """Micro/loa dùng được cả trước khi kết nối (để thử mức micro)."""
        if self.lop is None:
            self.lop = KetNoiAmThanh(self.o_ten.get().strip() or "?", self._bao)
            self.lop.am_luong_nghe = self.thanh_aln.get() / 100
            self.lop.am_luong_mic = self.thanh_amv.get() / 100
        return self.lop

    # ------------------------------------------------------------ nhận âm thanh
    def _keo_aln(self, v):
        self.nhan_aln.configure(text=f"Âm lượng nghe: {int(float(v))}%")
        if self.lop:
            self.lop.dat_am_luong_nghe(float(v) / 100, bao_server=False)   # áp dụng ngay tại client
        self._gui_aln(float(v) / 100)

    def _bao_aln(self, v):
        if self.lop:
            self.lop.gui({"loai": "am_luong_nghe", "gia_tri": v})        # báo server (trễ 200 ms)

    def bat_audio(self):
        loa, ly_do = mo_loa(self.sd)
        self._can_lop().dat_loa(loa, True)
        self.chi_bao_loa.configure(text="Loa: THẬT (thiết bị mặc định)" if loa else f"Loa: {ly_do}",
                                   fg="#006000" if loa else "#8a4b00")
        self.nut_audio_on.configure(state="disabled")
        self.nut_audio_off.configure(state="normal")

    def tat_audio(self):
        if self.lop:
            self.lop.dat_loa(None, False)
        self.chi_bao_loa.configure(text="Loa: tắt", fg="#404040")
        self.nut_audio_on.configure(state="normal")
        self.nut_audio_off.configure(state="disabled")

    # ------------------------------------------------------------ micro
    def bat_mic(self):
        chi_so, ten = self.ds_mic[self.chon_mic.current()]
        nguon, loi = mo_mic(self.sd, chi_so, ten, self.tan_so)
        if loi:
            self.ghi_su_kien(loi)
        self._can_lop().bat_mic(nguon)
        self.chi_bao_mic.configure(text=f"Micro: {nguon.mo_ta}", fg="#006000" if nguon.that else "#8a4b00")
        self.nut_mic_on.configure(state="disabled")
        self.nut_mic_off.configure(state="normal")
        self.chon_mic.configure(state="disabled")

    def tat_mic(self):
        if self.lop:
            self.lop.tat_mic()
        self.chi_bao_mic.configure(text="Micro: tắt", fg="#404040")
        self.nut_mic_on.configure(state="normal")
        self.nut_mic_off.configure(state="disabled")
        self.chon_mic.configure(state="readonly")

    def _keo_amv(self, v):
        self.nhan_amv.configure(text=f"Mic Volume: {int(float(v))}%")
        if self.lop:
            self.lop.am_luong_mic = float(v) / 100

    # ------------------------------------------------------------ vẽ lại định kỳ
    def cap_nhat(self):
        lop = self.lop
        self.muc_nhan.dat(lop.muc_nhan if lop else -math.inf)
        self.muc_mic.dat(lop.muc_mic if lop else -math.inf)
        if self.ui.song:
            self.root.after(100, self.cap_nhat)

    def dong(self):
        self.ui.song = False
        if self.lop:
            self.lop.dong()
        self.root.destroy()


def main():
    ap = argparse.ArgumentParser(description="Client hội thoại âm thanh có giao diện")
    ap.add_argument("ten", nargs="?", default="A")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41620)
    ap.add_argument("--tan-so", type=float, default=440.0, help="tần số của micro giả lập (Hz)")
    a = ap.parse_args()
    root = tk.Tk()
    CuaSoKhach(root, a.ten, a.host, a.cong, a.tan_so)
    root.mainloop()


if __name__ == "__main__":
    main()
