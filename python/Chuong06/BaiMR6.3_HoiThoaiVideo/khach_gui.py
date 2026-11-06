"""Bài mở rộng MR6.3 – Client hội thoại video có giao diện Tkinter ("Camera Client").

Hai lớp tách bạch:
  - KetNoiVideo: mạng + camera, KHÔNG chạm giao diện. Luồng đọc TCP, luồng nhận UDP (ghép mảnh lưới
    server gửi), luồng gửi (lấy khung camera theo fps server cho phép -> thu nhỏ 1/chia -> nén zlib ->
    chia mảnh ≤ 1200 byte -> UDP) – đúng đường gửi của khach_video.py.
  - CuaSoKhach: cửa sổ – khung xem trước camera của mình, khung "lưới nhận từ server", thanh trạng
    thái fps/kbit/s. Cứ 100 ms đọc khung mới nhất của KetNoiVideo rồi vẽ.
Quy ước (giao thức không đổi): kết nối xong mà chưa bấm START -> gửi "tu_tat_camera"; START mở camera
(thật qua ffmpeg, mẫu thử testsrc hoặc giả lập) rồi gửi "tu_bat_camera" (server có thể từ chối);
CANCEL gửi "tu_tat_camera" và đóng camera. Camera thật chỉ được mở khi bấm START.

Chạy:  python3 khach_gui.py [tên] [host] [cổng_tcp]
"""
import json
import socket
import sys
import threading
import time
import tkinter as tk
import zlib
from tkinter import ttk

from camera import GIA_LAP, MAU_THU, co_ffmpeg, liet_ke_camera, mo_camera
from chung import (CAO, LOAI_MANH, RONG, GhepKhung, chia_manh, giai_nen, goi_giu, gui_json,
                   tach_header, thu_nho)
from giao_dien_chung import NEN, HangDoiGiaoDien, anh_tk

FPS_CAMERA = 15                                   # camera chạy 15 fps; gửi theo giới hạn của server


class KetNoiVideo:
    def __init__(self, ten, bao):
        self.ten, self.bao = ten, bao             # bao(loại, …) gọi từ luồng nền
        self.khoa, self.khoa_tcp = threading.Lock(), threading.Lock()
        self.tcp = self.udp = None
        self.da_ket_noi, self.ma = False, None
        self.cam_mo_server, self.fps_server, self.chia = False, 10, 1
        self.nhan_fps, self.nhan_chia = 10, 1
        self.camera = None                        # nguồn đang chạy; None = chưa START
        self.xem_truoc = None                     # (w, h, rgb) khung vừa gửi
        self.luoi = None                          # (w, h, rgb) lưới mới nhất nhận từ server
        self.so_luoi = 0
        self.fps_gui = self.kbps_gui = self.fps_nhan = self.kbps_nhan = 0.0
        self.ghep = GhepKhung()
        self.song = True
        threading.Thread(target=self._vong_gui, daemon=True).start()

    # ------------------------------------------------------------ kết nối
    def ket_noi(self, host, cong):
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
            if hasattr(socket, "SIO_UDP_CONNRESET"):   # Windows: tắt lỗi 10054 ở recvfrom (ICMP cổng đóng)
                udp.ioctl(socket.SIO_UDP_CONNRESET, False)
            udp.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1 << 20)
            udp.connect((host, chao["cong_udp"]))
        except (OSError, ValueError, KeyError) as e:
            tcp.close()
            raise OSError(str(e)) from None
        with self.khoa:
            self.tcp, self.udp, self.ma = tcp, udp, chao["ma"]
            self.ghep, self.luoi, self.so_khung = GhepKhung(), None, 0
            self.da_ket_noi = True
            cam = self.camera
        udp.send(goi_giu(self.ma))                 # đăng ký địa chỉ UDP
        if cam is None:
            self.gui({"loai": "tu_tat_camera"})    # chưa START: báo server camera đang tắt
        threading.Thread(target=self._doc_tcp, args=(f,), daemon=True).start()
        threading.Thread(target=self._nhan_udp, args=(udp,), daemon=True).start()
        if self.nhan_fps != 10 or self.nhan_chia != 1:
            self.gui({"loai": "nhan", "fps": self.nhan_fps, "chia": self.nhan_chia})
        return self.ma

    def ngat(self):
        with self.khoa:
            tcp, udp, self.da_ket_noi = self.tcp, self.udp, False
            self.tcp = self.udp = None
            self.cam_mo_server = False
        self.fps_gui = self.kbps_gui = self.fps_nhan = self.kbps_nhan = 0.0
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
                if loai == "camera":
                    with self.khoa:
                        self.cam_mo_server, self.fps_server, self.chia = tin["mo"], tin["fps"], tin["chia"]
                    self.bao("camera", tin["mo"], tin["ly_do"], tin["fps"], tin["chia"])
                elif loai == "nhan":
                    self.bao("nhan", tin["fps"], tin["chia"])
                elif loai == "su_kien":
                    self.bao("su_kien", tin["noi_dung"])
                elif loai == "tu_choi":
                    self.bao("tu_choi", tin["ly_do"])
        except (OSError, ValueError):
            pass
        if self.da_ket_noi:
            self.ngat()
            self.bao("mat_ket_noi", "server đã đóng kết nối")

    def _nhan_udp(self, udp):
        dem, byte, moc = 0, 0, time.monotonic()
        while True:
            try:
                goi = udp.recv(2048)
            except OSError:
                return
            h = tach_header(goi)
            if h is None or h[0] != LOAI_MANH:
                continue
            _, _, so, i, tong, w, cao, payload = h
            khung = self.ghep.them(so, i, tong, w, cao, payload, time.monotonic())
            if khung:
                try:
                    anh = giai_nen(khung[2])
                except zlib.error:
                    continue
                if len(anh) != w * cao * 3:
                    continue
                dem, byte = dem + 1, byte + len(khung[2])
                with self.khoa:
                    self.luoi = (w, cao, anh)
                    self.so_luoi += 1
            if time.monotonic() - moc >= 1.0 and self.da_ket_noi:
                self.fps_nhan, self.kbps_nhan = dem, byte * 8 / 1000
                dem, byte, moc = 0, 0, moc + 1.0

    # ------------------------------------------------------------ camera, tham số nhận
    def bat_camera(self, cam):
        with self.khoa:
            self.camera = cam
        self.gui({"loai": "tu_bat_camera"})

    def tat_camera(self):
        with self.khoa:
            cam, self.camera = self.camera, None
            self.cam_mo_server = False             # tự tắt: hiệu lực ngay
            self.xem_truoc = None
        self.gui({"loai": "tu_tat_camera"})
        if cam:
            cam.dong()

    def dat_nhan(self, fps=None, chia=None):
        if fps is not None:
            self.nhan_fps = fps
            self.gui({"loai": "nhan", "fps": fps})
        if chia is not None:
            self.nhan_chia = chia
            self.gui({"loai": "nhan", "chia": chia})

    # ------------------------------------------------------------ vòng gửi
    def _vong_gui(self):
        so_khung = 0
        han_gui = moc = han_giu = time.monotonic()
        dem = byte = 0
        while self.song:
            bay_gio = time.monotonic()
            with self.khoa:
                cam, mo, udp = self.camera, self.cam_mo_server, self.udp
                fps, chia, ma = min(FPS_CAMERA, self.fps_server), self.chia, self.ma
            if cam is not None and bay_gio >= han_gui:
                han_gui = max(han_gui + 1 / fps, bay_gio)
                anh = cam.lay_khung(so_khung)
                if anh is not None:
                    anh, w, h = thu_nho(anh, RONG, CAO, chia)      # giảm độ phân giải trước khi nén
                    with self.khoa:
                        self.xem_truoc = (w, h, anh)
                    if udp is not None and mo:
                        nen = zlib.compress(anh, 1)
                        try:
                            for g in chia_manh(ma, so_khung, w, h, nen):
                                udp.send(g)
                        except OSError:
                            pass
                        dem, byte = dem + 1, byte + len(nen)
                    so_khung += 1
            if udp is not None and not (cam is not None and mo) and bay_gio >= han_giu:
                try:
                    udp.send(goi_giu(ma))                        # giữ kết nối khi không gửi hình
                except OSError:
                    pass
                han_giu = bay_gio + 1.0
            if bay_gio - moc >= 1.0:
                self.fps_gui, self.kbps_gui = (dem, byte * 8 / 1000) if self.da_ket_noi else (0, 0.0)
                dem = byte = 0
                moc += 1.0
            time.sleep(0.003)

    def dong(self):
        self.song = False
        self.ngat()
        if self.camera:
            self.camera.dong()


class CuaSoKhach:
    def __init__(self, root, ten="A", host="127.0.0.1", cong=41630):
        self.root = root
        self.ui = HangDoiGiaoDien(root)
        self.lop = None
        self.anh_xem = self.anh_luoi = None              # giữ tham chiếu PhotoImage (Tk không tự giữ)
        self.so_luoi_da_ve = -1
        root.title(f"Camera Client – MR6.3 ({ten})")
        root.configure(bg=NEN)
        root.resizable(False, False)

        tren = tk.Frame(root, bg=NEN)
        tren.pack(fill="x", padx=6, pady=4)
        g = tk.LabelFrame(tren, text="Kết nối server", bg=NEN)
        g.pack(side="left", fill="y")
        tk.Label(g, text="Server IP:", bg=NEN).grid(row=0, column=0, sticky="w")
        self.o_ip = tk.Entry(g, width=13)
        self.o_ip.insert(0, host)
        self.o_ip.grid(row=0, column=1, sticky="w")
        tk.Label(g, text="Port:", bg=NEN).grid(row=1, column=0, sticky="w")
        self.o_cong = tk.Entry(g, width=7)
        self.o_cong.insert(0, str(cong))
        self.o_cong.grid(row=1, column=1, sticky="w")
        tk.Label(g, text="Tên:", bg=NEN).grid(row=2, column=0, sticky="w")
        self.o_ten = tk.Entry(g, width=8)
        self.o_ten.insert(0, ten)
        self.o_ten.grid(row=2, column=1, sticky="w")
        self.nut_ket_noi = tk.Button(g, text="Kết nối", width=13, command=self.bam_ket_noi)
        self.nut_ket_noi.grid(row=3, column=0, columnspan=2, pady=3)

        g = tk.LabelFrame(tren, text="Camera", bg=NEN)
        g.pack(side="left", fill="both", expand=True, padx=(6, 0))
        tk.Label(g, text="Chọn camera:", bg=NEN).grid(row=0, column=0, columnspan=2, sticky="w")
        self.ds_cam = list(liet_ke_camera())
        if co_ffmpeg():
            self.ds_cam.append((MAU_THU, "Mẫu thử ffmpeg (testsrc)"))
        self.ds_cam.append((GIA_LAP, "Camera giả lập"))
        self.chon_cam = ttk.Combobox(g, state="readonly", width=30, values=[t for _, t in self.ds_cam])
        self.chon_cam.current(0)
        self.chon_cam.grid(row=1, column=0, columnspan=2, sticky="w")
        self.nut_start = tk.Button(g, text="START", width=10, command=self.bat_camera)
        self.nut_start.grid(row=2, column=0, pady=3, sticky="w")
        self.nut_cancel = tk.Button(g, text="CANCEL", width=10, command=self.tat_camera, state="disabled")
        self.nut_cancel.grid(row=2, column=1, pady=3, sticky="w")
        self.chi_bao_cam = tk.Label(g, text="Camera: tắt" + ("" if co_ffmpeg() else " (không có ffmpeg:"
                                    " chỉ có giả lập)"), bg=NEN, fg="#404040")
        self.chi_bao_cam.grid(row=3, column=0, columnspan=2, sticky="w")
        self.nhan_cam_server = tk.Label(g, text="Server: –", bg=NEN)
        self.nhan_cam_server.grid(row=4, column=0, columnspan=2, sticky="w")

        giua = tk.Frame(root, bg=NEN)
        giua.pack(padx=6, pady=2)
        g = tk.LabelFrame(giua, text="Camera của tôi (xem trước, đúng độ phân giải gửi)", bg=NEN)
        g.pack(side="left", padx=(0, 6))
        self.xem = tk.Canvas(g, width=2 * RONG, height=2 * CAO, bg="black", highlightthickness=0)
        self.xem.pack()
        self.muc_xem = self.xem.create_image(0, 0, anchor="nw")
        g = tk.LabelFrame(giua, text="Lưới nhận từ server", bg=NEN)
        g.pack(side="left")
        self.luoi = tk.Canvas(g, width=2 * RONG, height=2 * CAO, bg="black", highlightthickness=0)
        self.luoi.pack()
        self.muc_luoi = self.luoi.create_image(RONG, CAO, anchor="center")
        f = tk.Frame(g, bg=NEN)
        f.pack(fill="x")
        tk.Label(f, text="fps nhận:", bg=NEN).pack(side="left")
        self.spin_fps = tk.Spinbox(f, from_=1, to=15, width=4, command=self.ap_fps)
        self.spin_fps.delete(0, "end")
        self.spin_fps.insert(0, "10")
        self.spin_fps.pack(side="left")
        tk.Label(f, text="  độ phân giải:", bg=NEN).pack(side="left")
        self.chon_chia = ttk.Combobox(f, state="readonly", width=5, values=["1/1", "1/2"])
        self.chon_chia.current(0)
        self.chon_chia.pack(side="left")
        self.chon_chia.bind("<<ComboboxSelected>>", lambda e: self.ap_chia())
        self.nhan_luoi = tk.Label(g, text="", bg=NEN, anchor="w")
        self.nhan_luoi.pack(fill="x")

        self.thanh_tt = tk.Label(root, text="Trạng thái: chưa kết nối", anchor="w", relief="sunken", bg="white")
        self.thanh_tt.pack(fill="x", padx=6, pady=(2, 6))
        self.nhan_tt = "chưa kết nối"
        root.protocol("WM_DELETE_WINDOW", self.dong)
        root.after(100, self.cap_nhat)

    def _bao(self, loai, *ts):
        self.ui.dua_vao(self._xu_ly_bao, loai, *ts)

    def _xu_ly_bao(self, loai, *ts):
        if loai == "camera":
            mo, ly_do, fps, chia = ts
            self.nhan_cam_server.configure(text=f"Server: camera {ly_do}, tối đa {fps} fps, 1/{chia}",
                                           fg="#006000" if mo else "#b00000")
        elif loai == "nhan":
            self.nhan_luoi.configure(text=f"Server gửi lưới {ts[0]} fps, độ phân giải 1/{ts[1]}")
        elif loai == "su_kien":
            pass                                          # sự kiện chung: đã có trong nhật ký server
        elif loai == "tu_choi":
            self.nhan_tt = f"bị từ chối – {ts[0]}"
        elif loai == "mat_ket_noi":
            self._da_ngat(f"mất kết nối – {ts[0]}")

    # ------------------------------------------------------------ kết nối
    def bam_ket_noi(self):
        if self.lop and self.lop.da_ket_noi:
            self.lop.ngat()
            self._da_ngat("đã ngắt kết nối")
            return
        ten = self.o_ten.get().strip() or "?"
        host = self.o_ip.get().strip()
        try:
            cong = int(self.o_cong.get())
        except ValueError:
            self.nhan_tt = "cổng không hợp lệ"
            return
        if self.lop is None or self.lop.ten != ten:
            if self.lop:
                self.lop.dong()
            self.lop = KetNoiVideo(ten, self._bao)
        self.nut_ket_noi.configure(state="disabled", text="Đang kết nối…")
        self.nhan_tt = f"đang kết nối {host}:{cong}…"
        lop = self.lop

        def ket_noi_nen():
            try:
                self.ui.dua_vao(self._da_ket_noi, lop.ket_noi(host, cong))
            except OSError as e:
                self.ui.dua_vao(self._da_ngat, f"không kết nối được – {e}")
        threading.Thread(target=ket_noi_nen, daemon=True).start()

    def _da_ket_noi(self, ma):
        self.nut_ket_noi.configure(state="normal", text="Ngắt kết nối")
        self.nhan_tt = f"đã kết nối, mã {ma}"
        for o in (self.o_ip, self.o_cong, self.o_ten):
            o.configure(state="disabled")

    def _da_ngat(self, text):
        self.nut_ket_noi.configure(state="normal", text="Kết nối")
        self.nhan_tt = text
        self.nhan_cam_server.configure(text="Server: –", fg="black")
        for o in (self.o_ip, self.o_cong, self.o_ten):
            o.configure(state="normal")

    def _can_lop(self):
        if self.lop is None:
            self.lop = KetNoiVideo(self.o_ten.get().strip() or "?", self._bao)
        return self.lop

    # ------------------------------------------------------------ camera
    def bat_camera(self):
        ma_tb, ten_tb = self.ds_cam[self.chon_cam.current()]
        lop = self._can_lop()
        cam = mo_camera(ma_tb, ten_tb, lop.ten, lop.ma or 1)
        lop.bat_camera(cam)
        self.chi_bao_cam.configure(text=f"Camera: {cam.mo_ta}", fg="#006000" if cam.that else "#8a4b00")
        self.nut_start.configure(state="disabled")
        self.nut_cancel.configure(state="normal")
        self.chon_cam.configure(state="disabled")

    def tat_camera(self):
        if self.lop:
            self.lop.tat_camera()
        self.chi_bao_cam.configure(text="Camera: tắt", fg="#404040")
        self.nut_start.configure(state="normal")
        self.nut_cancel.configure(state="disabled")
        self.chon_cam.configure(state="readonly")
        self.xem.itemconfigure(self.muc_xem, image="")

    def ap_fps(self):
        try:
            n = max(1, min(15, int(self.spin_fps.get())))
        except ValueError:
            return
        self._can_lop().dat_nhan(fps=n)

    def ap_chia(self):
        self._can_lop().dat_nhan(chia=(1, 2)[self.chon_chia.current()])

    # ------------------------------------------------------------ vẽ lại định kỳ
    def cap_nhat(self):
        lop = self.lop
        if lop:
            with lop.khoa:
                xem, luoi, so_luoi, cam = lop.xem_truoc, lop.luoi, lop.so_luoi, lop.camera
            if xem is not None:                           # phóng hình đang gửi về 320×240 để xem
                w, h, rgb = xem
                self.anh_xem = anh_tk(rgb, w, h, 2 * RONG // w)
                self.xem.itemconfigure(self.muc_xem, image=self.anh_xem)
            if luoi is not None and so_luoi != self.so_luoi_da_ve:
                w, h, rgb = luoi
                k = max(1, min(2 * RONG // w, 2 * CAO // h))   # vừa khung 320×240
                if w * k > 2 * RONG or h * k > 2 * CAO:
                    k = 1
                self.anh_luoi = anh_tk(rgb, w, h, k)
                self.luoi.itemconfigure(self.muc_luoi, image=self.anh_luoi)
                self.so_luoi_da_ve = so_luoi
                self.kich_thuoc_luoi = (w, h)
            if cam is not None and cam.loi:
                self.chi_bao_cam.configure(text=f"Camera lỗi: {cam.loi[:60]}", fg="#b00000")
            gui = (f"Đang truyền dữ liệu camera… gửi {lop.fps_gui:.0f} fps {lop.kbps_gui:.0f} kbit/s"
                   if lop.fps_gui > 0 and lop.da_ket_noi else self.nhan_tt)
            self.thanh_tt.configure(text=f"Trạng thái: {gui} | nhận {lop.fps_nhan:.0f} fps "
                                    f"{lop.kbps_nhan:.0f} kbit/s")
        else:
            self.thanh_tt.configure(text=f"Trạng thái: {self.nhan_tt}")
        if self.ui.song:
            self.root.after(100, self.cap_nhat)

    def dong(self):
        self.ui.song = False
        if self.lop:
            self.lop.dong()
        self.root.destroy()


def main():
    ten = sys.argv[1] if len(sys.argv) > 1 else "A"
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    cong = int(sys.argv[3]) if len(sys.argv) > 3 else 41630
    root = tk.Tk()
    CuaSoKhach(root, ten, host, cong)
    root.mainloop()


if __name__ == "__main__":
    main()
