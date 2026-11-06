"""Bài mở rộng MR6.5 – Lõi máy phát (camera + micro -> máy chủ), dạng lớp, không chạm giao diện.

Dùng chung cho phat.py (dòng lệnh) và phat_gui.py. Báo sự kiện bằng `bao(text)`; giao diện đọc
số liệu (xem trước, mức micro, thống kê, danh sách người xem) dưới khóa.

Đồng hồ phương tiện chung: t = 0 khi bấm Phát (đồng hồ đơn điệu time.monotonic).
  - Âm thanh: luồng 20 ms; gói thứ n được gửi lúc t0 + n·20 ms (bám đồng hồ, không trôi), nhãn RTP
    ts = n·320 (= số mẫu đã gửi, 16 kHz).
  - Video: luồng theo fps; nhãn = ms phương tiện lúc chụp khung. Khung 0 là I; I lặp lại mỗi `gop`
    khung hoặc NGAY khung kế tiếp sau khi nhận xin_khung_khoa (đếm GOP lại từ khung I đó).
  - Mỗi 1 s gửi {"loai":"dong_ho","ts":ms phương tiện,"gio":ms Unix} (như RTCP Sender Report –
    RFC 3550 mục 6.4.1): người xem ánh xạ nhãn thời gian <-> giờ thực để đo độ trễ đầu-cuối.
"""
import math
import random
import socket
import threading
import time

from camera import CameraGiaLap
from chung import (CAO, FPS_MAC_DINH, GOP_MAC_DINH, KHUNG_MS, LOAI_I, LOAI_P, MAU_MOI_KHUNG, RONG,
                   TAN_SO_MAU, chia_manh, doc_dong, dong_goi_rtp, goi_giu, gui_json, ma_hoa_i, ma_hoa_p,
                   muc_dbfs, pcm_sang_byte)


class MayPhat:
    def __init__(self, ten, bao=None, mat_goi=0.0):
        self.ten, self.bao = ten, bao or (lambda text: None)
        self.khoa, self.khoa_tcp = threading.Lock(), threading.Lock()
        self.tcp = self.udp = None
        self.da_ket_noi, self.ma = False, None
        self.dang_phat = False
        self.camera = self.mic = None
        self.fps, self.gop = FPS_MAC_DINH, GOP_MAC_DINH
        self.nguoi_xem = []                       # tên người xem (theo thông điệp nguoi_xem)
        self.xem_truoc = None                     # (w, h, rgb, ts) khung vừa gửi
        self.muc_mic = -math.inf
        self.mat_goi, self.rng = mat_goi, random.Random(1)
        self.bo_gia_lap = 0
        self.t0, self.so, self.seq, self.n_am = 0.0, 0, 0, 0
        self.so_i_cuoi, self.truoc = None, None
        self.can_i, self.ly_do_i = False, ""
        self._lam_moi_thong_ke()
        self.song = True
        threading.Thread(target=self._vong_am, daemon=True).start()
        threading.Thread(target=self._vong_hinh, daemon=True).start()

    def _lam_moi_thong_ke(self):
        self.so_i = self.so_p = self.byte_i = self.byte_p = self.goi_am = self.pli_nhan = 0
        self.p_min = self.p_max = None
        self.dem_i = self.dem_p = self.dem_byte_am = self.dem_byte_hinh = 0
        self.kbps_am = self.kbps_hinh = 0.0

    # ------------------------------------------------------------ kết nối
    def ket_noi(self, host, cong):
        """Kết nối TCP, chào với vai trò "phat". Lỗi (kể cả server từ chối) -> OSError."""
        tcp = socket.create_connection((host, cong), timeout=5)
        try:
            tcp.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            tep = tcp.makefile("rb")
            gui_json(tcp, {"loai": "chao", "vai_tro": "phat", "ten": self.ten})
            tl = doc_dong(tep) or {}
            if tl.get("loai") == "loi":
                raise OSError(f"server từ chối: {tl.get('ly_do')}")
            if tl.get("loai") != "chao":
                raise OSError("server không trả lời 'chao'")
            tcp.settimeout(None)
            udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            if hasattr(socket, "SIO_UDP_CONNRESET"):   # Windows: tắt lỗi 10054 ở recvfrom (ICMP cổng đóng)
                udp.ioctl(socket.SIO_UDP_CONNRESET, False)
            udp.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 1 << 20)
            udp.connect((host, tl["cong_udp"]))
        except (OSError, ValueError, KeyError) as e:
            tcp.close()
            raise OSError(str(e)) from None
        with self.khoa:
            self.tcp, self.udp, self.ma, self.da_ket_noi = tcp, udp, tl["ma"], True
        udp.send(goi_giu(self.ma))                       # đăng ký địa chỉ UDP ngay
        threading.Thread(target=self._doc_tcp, args=(tep,), daemon=True).start()
        return self.ma

    def gui(self, obj):
        tcp = self.tcp
        if tcp is None:
            return
        try:
            with self.khoa_tcp:
                gui_json(tcp, obj)
        except OSError:
            pass

    def _doc_tcp(self, tep):
        try:
            while True:
                tin = doc_dong(tep)
                if tin is None:
                    break
                loai = tin.get("loai")
                if loai == "xin_khung_khoa":
                    with self.khoa:
                        self.pli_nhan += 1
                        self.ly_do_i = f"PLI từ {tin.get('tu', '?')}"
                        self.can_i = True
                elif loai == "nguoi_xem":
                    with self.khoa:
                        self.nguoi_xem = [str(t) for t in tin.get("ds", [])]
                elif loai == "su_kien":
                    self.bao(f"Sự kiện: {tin.get('noi_dung')}")
                elif loai == "loi":
                    self.bao(f"Lỗi từ server: {tin.get('ly_do')}")
        except (OSError, ValueError):
            pass
        if self.da_ket_noi:
            self.bao("Mất kết nối server")
            self.ngat()

    def ngat(self):
        with self.khoa:
            tcp, udp = self.tcp, self.udp
            self.tcp = self.udp = None
            self.da_ket_noi = False
            self.dang_phat = False
            self.nguoi_xem = []
        for s in (tcp, udp):
            if s:
                try:
                    if s is tcp:
                        s.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                s.close()

    # ------------------------------------------------------------ phát / dừng
    def _thong_tin(self):
        return {"loai": "thong_tin", "rong": RONG, "cao": CAO, "fps": self.fps, "gop": self.gop,
                "tan_so_mau": TAN_SO_MAU}

    def bat_dau_phat(self, camera, mic, fps=None, gop=None):
        with self.khoa:
            self.camera, self.mic = camera, mic
            self.fps = max(1, min(30, int(fps or self.fps)))
            self.gop = max(1, min(300, int(gop or self.gop)))
            tin = self._thong_tin()
        # thong_tin đi TCP, gói phương tiện đi UDP trên luồng khác: gửi thong_tin trước và chờ 20 ms để máy
        # chủ kịp chuyển sang "đang phát" – nếu không, khung I #0 có thể tới trước, bị bỏ vì "chưa phát",
        # và bộ đệm GOP rỗng tới khung I kế tiếp.
        self.gui(tin)
        time.sleep(0.02)
        with self.khoa:
            self.t0 = time.monotonic()
            self.so = self.seq = self.n_am = 0
            self.so_i_cuoi, self.truoc = None, None
            self.can_i, self.ly_do_i = False, ""
            self._lam_moi_thong_ke()
            self.dang_phat = True
            if hasattr(camera, "dat_fps"):
                camera.dat_fps(self.fps)
        self.bao(f"Bắt đầu phát {RONG}x{CAO} {self.fps} fps GOP {self.gop}")

    def dat_tham_so(self, fps, gop):
        with self.khoa:
            self.fps, self.gop = max(1, min(30, int(fps))), max(1, min(300, int(gop)))
            if self.camera and hasattr(self.camera, "dat_fps"):
                self.camera.dat_fps(self.fps)
            phat = self.dang_phat
        if phat:
            self.gui(self._thong_tin())
        self.bao(f"Tham số: {self.fps} fps, GOP {self.gop}")

    def khung_khoa_ngay(self, ly_do="lệnh khung khóa"):
        with self.khoa:
            self.can_i, self.ly_do_i = True, ly_do

    def dung_phat(self):
        with self.khoa:
            if not self.dang_phat:
                return
            self.dang_phat = False
            cam, mic, self.camera, self.mic = self.camera, self.mic, None, None
        self.gui({"loai": "dung_phat"})
        for tb in (cam, mic):
            if tb:
                tb.dong()
        self.bao("Dừng phát")

    def _gui_udp(self, goi):
        """Gửi một datagram; --mat-goi giả lập mạng làm rơi gói ở chiều lên."""
        if self.mat_goi and self.rng.random() < self.mat_goi:
            self.bo_gia_lap += 1
            return
        udp = self.udp
        if udp:
            try:
                udp.send(goi)
            except OSError:
                pass

    # ------------------------------------------------------------ luồng âm thanh 20 ms
    def _vong_am(self):
        han_giu = time.monotonic()
        while self.song:
            bay_gio = time.monotonic()
            with self.khoa:
                phat, mic, ma, udp = self.dang_phat, self.mic, self.ma, self.udp
                t0 = self.t0
            if udp is not None and bay_gio >= han_giu:
                self._gui_udp(goi_giu(ma))                # giữ kết nối mỗi 1 s
                han_giu = bay_gio + 1.0
            if not (phat and mic and udp):
                self.muc_mic = -math.inf
                time.sleep(0.02)
                continue
            han = t0 + self.n_am * KHUNG_MS / 1000
            if bay_gio < han:
                time.sleep(min(han - bay_gio, 0.02))
                continue
            if self.n_am % 50 == 0:                        # báo cáo đồng hồ mỗi 1 s
                self.gui({"loai": "dong_ho", "ts": int((bay_gio - t0) * 1000), "gio": int(time.time() * 1000)})
            khung = mic.lay_khung()
            self.muc_mic = muc_dbfs(khung)
            goi = dong_goi_rtp(self.seq, self.n_am * MAU_MOI_KHUNG, ma, pcm_sang_byte(khung))
            self._gui_udp(goi)
            with self.khoa:
                self.seq += 1
                self.n_am += 1
                self.goi_am += 1
                self.dem_byte_am += len(goi)

    # ------------------------------------------------------------ luồng video theo fps
    def _vong_hinh(self):
        han = None
        while self.song:
            bay_gio = time.monotonic()
            with self.khoa:
                phat, cam, ma, udp = self.dang_phat, self.camera, self.ma, self.udp
                t0, fps = self.t0, self.fps
            if not (phat and cam and udp):
                han = None
                time.sleep(0.01)
                continue
            if han is None or han < t0:
                han = t0
            if bay_gio < han:
                time.sleep(min(han - bay_gio, 0.01))
                continue
            han += 1 / fps
            if han < bay_gio - 0.2:                        # tụt quá xa (máy bận): bắt nhịp lại
                han = bay_gio
            if getattr(cam, "loi", ""):                    # ffmpeg dừng/lỗi: lùi về giả lập
                self.bao(f"Camera lỗi, dùng giả lập: {cam.loi[:28]}")
                cam.dong()
                cam = CameraGiaLap(self.ten, ma or 1, fps)
                with self.khoa:
                    self.camera = cam
            ts_ms = int((bay_gio - t0) * 1000)
            anh = cam.lay_khung(self.so, ts_ms)
            if anh is None or len(anh) != RONG * CAO * 3:
                continue                                   # camera chưa có khung
            with self.khoa:
                so, gop = self.so, self.gop
                la_i = (self.so_i_cuoi is None or so - self.so_i_cuoi >= gop or self.can_i)
                ly_do = self.ly_do_i if self.can_i else ""
                self.can_i, self.ly_do_i = False, ""
            if la_i:
                loai, du_lieu = LOAI_I, ma_hoa_i(anh)
            else:
                loai, du_lieu = LOAI_P, ma_hoa_p(anh, self.truoc)
            manh = chia_manh(loai, ma, so, RONG, CAO, ts_ms, du_lieu)
            for g in manh:
                self._gui_udp(g)
            if ly_do:
                self.bao(f"{ly_do} -> gửi khung I #{so}")
            with self.khoa:
                self.truoc = bytes(anh)
                self.so += 1
                if la_i:
                    self.so_i_cuoi = so
                    self.so_i += 1
                    self.byte_i += len(du_lieu)
                    self.dem_i += 1
                else:
                    self.so_p += 1
                    self.byte_p += len(du_lieu)
                    self.p_min = len(du_lieu) if self.p_min is None else min(self.p_min, len(du_lieu))
                    self.p_max = len(du_lieu) if self.p_max is None else max(self.p_max, len(du_lieu))
                    self.dem_p += 1
                self.dem_byte_hinh += sum(len(g) for g in manh)
                self.xem_truoc = (RONG, CAO, self.truoc, ts_ms)

    # ------------------------------------------------------------ số liệu
    def thong_ke_giay(self):
        """Gọi mỗi 1 s (dòng lệnh hoặc giao diện): trả về và xóa bộ đếm của giây vừa qua."""
        with self.khoa:
            r = {"i": self.dem_i, "p": self.dem_p, "kbps_am": self.dem_byte_am * 8 / 1000,
                 "kbps_hinh": self.dem_byte_hinh * 8 / 1000}
            self.kbps_am, self.kbps_hinh = r["kbps_am"], r["kbps_hinh"]
            self.dem_i = self.dem_p = self.dem_byte_am = self.dem_byte_hinh = 0
        return r

    def trang_thai(self):
        with self.khoa:
            return {"dang_phat": self.dang_phat, "da_ket_noi": self.da_ket_noi, "ma": self.ma,
                    "so_i": self.so_i, "so_p": self.so_p,
                    "tb_i": self.byte_i / self.so_i if self.so_i else 0.0,
                    "tb_p": self.byte_p / self.so_p if self.so_p else 0.0,
                    "p_min": self.p_min or 0, "p_max": self.p_max or 0, "goi_am": self.goi_am, "pli_nhan": self.pli_nhan, "kbps_am": self.kbps_am,
                    "kbps_hinh": self.kbps_hinh, "nguoi_xem": list(self.nguoi_xem),
                    "muc_mic": self.muc_mic, "xem_truoc": self.xem_truoc, "fps": self.fps, "gop": self.gop,
                    "camera": self.camera.mo_ta if self.camera else "", "mic": self.mic.mo_ta if self.mic else ""}

    def dong(self):
        self.dung_phat()
        self.song = False
        self.ngat()


def dong_tong_ket(tt, giay):
    """Các dòng tổng kết của máy phát (dùng chung cho dòng lệnh)."""
    return [f"===== TỔNG KẾT sau {giay:.1f} s =====",
            f"khung I {tt['so_i']}, P {tt['so_p']}; TB I {tt['tb_i']:.0f} B, P {tt['tb_p']:.0f} B",
            f"P nhỏ nhất {tt['p_min']} B, lớn nhất {tt['p_max']} B (khung chớp)",
            f"I/thô {tt['tb_i'] / (RONG * CAO * 3) * 100:.1f} %, P TB/I TB"
            f" {tt['tb_p'] / tt['tb_i'] * 100 if tt['tb_i'] else 0:.1f} %",
            f"tiếng {tt['goi_am']} gói; PLI nhận {tt['pli_nhan']}"]

