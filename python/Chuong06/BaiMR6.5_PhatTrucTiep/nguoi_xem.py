"""Bài mở rộng MR6.5 – Lõi người xem phát trực tiếp, dạng lớp, không chạm giao diện.

Dùng chung cho xem.py (dòng lệnh) và xem_gui.py. Bốn luồng, chung một khóa `khoa`:
  1. luồng đọc TCP: thong_tin, dong_ho (báo cáo đồng hồ), su_kien, ket_thuc, bi_ngat;
  2. luồng nhận UDP: RTP -> bộ đệm jitter; mảnh video -> GhepKhung -> GiaiMaVideo -> hàng hiển thị;
  3. luồng phát 20 ms = ĐỒNG HỒ CHỦ: lấy một khung âm thanh, đưa ra loa, cập nhật
        dong_ho_am = ts/16 (ms phương tiện) và giờ thực lúc phát; ghi lịch sử ~3 s để đo lệch;
  4. luồng hiển thị 5 ms: kiểm tra mất tham chiếu (xin khung khóa), chọn khung để hiện.

Đồng bộ tiếng-hình (lip-sync) – âm thanh làm đồng hồ chủ (như trình phát thật: tai nhạy với tiếng
bị ngắt hơn mắt nhạy với một khung hình hiện sớm/muộn):
  - BẬT: chỉ hiện khung mới nhất có nhãn ≤ đồng hồ phát hiện tại
        dong_ho_am + (bây giờ − lúc phát khung âm thanh đó); khung quá cũ bị bỏ.
  - TẮT (hoặc chỉ nhận video): hiện ngay khi giải mã xong -> hình đi TRƯỚC tiếng đúng bằng thời gian
    âm thanh nằm trong bộ đệm jitter (độ sâu × 20 ms).
Lệch tiếng-hình của một khung = giờ thực lúc hiện − giờ thực lúc âm thanh cùng nhãn được phát
(nội suy trên lưới 20 ms của lịch sử phát); dương = hình chậm hơn tiếng, âm = hình đi trước.
Độ trễ đầu-cuối (âm thanh) = giờ Unix lúc phát − (gio + (ts_ms − ts)) theo báo cáo đồng hồ mới nhất
– chỉ đúng khi máy phát và người xem cùng đồng hồ (cùng máy hoặc đồng bộ NTP).
"""
import array
import collections
import math
import random
import socket
import threading
import time

from chung import (BYTE_MOI_KHUNG, phong_to, to_mau, ve_chu, DAU_RTP, DO_SAU_MAC_DINH, HANG_HIEN_THI, KHUNG_MS, LOAI_I, LOAI_P,
                   PHIEN_BAN, PLI_XEM_S, PT_AM_THANH, TAN_SO_MAU, BoDemJitter, GhepKhung, GiaiMaVideo,
                   byte_sang_pcm, doc_dong, goi_giu, gui_json, muc_dbfs, tach_header, tach_rtp)

MS_MOI_MAU = 1000 / TAN_SO_MAU


class NguoiXem:
    def __init__(self, ten, bao=None, do_sau=DO_SAU_MAC_DINH, dong_bo=True, am_thanh=True, video=True,
                 mat_goi=0.0, loa=None):
        self.ten, self.bao = ten, bao or (lambda text: None)
        self.khoa, self.khoa_tcp = threading.Lock(), threading.Lock()
        self.tcp = self.udp = None
        self.da_ket_noi, self.ma = False, None
        self.do_sau, self.dong_bo, self.am_thanh, self.video = do_sau, dong_bo, am_thanh, video
        self.mat_goi, self.rng, self.bo_gia_lap = mat_goi, random.Random(0), 0
        self.loa = loa                              # đối tượng có phat(mẫu) hoặc None (không phát)
        self.thong_tin = None
        self.muc_am = -math.inf
        self.giai_doan = [{"dong_bo": dong_bo, "tong": 0.0, "n": 0}]
        self._lam_moi_thong_ke()
        self._lam_moi_phien()
        self.song = True
        threading.Thread(target=self._vong_am, daemon=True).start()
        threading.Thread(target=self._vong_hien, daemon=True).start()

    def _lam_moi_thong_ke(self):
        self.luc_chao = time.monotonic()
        self.luc_moc = self.luc_chao                      # mốc đo khung đầu: lúc vào, hoặc lúc máy phát
        self.cho_phat = False                             # bắt đầu phát nếu khi vào chưa có ai phát
        self.goi_am = self.byte_nhan = self.khung_i = self.khung_p = self.khung_hien = 0
        self.dung_hinh = self.pli_gui = self.bo_cu = 0
        self.mat_cu = self.hong_cu = self.bo_p_cu = 0     # số đếm của các phiên phát trước
        self.tong_tre = 0.0
        self.n_tre = 0
        self.tre_hien = self.lech_hien = None
        self.khung_dau_ms, self.nguon_dau, self.i_dau = None, "", None
        self.cho_hinh_lai = None
        self.luc_pli = -1e9
        self.dang_dung = False
        self.w_am = self.w_hien = self.w_byte = 0
        self.w_tre, self.w_lech = [], []

    def _lam_moi_phien(self):
        """Trạng thái của một phiên phát (làm mới khi máy phát dừng phát)."""
        if hasattr(self, "bo_dem"):
            self.mat_cu += self.bo_dem.mat
            self.hong_cu += self.ghep.khung_hong
            self.bo_p_cu += self.giai_ma.bo_p
        self.bo_dem = BoDemJitter(self.do_sau)
        self.ghep, self.giai_ma = GhepKhung(), GiaiMaVideo()
        self.hang = collections.deque()
        self.lich_su = collections.deque(maxlen=150)       # (ts ms, giờ đơn điệu) ~3 s gần nhất
        self.dong_ho_am = self.luc_phat_am = self.luc_nhan_am = None
        self.ts_am_moi = self.ts_hinh_moi = None
        self.dong_ho_bao = None                             # (ts, gio) báo cáo đồng hồ mới nhất
        self.hien = None                # (w, h, rgb, ts, số khung, đồng hồ tiếng lúc hiện) đang hiện
        self.so_hien = 0

    # ------------------------------------------------------------ kết nối
    def ket_noi(self, host, cong):
        tcp = socket.create_connection((host, cong), timeout=5)
        try:
            tcp.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            tep = tcp.makefile("rb")
            gui_json(tcp, {"loai": "chao", "vai_tro": "xem", "ten": self.ten})
            tl = doc_dong(tep) or {}
            if tl.get("loai") == "loi":
                raise OSError(f"server từ chối: {tl.get('ly_do')}")
            if tl.get("loai") != "chao":
                raise OSError("server không trả lời 'chao'")
            tcp.settimeout(None)
            udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            udp.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1 << 20)
            udp.connect((host, tl["cong_udp"]))                # chỉ nhận datagram từ server
        except (OSError, ValueError, KeyError) as e:
            tcp.close()
            raise OSError(str(e)) from None
        with self.khoa:
            self.tcp, self.udp, self.ma = tcp, udp, tl["ma"]
            self.rng = random.Random(self.ma)
            self._lam_moi_phien()
            self._lam_moi_thong_ke()
            self.cho_phat = not tl.get("phat")
            self.da_ket_noi = True
        if not (self.am_thanh and self.video):
            self.gui({"loai": "dang_ky", "am_thanh": self.am_thanh, "video": self.video})
        udp.send(goi_giu(self.ma))                             # đăng ký địa chỉ UDP
        self.bao(f"Vào xem: mã {self.ma}, " + ("máy phát đang phát" if tl.get("phat") else "chưa có ai phát"))
        threading.Thread(target=self._doc_tcp, args=(tep,), daemon=True).start()
        threading.Thread(target=self._nhan_udp, args=(udp,), daemon=True).start()
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

    def ngat(self):
        with self.khoa:
            tcp, udp = self.tcp, self.udp
            self.tcp = self.udp = None
            self.da_ket_noi = False
        for s in (tcp, udp):
            if s:
                try:
                    if s is tcp:
                        s.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                s.close()

    def _doc_tcp(self, tep):
        try:
            while True:
                tin = doc_dong(tep)
                if tin is None:
                    break
                loai = tin.get("loai")
                if loai == "thong_tin":
                    with self.khoa:
                        self.thong_tin = tin
                        if self.cho_phat:                   # vào trước khi phát: đo từ lúc bắt đầu phát
                            self.cho_phat = False
                            if self.khung_dau_ms is None:
                                self.luc_moc = time.monotonic()
                    self.bao(f"Luồng {tin.get('rong')}x{tin.get('cao')}, {tin.get('fps')} fps,"
                             f" GOP {tin.get('gop')}, {tin.get('tan_so_mau')} Hz")
                elif loai == "dong_ho":
                    with self.khoa:
                        self.dong_ho_bao = (float(tin["ts"]), float(tin["gio"]))
                elif loai == "su_kien":
                    self.bao(f"Sự kiện: {tin.get('noi_dung')}")
                elif loai == "ket_thuc":
                    with self.khoa:
                        self.thong_tin = None
                        self.cho_phat = True
                        self._lam_moi_phien()
                    self.bao("Máy phát kết thúc phát")
                elif loai == "bi_ngat":
                    self.bao(f"Bị ngắt: {tin.get('ly_do')}")
        except (OSError, ValueError, KeyError):
            pass
        if self.da_ket_noi:
            self.bao("Mất kết nối server")
            self.ngat()

    # ------------------------------------------------------------ luồng nhận UDP
    def _uoc_luong_ts(self):
        """Ước lượng ms phương tiện hiện tại (để biết khung I đầu tiên "cũ" bao nhiêu)."""
        if self.dong_ho_bao:
            ts, gio = self.dong_ho_bao
            return ts + (time.time() * 1000 - gio)               # giả định cùng đồng hồ
        moi = [x for x in (self.ts_hinh_moi, self.ts_am_moi and self.ts_am_moi * MS_MOI_MAU) if x]
        return max(moi) if moi else None

    def _nhan_udp(self, udp):
        while True:
            try:
                goi = udp.recv(2048)
            except OSError:
                return
            if not goi:
                continue
            if self.mat_goi and self.rng.random() < self.mat_goi:
                self.bo_gia_lap += 1                             # "mạng" làm rơi gói ở chiều xuống
                continue
            bay_gio = time.monotonic()
            with self.khoa:
                self.byte_nhan += len(goi)
                self.w_byte += len(goi)
                if goi[0] == DAU_RTP:
                    r = tach_rtp(goi)
                    if r and r[0] == PT_AM_THANH and len(r[4]) == BYTE_MOI_KHUNG and self.am_thanh:
                        self.bo_dem.nhan_goi(r[1], r[2], r[4])
                        self.luc_nhan_am, self.ts_am_moi = bay_gio, r[2]
                        self.goi_am += 1
                        self.w_am += 1
                elif goi[0] == PHIEN_BAN and self.video:
                    h = tach_header(goi)
                    if h is None or h[0] not in (LOAI_I, LOAI_P):
                        continue
                    loai, _, so, i, tong, w, hh, ts, payload = h
                    k = self.ghep.them(loai, so, i, tong, w, hh, ts, payload, bay_gio)
                    if k is not None:
                        self._nhan_khung(k, bay_gio)

    def _nhan_khung(self, k, bay_gio):
        """Khung vừa đủ mảnh -> giải mã (có thể giải thêm các khung đang chờ) -> hàng hiển thị."""
        for kh in self.giai_ma.them(*k, bay_gio):
            so, loai, ts = kh[0], kh[1], kh[2]
            self.ts_hinh_moi = max(self.ts_hinh_moi or 0, ts)
            if loai == LOAI_I:
                self.khung_i += 1
                if self.i_dau is None:
                    uoc = self._uoc_luong_ts()
                    self.i_dau = (None if uoc is None else uoc - ts, (bay_gio - self.luc_moc) * 1000)
                if self.dang_dung:
                    self.dang_dung = False
                    ms = (bay_gio - self.giai_ma.luc_mat) * 1000 if self.giai_ma.luc_mat else 0
                    self.bao(f"Hồi phục bằng khung I #{so} sau {ms:.0f} ms")
            else:
                self.khung_p += 1
            self.hang.append(kh)
            if len(self.hang) > HANG_HIEN_THI:
                self.hang.popleft()
                self.bo_cu += 1

    # ------------------------------------------------------------ luồng phát 20 ms (đồng hồ chủ)
    def _vong_am(self):
        ke_tiep = time.monotonic()
        n = 0
        im_lang = array.array("h", bytes(BYTE_MOI_KHUNG))
        while self.song:
            n += 1
            mau = None
            with self.khoa:
                udp = self.udp
                r = self.bo_dem.lay() if self.da_ket_noi and self.am_thanh else None
                bay_gio = time.monotonic()
                if r is not None:
                    ts, p = r
                    ts_ms = ts * MS_MOI_MAU
                    self.dong_ho_am, self.luc_phat_am = ts_ms, bay_gio
                    self.lich_su.append((ts_ms, bay_gio))
                    if p is None:
                        mau = im_lang                              # mất gói: phát im lặng
                    else:
                        mau = byte_sang_pcm(p)
                        if self.dong_ho_bao:                        # trễ đầu-cuối (cùng đồng hồ)
                            ts_r, gio_r = self.dong_ho_bao
                            tre = time.time() * 1000 - (gio_r + (ts_ms - ts_r))
                            self.tre_hien = tre
                            self.tong_tre += tre
                            self.n_tre += 1
                            self.w_tre.append(tre)
            self.muc_am = muc_dbfs(mau) if mau is not None else -math.inf
            loa = self.loa
            if loa is not None:
                loa.phat(mau if mau is not None else im_lang)
            if udp is not None and n % 50 == 0:
                try:
                    udp.send(goi_giu(self.ma))                      # giữ kết nối mỗi 1 s
                except OSError:
                    pass
            ke_tiep += KHUNG_MS / 1000
            con = ke_tiep - time.monotonic()
            if con < -0.1:
                ke_tiep = time.monotonic()                          # tụt quá xa: bắt nhịp lại
            elif con > 0:
                time.sleep(con)

    # ------------------------------------------------------------ luồng hiển thị 5 ms
    def _vong_hien(self):
        while self.song:
            time.sleep(0.005)
            xin, gui_pli = None, False
            with self.khoa:
                if not self.da_ket_noi:
                    continue
                bay_gio = time.monotonic()
                gm = self.giai_ma
                thieu = gm.tham_chieu[0] + 1 if gm.tham_chieu is not None and gm.doi else None
                duoc_xin = bay_gio - self.luc_pli >= PLI_XEM_S          # giới hạn 1 lần / 1 s
                if self.video and gm.kiem_tra(bay_gio):
                    self.dung_hinh += 1
                    self.dang_dung = True
                    xin = f"Thiếu khung #{thieu} -> đứng hình" + (", xin khung khóa" if duoc_xin
                                                               else " (xin sau giới hạn 1 s)")
                    gui_pli = duoc_xin
                elif self.video and gm.mat_tham_chieu and duoc_xin:
                    xin = "Chưa có khung I: xin khung khóa"          # bị giới hạn lúc trước / I bị mất
                    gui_pli = True
                if gui_pli:
                    self.luc_pli = bay_gio
                    self.pli_gui += 1
                self.ghep.don_dep(bay_gio)
                chon = self._chon_khung(bay_gio)
                if chon is not None:
                    self._hien(chon, bay_gio)
            if xin:
                self.bao(xin)
                if gui_pli:
                    self.gui({"loai": "xin_khung_khoa"})

    def _dong_ho_hien_tai(self, bay_gio):
        """Đồng hồ phát âm thanh hiện tại (ms phương tiện) hoặc None nếu tiếng không chạy."""
        if self.luc_phat_am is None or bay_gio - self.luc_phat_am > 0.2:
            return None
        return self.dong_ho_am + min(bay_gio - self.luc_phat_am, 0.04) * 1000

    def _chon_khung(self, bay_gio):
        if not self.hang:
            return None
        if not self.video:
            self.hang.clear()
            return None
        # Tiếng "đang chạy": vừa nhận gói tiếng trong 0,5 s, hoặc mới vào xem chưa tới 0,5 s (chờ tiếng
        # tới để khung đầu cũng được đồng bộ). Không có tiếng -> hiện ngay, không chờ mãi.
        if self.luc_nhan_am is not None:
            tieng_chay = self.am_thanh and bay_gio - self.luc_nhan_am < 0.5
        else:
            tieng_chay = self.am_thanh and bay_gio - self.luc_chao < 0.5
        if self.dong_bo and tieng_chay:
            am = self._dong_ho_hien_tai(bay_gio)
            if am is None:
                return None                                 # chờ đồng hồ tiếng (bộ đệm jitter đang nạp)
            chon = None
            while self.hang and self.hang[0][2] <= am:
                if chon is not None:
                    self.bo_cu += 1                         # khung quá cũ: bỏ
                chon = self.hang.popleft()
            return chon
        chon = self.hang.pop()                              # không đồng bộ: hiện ngay khung mới nhất
        self.bo_cu += len(self.hang)
        self.hang.clear()
        return chon

    def _gio_am(self, ts_ms):
        """Giờ (đơn điệu) mà âm thanh có nhãn ts_ms được/sẽ được phát – nội suy theo lịch sử phát."""
        for t_am, gio in reversed(self.lich_su):
            if t_am <= ts_ms:
                return gio + (ts_ms - t_am) / 1000
        return None

    def _hien(self, k, bay_gio):
        so, loai, ts, w, h, anh = k
        self.hien = (w, h, anh, ts, so, self._dong_ho_hien_tai(bay_gio))
        self.so_hien += 1
        self.khung_hien += 1
        self.w_hien += 1
        if self.khung_dau_ms is None:
            self.khung_dau_ms = (bay_gio - self.luc_moc) * 1000
            # Nguồn khung đầu: khung I đã "cũ" (chụp trước lúc vào > 1,5 chu kỳ khung) -> từ bộ đệm GOP;
            # I mới nhưng tới muộn -> phải chờ khung I; I mới tới ngay -> vừa đúng lúc có khung I.
            cu, den = self.i_dau if self.i_dau else (None, None)
            nguong = 1500 / (self.thong_tin or {}).get("fps", 10)
            them = ""
            if cu is not None and cu > nguong:
                self.nguon_dau, them = "bộ đệm GOP", f" (I cũ {cu / 1000:.1f} s)"
            elif den is not None and den > nguong:
                self.nguon_dau = "chờ khung I"
            else:
                self.nguon_dau = "khung I trực tiếp"
            moc = " (từ lúc phát)" if self.luc_moc > self.luc_chao else ""
            self.bao(f"Khung đầu sau {self.khung_dau_ms:.0f} ms{moc}: {self.nguon_dau}{them}")
        if self.cho_hinh_lai is not None:
            self.bao(f"Hình trở lại sau {(bay_gio - self.cho_hinh_lai) * 1000:.0f} ms")
            self.cho_hinh_lai = None
        tieng_chay = self.am_thanh and self.luc_phat_am is not None and bay_gio - self.luc_phat_am < 0.5
        gio_am = self._gio_am(ts) if tieng_chay else None
        if gio_am is not None:
            lech = (bay_gio - gio_am) * 1000
            self.lech_hien = lech
            self.giai_doan[-1]["tong"] += lech
            self.giai_doan[-1]["n"] += 1
            self.w_lech.append(lech)
        else:
            self.lech_hien = None

    # ------------------------------------------------------------ điều khiển
    def dat_dong_bo(self, bat):
        with self.khoa:
            if bat != self.dong_bo:
                self.dong_bo = bat
                self.giai_doan.append({"dong_bo": bat, "tong": 0.0, "n": 0})
        self.bao("Đồng bộ tiếng-hình: " + ("BẬT" if bat else "TẮT"))

    def dang_ky(self, am_thanh, video):
        with self.khoa:
            if not video and self.video:
                self.ghep.bo_dang_do()
                self.hang.clear()
            if video and not self.video:
                self.cho_hinh_lai = time.monotonic()
            if not am_thanh and self.am_thanh:
                self.bo_dem.xoa()
                self.lich_su.clear()
                self.luc_phat_am = self.luc_nhan_am = None
            if am_thanh and not self.am_thanh:
                self.mat_cu += self.bo_dem.mat
                self.bo_dem = BoDemJitter(self.do_sau)
            self.am_thanh, self.video = am_thanh, video
        self.gui({"loai": "dang_ky", "am_thanh": am_thanh, "video": video})
        ten = "âm thanh+video" if am_thanh and video else ("âm thanh" if am_thanh else "video")
        self.bao(f"Đăng ký nhận: {ten}")

    def dat_do_sau(self, n):
        with self.khoa:
            self.do_sau = max(1, min(25, int(n)))
            self.bo_dem.dat_do_sau(self.do_sau)

    def chup(self):
        """(w, h, rgb, nhãn hình ms, đồng hồ tiếng LÚC HIỆN khung ms|None, lệch ms|None, số khung)."""
        with self.khoa:
            if self.hien is None:
                return None
            w, h, anh, ts, so, am = self.hien
            return w, h, anh, ts, am, self.lech_hien, so

    # ------------------------------------------------------------ số liệu
    def thong_ke_giay(self):
        """Gọi mỗi 1 s: số liệu của giây vừa qua (rồi xóa bộ đếm cửa sổ)."""
        with self.khoa:
            r = {"am": self.w_am, "mat": self.mat_cu + self.bo_dem.mat, "hien": self.w_hien,
                 "tre": sum(self.w_tre) / len(self.w_tre) if self.w_tre else None,
                 "lech": sum(self.w_lech) / len(self.w_lech) if self.w_lech else None,
                 "kbps": self.w_byte * 8 / 1000}
            self.w_am = self.w_hien = self.w_byte = 0
            self.w_tre, self.w_lech = [], []
        return r

    def trang_thai(self):
        with self.khoa:
            giay = max(time.monotonic() - self.luc_chao, 0.001)
            return {"da_ket_noi": self.da_ket_noi, "ma": self.ma, "thong_tin": self.thong_tin,
                    "goi_am": self.goi_am, "mat_am": self.mat_cu + self.bo_dem.mat,
                    "tre_tb": self.tong_tre / self.n_tre if self.n_tre else None, "tre_hien": self.tre_hien,
                    "khung_i": self.khung_i, "khung_p": self.khung_p, "khung_hien": self.khung_hien,
                    "hong": self.hong_cu + self.ghep.khung_hong, "bo_p": self.bo_p_cu + self.giai_ma.bo_p,
                    "bo_cu": self.bo_cu, "dung_hinh": self.dung_hinh, "pli_gui": self.pli_gui,
                    "khung_dau_ms": self.khung_dau_ms, "nguon_dau": self.nguon_dau,
                    "lech_hien": self.lech_hien, "giai_doan": [dict(g) for g in self.giai_doan],
                    "kbps": self.byte_nhan * 8 / 1000 / giay, "dong_bo": self.dong_bo,
                    "am_thanh": self.am_thanh, "video": self.video, "do_sau": self.do_sau,
                    "muc_am": self.muc_am, "so_hien": self.so_hien, "hien": self.hien,
                    "bo_gia_lap": self.bo_gia_lap}

    def dong(self):
        self.song = False
        self.ngat()
        if self.loa:
            self.loa.dong()


def chu_ms(x, dau=False):
    """Số ms để in: '–' nếu chưa có; dau=True in dấu +/−."""
    if x is None:
        return "–"
    return f"{x:+.0f}" if dau else f"{x:.0f}"


def dong_tong_ket(tt, tre_vao):
    """Các dòng TỔNG KẾT của người xem (dùng chung cho dòng lệnh)."""
    ds = ["===== TỔNG KẾT =====",
          (f"vào muộn {tre_vao:.1f} s; " if tre_vao > 0 else "vào ngay; ")
          + (f"khung đầu sau {tt['khung_dau_ms']:.0f} ms" if tt["khung_dau_ms"] is not None
             else "chưa có khung nào"),
          f"  nguồn khung đầu: {tt['nguon_dau'] or '–'}",
          f"tiếng: nhận {tt['goi_am']} gói, mất {tt['mat_am']}, trễ TB {chu_ms(tt['tre_tb'])} ms",
          "  (trễ đầu-cuối: giả định cùng đồng hồ)",
          f"hình: nhận {tt['khung_i'] + tt['khung_p']} khung (I {tt['khung_i']}, P {tt['khung_p']}),"
          f" hỏng {tt['hong']}",
          f"  P bỏ chờ I {tt['bo_p']}; hiện {tt['khung_hien']}, bỏ cũ {tt['bo_cu']}",
          f"đứng hình {tt['dung_hinh']}, xin khung khóa {tt['pli_gui']}",
          "lệch tiếng-hình TB (+ = hình chậm hơn tiếng):"]
    for i, g in enumerate(tt["giai_doan"], 1):
        if g["n"]:
            ds.append(f"  {i}. đồng bộ {'BẬT' if g['dong_bo'] else 'TẮT'}: {g['tong'] / g['n']:+.0f} ms"
                      f" (n={g['n']})")
        else:
            ds.append(f"  {i}. đồng bộ {'BẬT' if g['dong_bo'] else 'TẮT'}: – (không có khung)")
    ds.append(f"nhận TB {tt['kbps']:.0f} kbit/s")
    return ds


def anh_chup(c):
    """Ảnh chụp có chú thích: khung đang hiện phóng ×2 + dải chữ nhãn hình, đồng hồ tiếng, lệch.
    c = kết quả NguoiXem.chup(). Trả về (rộng, cao, rgb)."""
    w, h, anh, ts, am, lech, so = c
    W, H = 2 * w, 2 * h + 62
    ra = bytearray(phong_to(anh, w, h, 2)) + bytearray(bytes((20, 20, 20)) * (W * 62))
    ve_chu(ra, W, H, 6, 2 * h + 6, f"HINH  T={ts / 1000:.3f} #{so}", 2)
    ve_chu(ra, W, H, 6, 2 * h + 24, "TIENG " + ("-" if am is None else f"T={am / 1000:.3f}"), 2)
    if lech is None:
        chu, mau = "LECH -", (200, 200, 200)
    else:
        chu = f"LECH {lech:+.0f} MS" + (" HINH SOM" if lech < -40 else (" HINH TRE" if lech > 40 else ""))
        mau = (120, 255, 120) if abs(lech) <= 40 else (255, 120, 120)
    ve_chu(ra, W, H, 6, 2 * h + 42, chu, 2, mau)
    to_mau(ra, W, 0, 2 * h, W, 2, (255, 255, 0))
    return W, H, bytes(ra)
