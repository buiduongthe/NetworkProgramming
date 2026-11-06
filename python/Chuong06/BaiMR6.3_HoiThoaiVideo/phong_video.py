"""Bài mở rộng MR6.3 – Lõi server phòng hội thoại video dạng lớp (dùng cho server_gui.py).

Cùng giao thức (không thêm thông điệp nào) và cùng 6 quy tắc với server_video.py (xem docstring ở
đó): server MCU giải nén khung của mọi client, ghép lưới, nén lại và gửi cho từng người nhận.
Khác bản dòng lệnh: trạng thái nằm trong đối tượng PhongVideo (Start/Stop nhiều lần); các luồng
dùng socket có timeout và cờ `chay` để dừng gọn; cửa sổ đọc ô hình của từng client bằng anh_chup()
để hiện lưới camera ngay trên server.
"""
import itertools
import json
import socket
import threading
import time
import zlib

from chung import (CAO, LOAI_MANH, MA_SERVER, RONG, GhepKhung, chia_manh, giai_nen, gui_json,
                   phong_to, tach_header, thu_nho, to_mau, ve_chu)
from server_video import ghep_luoi          # dùng lại hàm ghép lưới của bản dòng lệnh

FPS_MAC_DINH = 10


class KhachVideo:
    """Một client trong phòng (giống lớp Khach của server_video.py)."""

    def __init__(self, phong, ma, ten, conn, ip):
        self.phong, self.ma, self.ten, self.conn, self.ip = phong, ma, ten, conn, ip
        self.khoa_gui = threading.Lock()
        self.udp = None
        self.tu_tat = self.server_tat = False
        self.fps_toi_da, self.chia = FPS_MAC_DINH, 1      # giới hạn gửi do server đặt
        self.nhan_fps, self.nhan_chia = FPS_MAC_DINH, 1   # tham số nhận do client chọn
        self.ghep = GhepKhung()
        self.o = None                                     # ô 160×120 có nhãn (dùng ghép lưới)
        self.so_o = 0                                     # tăng mỗi lần ô đổi (cửa sổ biết khi nào vẽ lại)
        self.luc_nhan_cuoi = 0.0
        self.bo_vuot_fps = self.bo_tat = self.sai_nguon = 0
        self.dem_khung = self.dem_byte = 0
        self.fps_vao = self.kbps_vao = 0.0
        self.so_khung_ra = 0
        self.han_gui_ra = 0.0
        self.dem_ra = self.byte_ra = 0
        self.fps_ra = self.kbps_ra = 0.0

    @property
    def cam_mo(self):                                     # quy tắc 1
        return not self.tu_tat and not self.server_tat

    def ly_do(self):
        if self.cam_mo:
            return "mở"
        if self.tu_tat and self.server_tat:
            return "tắt (cả hai)"
        return "tắt (server)" if self.server_tat else "tắt (tự tắt)"

    def gui(self, obj):
        try:
            with self.khoa_gui:
                gui_json(self.conn, obj)
        except OSError:
            pass

    def gui_camera(self):
        self.gui({"loai": "camera", "mo": self.cam_mo, "ly_do": self.ly_do(),
                  "fps": self.fps_toi_da, "chia": self.chia})
        self.ve_o()

    def gui_nhan(self):
        self.gui({"loai": "nhan", "fps": min(self.nhan_fps, self.phong.fps_chung), "chia": self.nhan_chia})

    def ve_o(self, khung=None):
        if khung is not None:
            w, h, anh = khung
            o = bytearray(phong_to(anh, w, h, RONG // w))
        elif self.cam_mo and self.o is not None:
            o = self.o
        else:
            o = bytearray(bytes((40, 40, 40)) * (RONG * CAO))
            if not self.cam_mo:
                ve_chu(o, RONG, CAO, 20, 44, "TAT CAMERA", 2)
                ve_chu(o, RONG, CAO, 44, 66, "(SERVER)" if self.server_tat else "(TU TAT)", 2)
        to_mau(o, RONG, 0, CAO - 11, RONG, 11, (0, 0, 0))
        nhan = f"{self.ten} {self.fps_toi_da}FPS" + (f" 1/{self.chia}" if self.chia > 1 else "")
        ve_chu(o, RONG, CAO, 2, CAO - 9, nhan, 1, (255, 255, 0))
        self.o = o
        self.so_o += 1


class PhongVideo:
    def __init__(self, bao=None):
        self.bao = bao or (lambda text: None)
        self.khoa = threading.Lock()
        self.khach = {}
        self.fps_chung = FPS_MAC_DINH
        self.chay = False
        self.dia_chi = ""
        self._luong, self._socks = [], []

    # ------------------------------------------------------------ khởi động / dừng
    def bat_dau(self, host, cong_tcp, cong_udp):
        srv = socket.create_server((host, cong_tcp))
        udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        if hasattr(socket, "SIO_UDP_CONNRESET"):   # Windows: tắt lỗi 10054 ở recvfrom (ICMP cổng đóng)
            udp.ioctl(socket.SIO_UDP_CONNRESET, False)
        try:
            udp.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1 << 20)
            udp.bind((host, cong_udp))
        except OSError:
            srv.close()
            udp.close()
            raise
        srv.settimeout(0.5)
        udp.settimeout(0.5)
        self.cong_udp, self.ma_moi, self._socks = cong_udp, itertools.count(1), [srv, udp]
        self.chay = True
        self.dia_chi = f"{host}:{cong_tcp} (UDP {cong_udp})"
        self._luong = [threading.Thread(target=h, args=(s,), daemon=True)
                       for h, s in ((self._nhan_ket_noi, srv), (self._nhan_udp, udp), (self._vong_gui, udp))]
        for t in self._luong:
            t.start()
        self.bao(f"Server chạy: TCP {host}:{cong_tcp} điều khiển, UDP {host}:{cong_udp} video")

    def dung(self):
        if not self.chay:
            return
        self.chay = False
        with self.khoa:
            ds = list(self.khach.values())
        for k in ds:
            try:
                k.conn.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        for t in self._luong:
            t.join(timeout=2)
        for s in self._socks:
            s.close()
        self.bao("Server đã dừng")

    # ------------------------------------------------------------ TCP
    def _phat_su_kien(self, text):
        self.bao(f"=> {text}")
        for k in list(self.khach.values()):
            k.gui({"loai": "su_kien", "noi_dung": text})

    def _nhan_ket_noi(self, srv):
        while self.chay:
            try:
                conn, addr = srv.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            conn.settimeout(None)
            conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            threading.Thread(target=self._phuc_vu, args=(conn, addr), daemon=True).start()

    def _phuc_vu(self, conn, addr):
        f = conn.makefile("r", encoding="utf-8")
        k = None
        try:
            tin = json.loads(f.readline() or "{}")
            if tin.get("loai") != "vao" or not tin.get("ten"):
                return
            with self.khoa:
                k = KhachVideo(self, next(self.ma_moi), str(tin["ten"])[:8], conn, addr[0])
                self.khach[k.ma] = k
            k.gui({"loai": "chao", "ma": k.ma, "cong_udp": self.cong_udp, "rong": RONG, "cao": CAO})
            self.bao(f"#{k.ma} {k.ten} vào phòng từ {addr[0]}:{addr[1]}")
            with self.khoa:
                k.gui_camera()
                k.gui_nhan()
                self._phat_su_kien(f"{k.ten} vào phòng")
            for dong in f:
                tin = json.loads(dong)
                with self.khoa:
                    self._xu_ly_tin(k, tin)
        except (OSError, ValueError):
            pass
        finally:
            if k:
                with self.khoa:
                    self.khach.pop(k.ma, None)
                    self.bao(f"#{k.ma} {k.ten} rời phòng")
                    self._phat_su_kien(f"{k.ten} rời phòng")
            conn.close()

    def _xu_ly_tin(self, k, tin):
        loai = tin.get("loai")
        if loai == "tu_tat_camera":
            k.tu_tat = True
            k.gui_camera()
            self._phat_su_kien(f"{k.ten} tự tắt camera")
        elif loai == "tu_bat_camera":
            k.tu_tat = False
            if k.server_tat:                                          # quy tắc 2
                self.bao(f"{k.ten} xin bật camera -> từ chối (server đang tắt)")
                k.gui({"loai": "tu_choi", "ly_do": "server đang tắt camera của bạn"})
            else:
                k.gui_camera()
                self._phat_su_kien(f"{k.ten} tự bật camera")
        elif loai == "nhan":                                          # quy tắc 5
            k.nhan_fps = max(1, min(30, int(tin.get("fps", k.nhan_fps))))
            k.nhan_chia = 2 if int(tin.get("chia", k.nhan_chia)) == 2 else 1
            self.bao(f"{k.ten} chọn nhận {k.nhan_fps} fps, độ phân giải 1/{k.nhan_chia}")
            k.gui_nhan()

    # ------------------------------------------------------------ UDP vào, lưới ra
    def _nhan_udp(self, sock):
        while self.chay:
            try:
                goi, addr = sock.recvfrom(2048)
            except socket.timeout:
                continue
            except OSError:
                break
            h = tach_header(goi)
            if h is None:
                continue
            loai, ma, so, i, tong, w, cao, payload = h
            bay_gio = time.monotonic()
            with self.khoa:
                k = self.khach.get(ma)
                if k is None:
                    continue
                if k.udp is None and addr[0] == k.ip:
                    k.udp = addr
                    self.bao(f"#{k.ma} {k.ten} đăng ký UDP {addr[0]}:{addr[1]}")
                if addr != k.udp:                                     # quy tắc 6
                    k.sai_nguon += 1
                    continue
                if loai != LOAI_MANH:
                    continue
                khung = k.ghep.them(so, i, tong, w, cao, payload, bay_gio)
                if khung is None:
                    continue
                if not k.cam_mo:
                    k.bo_tat += 1
                    continue
                if bay_gio - k.luc_nhan_cuoi < 0.8 / k.fps_toi_da:
                    k.bo_vuot_fps += 1
                    continue
            try:
                anh = giai_nen(khung[2])                              # giải nén ngoài khóa
            except zlib.error:
                continue
            if w * cao * 3 != len(anh) or RONG % w or w * CAO != cao * RONG:
                continue
            with self.khoa:
                k.luc_nhan_cuoi = bay_gio
                k.dem_khung += 1
                k.dem_byte += len(khung[2])
                k.ve_o((w, cao, anh))

    def _vong_gui(self, sock):
        moc_do = time.monotonic()
        while self.chay:
            bay_gio = time.monotonic()
            with self.khoa:
                ds = sorted((k for k in self.khach.values() if k.o is not None), key=lambda k: k.ma)
                for k in self.khach.values():
                    fps = min(k.nhan_fps, self.fps_chung)
                    if k.udp is None or not ds or bay_gio < k.han_gui_ra:
                        continue
                    k.han_gui_ra = max(k.han_gui_ra + 1 / fps, bay_gio)
                    anh, w, h = ghep_luoi(ds)
                    anh, w, h = thu_nho(anh, w, h, k.nhan_chia)
                    nen = zlib.compress(anh, 1)
                    for g in chia_manh(MA_SERVER, k.so_khung_ra, w, h, nen):
                        try:
                            sock.sendto(g, k.udp)
                        except OSError:
                            pass
                    k.so_khung_ra += 1
                    k.dem_ra += 1
                    k.byte_ra += len(nen)
                if bay_gio - moc_do >= 1.0:
                    for k in self.khach.values():
                        k.fps_vao, k.kbps_vao = k.dem_khung, k.dem_byte * 8 / 1000
                        k.fps_ra, k.kbps_ra = k.dem_ra, k.byte_ra * 8 / 1000
                        k.dem_khung = k.dem_byte = k.dem_ra = k.byte_ra = 0
                    moc_do += 1.0
            time.sleep(0.005)

    # ------------------------------------------------------------ lệnh của bảng điều khiển
    def _chon(self, ma):
        return [k for k in self.khach.values() if ma is None or k.ma == ma]

    def dat_camera(self, ma, tat):
        with self.khoa:
            ds = self._chon(ma)
            for k in ds:
                k.server_tat = tat
                k.gui_camera()
            ai = "tất cả" if ma is None else ", ".join(k.ten for k in ds)
            if tat:
                self._phat_su_kien(f"server tắt camera {ai}")
            else:
                van_tat = [k.ten for k in ds if k.tu_tat]
                them = f" ({', '.join(van_tat)} vẫn tự tắt)" if van_tat else ""
                self._phat_su_kien(f"server bật camera {ai}{them}")

    def dat_fps_toi_da(self, ma, n):
        with self.khoa:
            for k in self._chon(ma):
                k.fps_toi_da = max(1, min(30, int(n)))
                k.gui_camera()
                self._phat_su_kien(f"server giới hạn {k.ten} gửi tối đa {k.fps_toi_da} fps")

    def dat_do_phan_giai(self, ma, chia):
        with self.khoa:
            for k in self._chon(ma):
                k.chia = chia
                k.gui_camera()
                self._phat_su_kien(f"server đặt độ phân giải {k.ten} = 1/{chia}")

    def dat_fps_chung(self, n):
        with self.khoa:
            self.fps_chung = max(1, min(30, int(n)))
            for k in self.khach.values():
                k.gui_nhan()
            self._phat_su_kien(f"server đặt fps chung tối đa {self.fps_chung}")

    def anh_chup(self, so_o_da_ve=None):
        """Trạng thái cho giao diện; ô hình chỉ được sao chép khi đã đổi so với lần vẽ trước."""
        so_o_da_ve = so_o_da_ve or {}
        with self.khoa:
            return [{"ma": k.ma, "ten": k.ten, "ly_do": k.ly_do(), "cam_mo": k.cam_mo,
                     "server_tat": k.server_tat, "fps_toi_da": k.fps_toi_da, "chia": k.chia,
                     "fps_vao": k.fps_vao, "kbps_vao": k.kbps_vao, "fps_ra": k.fps_ra,
                     "kbps_ra": k.kbps_ra, "so_o": k.so_o, "manh_mat": k.ghep.manh_mat,
                     "o": bytes(k.o) if k.o is not None and so_o_da_ve.get(k.ma) != k.so_o else None}
                    for k in sorted(self.khach.values(), key=lambda k: k.ma)]
