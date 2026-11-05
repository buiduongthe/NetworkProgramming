"""Bài mở rộng MR6.2 – Lõi server phòng hội thoại âm thanh dạng lớp (dùng cho server_gui.py).

Cùng giao thức và cùng 6 quy tắc quyền điều khiển với server_am_thanh.py (xem docstring ở đó).
Khác bản dòng lệnh:
  - Trạng thái gói trong một đối tượng PhongAmThanh thay vì biến toàn cục, nên cửa sổ có thể
    Start/Stop server nhiều lần. Các luồng nền dùng socket có timeout 0,5 s và kiểm tra cờ `chay`
    để dừng gọn (cách dừng luồng an toàn – xem Bài MR6.1).
  - Server có micro riêng: một nguồn nữa trong bộ trộn với mã SSRC_SERVER = 0; mọi client nghe
    thấy (tiếng server không đi qua mạng tới chính server).
  - Server có thể nghe: phát ra loa máy server tổng tiếng các client (đã nhân độ lợi và âm lượng
    chung), KHÔNG gồm tiếng chính server – tức là mix-minus cho "người nghe" server.
  - Thông điệp THÊM, tương thích ngược (client cũ bỏ qua loại lạ):
        server -> client  {"loai": "do_loi", "gia_tri": g}
    gửi một lần ngay sau "chao" và mỗi khi server đổi độ lợi đầu vào của client đó; client hiện
    "Server Vol: g×100 %".
Lớp không biết gì về giao diện: báo nhật ký qua hàm bao(text) – được gọi từ luồng nền, cửa sổ tự
chuyển về luồng giao diện; cửa sổ đọc số liệu bằng anh_chup() (chụp trạng thái dưới khóa).
"""
import array
import itertools
import json
import math
import socket
import threading
import time

from chung import (BYTE_MOI_KHUNG, KHUNG_MS, MAU_MOI_KHUNG, PT_AM_THANH, SSRC_SERVER, TAN_SO_MAU,
                   BoDemJitter, byte_sang_pcm, dong_goi_rtp, gui_json, muc_dbfs, pcm_sang_byte,
                   tach_rtp)


def tru_va_cat(tong, rieng, he_so):
    """Mix-minus cho một người nghe: (T - tiếng của chính họ) × hệ số, làm tròn, cắt ngọn 16 bit.
    Trả về (array('h'), số mẫu bị cắt ngọn)."""
    ra = array.array("h", bytes(2 * MAU_MOI_KHUNG))
    cat = 0
    for i in range(MAU_MOI_KHUNG):
        v = round((tong[i] - (rieng[i] if rieng else 0.0)) * he_so)
        if v > 32767 or v < -32768:
            cat += 1
            v = 32767 if v > 0 else -32768
        ra[i] = v
    return ra, cat


class KhachPhong:
    """Một client trong phòng (giống lớp Khach của server_am_thanh.py)."""

    def __init__(self, ma, ten, conn, ip):
        self.ma, self.ten, self.conn, self.ip = ma, ten, conn, ip
        self.khoa_gui = threading.Lock()     # nhiều luồng có thể cùng gửi trên TCP
        self.udp = None                      # địa chỉ UDP học được từ gói đầu tiên
        self.tu_tat = False                  # client tự tắt mic
        self.server_khoa = False             # server khóa mic
        self.do_loi = 1.0                    # độ lợi đầu vào (gain) do server đặt
        self.am_luong_nghe = 1.0             # client báo lên (chỉ để hiển thị)
        self.bo_dem = BoDemJitter(do_sau=3)
        self.seq_ra = self.ts_ra = 0
        self.muc_vao = -math.inf
        self.bi_chan = self.sai_nguon = self.cat_ngon = 0

    @property
    def mic_mo(self):                        # quy tắc 1
        return not self.tu_tat and not self.server_khoa

    def ly_do(self):
        if self.mic_mo:
            return "mở"
        if self.tu_tat and self.server_khoa:
            return "tắt (cả hai)"
        return "tắt (server)" if self.server_khoa else "tắt (tự tắt)"

    def gui(self, obj):
        try:
            with self.khoa_gui:
                gui_json(self.conn, obj)
        except OSError:
            pass

    def gui_mic(self):
        self.gui({"loai": "mic", "mo": self.mic_mo, "ly_do": self.ly_do()})
        if not self.mic_mo:
            self.bo_dem.xoa()


class PhongAmThanh:
    def __init__(self, bao=None):
        self.bao = bao or (lambda text: None)        # hàm ghi nhật ký (gọi từ luồng nền)
        self.khoa = threading.Lock()                 # bảo vệ mọi trạng thái bên dưới
        self.khach = {}                              # mã client (SSRC) -> KhachPhong
        self.am_luong_chung = 1.0
        self.mic_server = None                       # nguồn có lay_khung(); None = server không nói
        self.am_luong_mic_server = 1.0
        self.nghe = None                             # hàm nhận khung 320 mẫu; None = server không nghe
        self.muc_mic_server = self.muc_nghe = -math.inf
        self.nghe_gan_day = array.array("h")         # 1 s gần nhất server nghe (để đo theo tần số)
        self.chay = False
        self.dia_chi = ""
        self._luong, self._socks = [], []

    # ------------------------------------------------------------ khởi động / dừng
    def bat_dau(self, host, cong_tcp, cong_udp):
        """Mở socket và chạy 3 luồng nền; lỗi (cổng bận…) ném OSError cho cửa sổ báo."""
        srv = socket.create_server((host, cong_tcp))
        udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        try:
            udp.bind((host, cong_udp))
        except OSError:
            srv.close()
            udp.close()
            raise
        srv.settimeout(0.5)                          # để luồng chờ kết nối kiểm tra cờ `chay`
        udp.settimeout(0.5)
        self.cong_udp, self.ma_moi = cong_udp, itertools.count(1)
        self._socks = [srv, udp]
        self.chay = True
        self.dia_chi = f"{host}:{cong_tcp} (UDP {cong_udp})"
        self._luong = [threading.Thread(target=ham, args=(s,), daemon=True)
                       for ham, s in ((self._nhan_ket_noi, srv), (self._nhan_udp, udp),
                                      (self._vong_tron, udp))]
        for t in self._luong:
            t.start()
        self.bao(f"Server chạy: TCP {host}:{cong_tcp} điều khiển, UDP {host}:{cong_udp} âm thanh RTP")

    def dung(self):
        if not self.chay:
            return
        self.chay = False
        with self.khoa:
            ds = list(self.khach.values())
        for k in ds:                                 # đóng kênh TCP -> luồng phục vụ client kết thúc
            try:
                k.conn.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        for t in self._luong:
            t.join(timeout=2)
        for s in self._socks:
            s.close()
        self.bao("Server đã dừng")

    # ------------------------------------------------------------ kênh điều khiển TCP
    def _phat_su_kien(self, text):
        """Báo sự kiện cho mọi client (gọi khi đang giữ khóa)."""
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
                k = KhachPhong(next(self.ma_moi), str(tin["ten"])[:8], conn, addr[0])
                self.khach[k.ma] = k
            k.gui({"loai": "chao", "ma": k.ma, "cong_udp": self.cong_udp,
                   "tan_so_mau": TAN_SO_MAU, "khung_ms": KHUNG_MS})
            self.bao(f"#{k.ma} {k.ten} vào phòng từ {addr[0]}:{addr[1]}")
            with self.khoa:
                k.gui_mic()
                k.gui({"loai": "do_loi", "gia_tri": k.do_loi})     # thông điệp thêm
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
        if loai == "tu_tat_mic":
            k.tu_tat = True
            k.gui_mic()
            self._phat_su_kien(f"{k.ten} tự tắt mic")
        elif loai == "tu_bat_mic":
            k.tu_tat = False
            if k.server_khoa:                                     # quy tắc 2
                self.bao(f"{k.ten} xin bật mic -> từ chối (server đang khóa)")
                k.gui({"loai": "tu_choi", "ly_do": "server đang khóa mic của bạn"})
            else:
                k.gui_mic()
                self._phat_su_kien(f"{k.ten} tự bật mic")
        elif loai == "am_luong_nghe":                             # quy tắc 5: chỉ ghi nhận
            k.am_luong_nghe = float(tin.get("gia_tri", 1.0))
            self.bao(f"{k.ten} tự đặt âm lượng nghe {k.am_luong_nghe:.2f}")

    # ------------------------------------------------------------ kênh dữ liệu UDP
    def _nhan_udp(self, sock):
        while self.chay:
            try:
                goi, addr = sock.recvfrom(2048)
            except socket.timeout:
                continue
            except OSError:
                break
            r = tach_rtp(goi)
            if r is None:
                continue
            pt, seq, ts, ssrc, payload = r
            with self.khoa:
                k = self.khach.get(ssrc)
                if k is None:
                    continue
                if k.udp is None and addr[0] == k.ip:
                    k.udp = addr
                    self.bao(f"#{k.ma} {k.ten} đăng ký UDP {addr[0]}:{addr[1]}")
                if addr != k.udp:                                 # quy tắc 6: sai nguồn
                    k.sai_nguon += 1
                    continue
                if pt != PT_AM_THANH or len(payload) != BYTE_MOI_KHUNG:
                    continue
                if not k.mic_mo:                                  # quy tắc 6: mic tắt
                    k.bi_chan += 1
                    continue
                k.bo_dem.nhan_goi(seq, ts, payload, time.monotonic())

    def _vong_tron(self, sock):
        """Mỗi 20 ms: trộn các client + micro server, gửi mix-minus cho từng client, cấp cho loa server."""
        ke_tiep = time.monotonic()
        while self.chay:
            with self.khoa:
                ds = list(self.khach.values())
                vao, tong = {}, [0.0] * MAU_MOI_KHUNG
                for k in ds:                                      # 1. tiếng các client × độ lợi
                    p = k.bo_dem.lay() if k.mic_mo else None
                    if p is None:
                        k.muc_vao = -math.inf
                        continue
                    mau = byte_sang_pcm(p)
                    k.muc_vao = muc_dbfs(mau)
                    vao[k.ma] = [k.do_loi * x for x in mau]
                if self.mic_server is not None:                   # 2. tiếng micro server × Mic Volume
                    g = self.am_luong_mic_server
                    vao[SSRC_SERVER] = [g * x for x in self.mic_server.lay_khung()]
                    self.muc_mic_server = muc_dbfs(vao[SSRC_SERVER])
                else:
                    self.muc_mic_server = -math.inf
                for x in vao.values():                            # 3. tổng T
                    for i, v in enumerate(x):
                        tong[i] += v
                for k in ds:                                      # 4. mỗi client: T - tiếng mình
                    if k.udp is None:
                        continue
                    ra, cat = tru_va_cat(tong, vao.get(k.ma), self.am_luong_chung)
                    k.cat_ngon += cat
                    goi = dong_goi_rtp(PT_AM_THANH, k.seq_ra, k.ts_ra, SSRC_SERVER, pcm_sang_byte(ra))
                    k.seq_ra += 1
                    k.ts_ra += MAU_MOI_KHUNG
                    try:
                        sock.sendto(goi, k.udp)
                    except OSError:
                        pass
                if self.nghe is not None:                         # 5. server nghe: T - tiếng server
                    ra, _ = tru_va_cat(tong, vao.get(SSRC_SERVER), self.am_luong_chung)
                    self.muc_nghe = muc_dbfs(ra)
                    self.nghe_gan_day.extend(ra)
                    del self.nghe_gan_day[:-TAN_SO_MAU]
                    nghe = self.nghe
                else:
                    self.muc_nghe, nghe = -math.inf, None
            if nghe is not None:
                nghe(ra)                                          # đẩy vào hàng đợi loa (ngoài khóa)
            ke_tiep += KHUNG_MS / 1000
            time.sleep(max(0.0, ke_tiep - time.monotonic()))

    # ------------------------------------------------------------ lệnh từ bảng điều khiển
    def _chon(self, ma):
        return list(self.khach.values()) if ma is None else [k for k in self.khach.values() if k.ma == ma]

    def khoa_mic(self, ma, khoa):
        """Khóa (tắt) / gỡ khóa (bật) mic của một client (ma) hoặc tất cả (ma=None)."""
        with self.khoa:
            ds = self._chon(ma)
            for k in ds:
                k.server_khoa = khoa
                k.gui_mic()
            ai = "tất cả" if ma is None else ", ".join(k.ten for k in ds)
            if khoa:
                self._phat_su_kien(f"server tắt mic {ai}")
            else:
                van_tat = [k.ten for k in ds if k.tu_tat]          # quy tắc 3
                them = f" ({', '.join(van_tat)} vẫn tự tắt)" if van_tat else ""
                self._phat_su_kien(f"server bật mic {ai}{them}")

    def dat_do_loi(self, ma, g):
        with self.khoa:
            for k in self._chon(ma):
                k.do_loi = g
                k.gui({"loai": "do_loi", "gia_tri": g})            # thông điệp thêm
                self._phat_su_kien(f"server đặt độ lợi {k.ten} = {g:.2f}")

    def dat_am_luong_chung(self, v):
        with self.khoa:
            self.am_luong_chung = v
            self._phat_su_kien(f"server đặt âm lượng chung {v:.2f}")

    def dat_mic_server(self, nguon):
        """nguon = None: server ngừng nói. Nguồn cũ do cửa sổ tự đóng."""
        with self.khoa:
            self.mic_server = nguon

    def dat_am_luong_mic_server(self, v):
        with self.khoa:
            self.am_luong_mic_server = v

    def dat_nghe(self, ham):
        with self.khoa:
            self.nghe = ham
            self.nghe_gan_day = array.array("h")

    def anh_chup(self):
        """Ảnh chụp trạng thái cho giao diện (bản sao, không giữ tham chiếu tới đối tượng dùng chung)."""
        with self.khoa:
            return {
                "khach": [{"ma": k.ma, "ten": k.ten, "ly_do": k.ly_do(), "mic_mo": k.mic_mo,
                           "server_khoa": k.server_khoa, "muc": k.muc_vao, "do_loi": k.do_loi,
                           "am_luong_nghe": k.am_luong_nghe, "mat": k.bo_dem.mat,
                           "jitter_ms": k.bo_dem.jitter_ms}
                          for k in sorted(self.khach.values(), key=lambda k: k.ma)],
                "muc_mic_server": self.muc_mic_server, "muc_nghe": self.muc_nghe,
                "nghe_gan_day": array.array("h", self.nghe_gan_day),
            }
