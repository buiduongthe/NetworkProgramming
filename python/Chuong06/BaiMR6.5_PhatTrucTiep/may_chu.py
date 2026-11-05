"""Bài mở rộng MR6.5 – Lõi máy chủ trung chuyển phát trực tiếp 1→n (kiểu SFU), dạng lớp.

Dùng chung cho server_phat.py (dòng lệnh) và server_gui.py (giao diện); không chạm giao diện, báo
sự kiện bằng hàm `bao(text)`. Start/Stop được nhiều lần; mọi socket có timeout để dừng gọn.

Kiến trúc SFU (Selective Forwarding Unit – RFC 7667 mục 3.7): máy chủ KHÔNG giải nén, KHÔNG trộn;
mỗi datagram của máy phát được NHÂN BẢN nguyên byte tới từng người xem (fan-out):
    băng thông ra ≈ n × băng thông vào; CPU chỉ tốn cho sendto() – khác MCU ở MR6.3.
Máy chủ chỉ đọc header (byte đầu, loại khung, số khung) để: đếm gói/byte, lọc theo đăng ký (âm
thanh/video) và giữ BỘ ĐỆM GOP: khung I mới nhất + mọi khung P sau nó (như GOP cache của SRS,
nginx-rtmp). Người vào muộn nhận ngay cả nhóm -> giải được hình tức thì, không phải chờ I kế tiếp.

Luồng: một luồng nhận kết nối TCP, mỗi client một luồng đọc TCP, một luồng nhận UDP chuyển tiếp
ngay (mọi gói phương tiện đi ra từ CHỈ luồng này nên thứ tự gói được giữ). Trạng thái chung
(danh sách client, bộ đệm GOP, thống kê) được bảo vệ bằng một khóa.
"""
import itertools
import socket
import threading
import time

from chung import (DAU_RTP, BO_DEM_GOP_TOI_DA, LOAI_GIU, LOAI_I, LOAI_P, PHIEN_BAN, PLI_SERVER_S,
                   GhepKhung, GiaiMaVideo, doc_dong, gui_json, tach_header, ten_hop_le)


class Khach:
    """Một kết nối: máy phát (vai_tro "phat") hoặc người xem ("xem")."""

    def __init__(self, ma, ten, vai_tro, conn, ip):
        self.ma, self.ten, self.vai_tro, self.conn, self.ip = ma, ten, vai_tro, conn, ip
        self.khoa_gui = threading.Lock()
        self.udp = None                      # địa chỉ UDP đã đăng ký
        self.am_thanh = self.video = True    # đăng ký nhận (người xem)
        self.luc_chao = time.monotonic()
        self.can_bo_dem = False              # chờ luồng UDP gửi bộ đệm GOP cho riêng người này
        self.goi_ra = self.byte_ra = 0
        self.dem_byte = 0                    # cửa sổ 1 giây
        self.kbps = 0.0
        self.khung_dau_ms = None             # từ lúc chào tới mảnh khung I đầu tiên gửi cho người này
        self.nguon_dau = ""                  # "bộ đệm" hoặc "trực tiếp"
        self.pli = 0

    def gui(self, obj):
        try:
            with self.khoa_gui:
                gui_json(self.conn, obj)
        except OSError:
            pass

    def nhan(self):
        return "A+V" if self.am_thanh and self.video else ("A" if self.am_thanh else ("V" if self.video else "-"))

    def tom_tat(self):
        return {"ma": self.ma, "ten": self.ten, "udp": f"{self.udp[0]}:{self.udp[1]}" if self.udp else "–",
                "nhan": self.nhan(), "goi_ra": self.goi_ra, "byte_ra": self.byte_ra, "kbps": self.kbps,
                "khung_dau_ms": self.khung_dau_ms, "nguon_dau": self.nguon_dau, "pli": self.pli,
                "giay": time.monotonic() - self.luc_chao}


class MayChuPhat:
    def __init__(self, bao=None):
        self.bao = bao or (lambda text: None)
        self.khoa = threading.Lock()
        self.chay = False
        self.bo_dem_bat = True
        self.xem_truoc = False               # True: tự giải mã một bản để giao diện hiện ảnh xem trước
        self._luong, self._socks = [], []
        self._lam_moi()

    def _lam_moi(self):
        self.ds = {}                         # mã -> Khach
        self.may_phat = None
        self.dang_phat = False
        self.thong_tin = self.dong_ho = None
        self.bo_dem, self.bo_dem_byte, self.bo_dem_so_i = [], 0, None
        self.byte_vao = self.goi_vao = self.byte_ra = self.goi_ra = 0
        self.dem_vao_am = self.dem_vao_hinh = self.dem_ra = 0
        self.kbps_vao_am = self.kbps_vao_hinh = self.kbps_ra = 0.0
        self.pli_chuyen = self.pli_bo = 0
        self.luc_pli = -1e9
        self.sai_nguon = 0
        self.da_roi = []                     # tóm tắt người xem đã rời (bảng tổng kết)
        self.anh_xem_truoc = None            # (w, h, rgb, ts, số khung)
        self._ghep, self._giai_ma = GhepKhung(), GiaiMaVideo()

    # ------------------------------------------------------------ khởi động / dừng
    def bat_dau(self, host, cong_tcp, cong_udp=None):
        cong_udp = cong_udp or cong_tcp + 1
        srv = socket.create_server((host, cong_tcp))
        udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        try:
            udp.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1 << 20)
            udp.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 1 << 20)
            udp.bind((host, cong_udp))
        except OSError:
            srv.close()
            udp.close()
            raise
        srv.settimeout(0.5)
        udp.settimeout(0.5)
        with self.khoa:
            self._lam_moi()
        self.cong_udp, self.ma_moi, self._socks = cong_udp, itertools.count(1), [srv, udp]
        self.dia_chi = f"{host}:{cong_tcp}"
        self.chay = True
        self._luong = [threading.Thread(target=self._nhan_ket_noi, args=(srv,), daemon=True),
                       threading.Thread(target=self._nhan_udp, args=(udp,), daemon=True)]
        for t in self._luong:
            t.start()
        self.bao(f"TCP {host}:{cong_tcp}, UDP {cong_udp}; bộ đệm GOP: "
                 + ("bật" if self.bo_dem_bat else "tắt"))

    def dung(self):
        if not self.chay:
            return
        self.chay = False
        with self.khoa:
            ds = list(self.ds.values())
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

    # ------------------------------------------------------------ TCP điều khiển
    def _su_kien(self, text):
        """Gửi sự kiện cho mọi client (gọi khi đang giữ khóa)."""
        self.bao(f"=> {text}")
        for k in list(self.ds.values()):
            k.gui({"loai": "su_kien", "noi_dung": text})

    def _ds_nguoi_xem(self):
        return [k for k in self.ds.values() if k.vai_tro == "xem"]

    def _bao_nguoi_xem(self):
        """Gửi máy phát danh sách người xem mỗi khi danh sách đổi (gọi khi giữ khóa)."""
        if self.may_phat:
            ds = [k.ten for k in sorted(self._ds_nguoi_xem(), key=lambda k: k.ma)]
            self.may_phat.gui({"loai": "nguoi_xem", "so": len(ds), "ds": ds})

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
        tep = conn.makefile("rb")
        k = None
        try:
            tin = doc_dong(tep)
            if not tin or tin.get("loai") != "chao" or tin.get("vai_tro") not in ("phat", "xem"):
                return
            if not ten_hop_le(tin.get("ten")):
                gui_json(conn, {"loai": "loi", "ly_do": "tên không hợp lệ"})
                return
            with self.khoa:
                if tin["vai_tro"] == "phat" and self.may_phat is not None:
                    gui_json(conn, {"loai": "loi", "ly_do": "đã có máy phát"})
                    self.bao(f"Từ chối máy phát thứ hai {tin['ten']}")
                    return
                k = Khach(next(self.ma_moi), tin["ten"], tin["vai_tro"], conn, addr[0])
                self.ds[k.ma] = k
                k.gui({"loai": "chao", "ma": k.ma, "cong_udp": self.cong_udp, "phat": self.dang_phat})
                if k.vai_tro == "phat":
                    self.may_phat = k
                    self.bao(f"#{k.ma} {k.ten} (phát) vào từ {addr[0]}:{addr[1]}")
                    self._su_kien(f"{k.ten} kết nối (máy phát)")
                    self._bao_nguoi_xem()
                else:
                    if self.thong_tin:
                        k.gui(self.thong_tin)           # bản đã lưu cho người vào sau
                    if self.dong_ho:
                        k.gui(self.dong_ho)
                    self.bao(f"#{k.ma} {k.ten} (xem) vào từ {addr[0]}:{addr[1]}")
                    self._su_kien(f"{k.ten} vào xem")
                    self._bao_nguoi_xem()
            while self.chay:
                tin = doc_dong(tep)
                if tin is None:
                    break
                with self.khoa:
                    self._xu_ly(k, tin)
        except (OSError, ValueError):
            pass
        finally:
            if k:
                with self.khoa:
                    self._roi(k)
            try:
                conn.close()
            except OSError:
                pass

    def _roi(self, k, ly_do="rời"):
        if self.ds.pop(k.ma, None) is None:
            return
        if k is self.may_phat:
            if self.dang_phat:
                self._ket_thuc_phat()
            self.may_phat = None
            self._su_kien(f"{k.ten} (máy phát) thoát")
        else:
            self.da_roi.append(k.tom_tat())
            self._su_kien(f"{k.ten} {ly_do}")
            self._bao_nguoi_xem()

    def _ket_thuc_phat(self):
        self.dang_phat = False
        self.thong_tin = self.dong_ho = None
        self.bo_dem, self.bo_dem_byte, self.bo_dem_so_i = [], 0, None
        self._ghep, self._giai_ma = GhepKhung(), GiaiMaVideo()
        for x in self.ds.values():
            x.gui({"loai": "ket_thuc"})

    def _xu_ly(self, k, tin):
        loai = tin.get("loai")
        if k.vai_tro == "phat":
            if loai == "thong_tin":
                self.thong_tin = tin
                for x in self._ds_nguoi_xem():
                    x.gui(tin)                          # chuyển tiếp nguyên văn
                self.bao(f"{k.ten} phát {tin.get('rong')}x{tin.get('cao')} {tin.get('fps')} fps"
                         f" GOP {tin.get('gop')}, {tin.get('tan_so_mau')} Hz")
                if not self.dang_phat:
                    self.dang_phat = True
                    self._su_kien(f"{k.ten} bắt đầu phát")
            elif loai == "dong_ho":
                self.dong_ho = tin
                for x in self._ds_nguoi_xem():
                    x.gui(tin)
            elif loai == "dung_phat" and self.dang_phat:
                self._ket_thuc_phat()
                self._su_kien(f"{k.ten} dừng phát")
            return
        if loai == "dang_ky":
            co_video = k.video
            k.am_thanh = bool(tin.get("am_thanh", k.am_thanh))
            k.video = bool(tin.get("video", k.video))
            if k.video and not co_video and self.bo_dem_bat and k.udp:
                k.can_bo_dem = True                     # bật lại video: gửi bộ đệm GOP
            self.bao(f"{k.ten} đăng ký nhận: {k.nhan()}")
        elif loai == "xin_khung_khoa":
            k.pli += 1
            bay_gio = time.monotonic()
            if self.may_phat and self.dang_phat and bay_gio - self.luc_pli >= PLI_SERVER_S:
                self.luc_pli = bay_gio
                self.pli_chuyen += 1
                self.may_phat.gui({"loai": "xin_khung_khoa", "tu": k.ten})
                self.bao(f"PLI từ {k.ten} -> {self.may_phat.ten}")
            else:
                self.pli_bo += 1
                self.bao(f"PLI từ {k.ten}: bỏ (lọc 500 ms)")

    # ------------------------------------------------------------ UDP: nhận và nhân bản
    def _nhan_udp(self, sock):
        moc = time.monotonic()
        while self.chay:
            try:
                goi, addr = sock.recvfrom(2048)
            except socket.timeout:
                goi = None
            except OSError:
                break
            bay_gio = time.monotonic()
            self._gui_bo_dem_cho(sock)                  # bộ đệm GOP đi TRƯỚC mọi gói trực tiếp
            if goi:
                with self.khoa:
                    ra = self._xu_ly_goi(goi, addr, bay_gio)
                for g, dich in ra:
                    try:
                        sock.sendto(g, dich)
                    except OSError:
                        pass
            if bay_gio - moc >= 1.0:
                moc += 1.0
                if bay_gio - moc > 1.0:
                    moc = bay_gio
                with self.khoa:
                    self.kbps_vao_am, self.kbps_vao_hinh = self.dem_vao_am * 8 / 1000, self.dem_vao_hinh * 8 / 1000
                    self.kbps_ra = self.dem_ra * 8 / 1000
                    self.dem_vao_am = self.dem_vao_hinh = self.dem_ra = 0
                    for k in self.ds.values():
                        k.kbps, k.dem_byte = k.dem_byte * 8 / 1000, 0

    def _gui_bo_dem_cho(self, sock):
        """Gửi toàn bộ bộ đệm GOP cho người vừa có địa chỉ UDP / vừa bật lại video.
        Nghỉ 1 ms mỗi 32 gói để không làm tràn bộ đệm nhận của người xem."""
        with self.khoa:
            ds = [k for k in self._ds_nguoi_xem() if k.can_bo_dem and k.udp]
            if not ds:
                return
            goi = list(self.bo_dem) if self.bo_dem_bat else []
            so_i = self.bo_dem_so_i
            for k in ds:
                k.can_bo_dem = False
        if not goi:
            return
        for k in ds:
            for n, g in enumerate(goi):
                try:
                    sock.sendto(g, k.udp)
                except OSError:
                    pass
                if n % 32 == 31:
                    time.sleep(0.001)
            with self.khoa:
                k.goi_ra += len(goi)
                b = sum(len(g) for g in goi)
                k.byte_ra += b
                k.dem_byte += b
                self.goi_ra += len(goi)
                self.byte_ra += b
                self.dem_ra += b
                so_p = len({g[4:8] for g in goi if g[1] == LOAI_P})
                self.bao(f"{k.ten}: gửi bộ đệm GOP {len(goi)} gói (I #{so_i} + {so_p} P)")
                if k.khung_dau_ms is None:
                    k.khung_dau_ms = (time.monotonic() - k.luc_chao) * 1000
                    k.nguon_dau = "bộ đệm"
                    self.bao(f"{k.ten}: khung I đầu sau {k.khung_dau_ms:.0f} ms (bộ đệm)")

    def _xu_ly_goi(self, goi, addr, bay_gio):
        """Xử lý một datagram (đang giữ khóa). Trả về danh sách (gói, đích) cần gửi."""
        b0 = goi[0]
        if b0 == PHIEN_BAN:
            h = tach_header(goi)
            if h is None:
                return []
            loai, ma, so, i, tong, w, hh, ts, payload = h
            if loai == LOAI_GIU:
                k = self.ds.get(ma)
                if k is not None and addr[0] == k.ip and k.udp != addr:
                    lan_dau = k.udp is None
                    k.udp = addr
                    self.bao(f"#{k.ma} {k.ten} đăng ký UDP {addr[0]}:{addr[1]}")
                    if lan_dau and k.vai_tro == "xem" and k.video and self.bo_dem_bat:
                        k.can_bo_dem = True
                return []
            video = True
        elif b0 == DAU_RTP:
            video = False
        else:
            return []
        if self.may_phat is None or addr != self.may_phat.udp or not self.dang_phat:
            self.sai_nguon += 1                          # chỉ nhận phương tiện từ máy phát
            return []
        self.goi_vao += 1
        self.byte_vao += len(goi)
        if video:
            self.dem_vao_hinh += len(goi)
            self._cap_nhat_bo_dem(loai, so, goi)
            if self.xem_truoc:
                self._giai_ma_xem_truoc(loai, so, i, tong, w, hh, ts, payload, bay_gio)
        else:
            self.dem_vao_am += len(goi)
        ra = []
        for k in self._ds_nguoi_xem():
            if k.udp is None or k.can_bo_dem or not (k.video if video else k.am_thanh):
                continue
            ra.append((goi, k.udp))
            k.goi_ra += 1
            k.byte_ra += len(goi)
            k.dem_byte += len(goi)
            if video and loai == LOAI_I and k.khung_dau_ms is None:
                k.khung_dau_ms = (bay_gio - k.luc_chao) * 1000
                k.nguon_dau = "trực tiếp"
                self.bao(f"{k.ten}: khung I đầu sau {k.khung_dau_ms:.0f} ms (trực tiếp)")
        self.goi_ra += len(ra)
        self.byte_ra += len(goi) * len(ra)
        self.dem_ra += len(goi) * len(ra)
        return ra

    def _cap_nhat_bo_dem(self, loai, so, goi):
        """Bộ đệm GOP: mảnh đầu tiên của khung I mới -> xóa; lưu mảnh của I và các P sau nó."""
        if not self.bo_dem_bat:
            return
        if loai == LOAI_I and so != self.bo_dem_so_i:
            self.bo_dem, self.bo_dem_byte, self.bo_dem_so_i = [], 0, so
        elif loai != LOAI_I and self.bo_dem_so_i is None:
            return                                       # chưa có I (hoặc vừa tràn): chờ I kế tiếp
        self.bo_dem.append(goi)
        self.bo_dem_byte += len(goi)
        if self.bo_dem_byte > BO_DEM_GOP_TOI_DA:
            self.bo_dem, self.bo_dem_byte, self.bo_dem_so_i = [], 0, None
            self.bao("Bộ đệm GOP quá 4 MB: xóa, chờ khung I kế tiếp")

    def _giai_ma_xem_truoc(self, loai, so, i, tong, w, h, ts, payload, bay_gio):
        """Server giải mã MỘT bản chỉ để hiện ảnh xem trước (không ảnh hưởng việc chuyển tiếp)."""
        k = self._ghep.them(loai, so, i, tong, w, h, ts, payload, bay_gio)
        if k is None:
            return
        self._giai_ma.kiem_tra(bay_gio)
        for so_k, _, ts_k, wk, hk, anh in self._giai_ma.them(*k, bay_gio):
            self.anh_xem_truoc = (wk, hk, anh, ts_k, so_k)

    # ------------------------------------------------------------ lệnh của bảng điều khiển
    def dat_bo_dem(self, bat):
        with self.khoa:
            self.bo_dem_bat = bat
            if not bat:
                self.bo_dem, self.bo_dem_byte, self.bo_dem_so_i = [], 0, None
        self.bao("Bộ đệm GOP: " + ("bật" if bat else "tắt"))

    def ngat(self, ten=None, ma=None):
        """Đuổi người xem theo tên (hoặc mã). Trả về số người bị ngắt."""
        with self.khoa:
            ds = [k for k in self._ds_nguoi_xem() if (ma is not None and k.ma == ma) or k.ten == ten]
            for k in ds:
                k.gui({"loai": "bi_ngat", "ly_do": "server ngắt"})
                self._roi(k, "bị server ngắt")
        for k in ds:
            try:
                k.conn.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        return len(ds)

    def trang_thai(self):
        """Ảnh chụp trạng thái cho dòng lệnh và giao diện."""
        with self.khoa:
            vao = self.kbps_vao_am + self.kbps_vao_hinh
            return {"dang_phat": self.dang_phat, "ten_phat": self.may_phat.ten if self.may_phat else None,
                    "thong_tin": self.thong_tin, "bo_dem_bat": self.bo_dem_bat,
                    "bo_dem_goi": len(self.bo_dem), "bo_dem_byte": self.bo_dem_byte,
                    "bo_dem_so_i": self.bo_dem_so_i,
                    "kbps_vao": vao, "kbps_vao_am": self.kbps_vao_am, "kbps_vao_hinh": self.kbps_vao_hinh,
                    "kbps_ra": self.kbps_ra, "he_so": self.kbps_ra / vao if vao else 0.0,
                    "byte_vao": self.byte_vao, "byte_ra": self.byte_ra, "goi_vao": self.goi_vao,
                    "goi_ra": self.goi_ra, "pli_chuyen": self.pli_chuyen, "pli_bo": self.pli_bo,
                    "nguoi_xem": [k.tom_tat() for k in sorted(self._ds_nguoi_xem(), key=lambda k: k.ma)],
                    "da_roi": list(self.da_roi), "anh": self.anh_xem_truoc}


def dong_bang(x):
    """Một dòng của bảng người xem (≤ 55 ký tự) – dùng chung cho trang_thai và tổng kết."""
    dau = "–" if x["khung_dau_ms"] is None else f"{x['khung_dau_ms']:4.0f} ms {x['nguon_dau']}"
    return f"{x['ten'][:6]:<6} {x['nhan']:<4} {x['goi_ra']:6d} {x['byte_ra'] / 1024:6.0f} {x['kbps']:6.0f} {dau}"


TIEU_DE_BANG = f"{'tên':<6} {'nhận':<4} {'gói ra':>6} {'KB ra':>6} {'kbit/s':>6} khung I đầu"


def bang_trang_thai(tt):
    """Các dòng in cho lệnh trang_thai."""
    ds = [f"Trạng thái: {tt['ten_phat'] + ' đang phát' if tt['dang_phat'] else 'chưa phát'},"
          f" {len(tt['nguoi_xem'])} người xem",
          f"bộ đệm GOP {'bật' if tt['bo_dem_bat'] else 'tắt'}: {tt['bo_dem_goi']} gói,"
          f" {tt['bo_dem_byte'] / 1024:.0f} KB",
          f"vào {tt['kbps_vao']:.0f} kbit/s (tiếng {tt['kbps_vao_am']:.0f}, hình {tt['kbps_vao_hinh']:.0f})",
          f"ra {tt['kbps_ra']:.0f} kbit/s, hệ số nhân bản {tt['he_so']:.2f}",
          f"PLI chuyển {tt['pli_chuyen']}, bỏ {tt['pli_bo']}",
          TIEU_DE_BANG]
    return ds + [dong_bang(x) for x in tt["nguoi_xem"]]


def bang_tong_ket(tt):
    """Các dòng in khi server kết thúc: mọi người xem đã vào (cả người đã rời)."""
    ds = tt["da_roi"] + tt["nguoi_xem"]
    he_so = tt["byte_ra"] / tt["byte_vao"] if tt["byte_vao"] else 0.0
    return ([f"===== TỔNG KẾT: {len(ds)} người xem =====",
             f"vào {tt['goi_vao']} gói {tt['byte_vao'] / 1024:.0f} KB,"
             f" ra {tt['goi_ra']} gói {tt['byte_ra'] / 1024:.0f} KB",
             f"hệ số nhân bản (byte ra/vào) {he_so:.2f}",
             f"PLI chuyển {tt['pli_chuyen']}, bỏ {tt['pli_bo']}",
             TIEU_DE_BANG]
            + [dong_bang(dict(x, kbps=x["byte_ra"] * 8 / 1000 / max(x["giay"], 0.001))) for x in ds]
            + ["(kbit/s = trung bình suốt thời gian xem)"])
