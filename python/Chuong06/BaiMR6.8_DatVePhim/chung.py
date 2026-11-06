"""Khung dùng chung cho nhóm bài mở rộng "tranh chấp tài nguyên" MR6.6–MR6.14 (bài đầu: MR6.6 Đấu giá).

Tệp này GIỐNG NHAU ở cả 9 bài (chép nguyên, không sửa); nghiệp vụ riêng của từng bài nằm ở may_chu.py
(lớp con của MayChuCoSo, lớp con của SoKiemToan) và phần "NGHIỆP VỤ" của mo_phong.py. Gồm:
  1. Giao thức: gui_json, doc_dong (mỗi dòng một JSON UTF-8 kết thúc "\\n", tối đa 64 KB), ten_hop_le,
     doc_lenh_stdin ("@t lệnh"), doc_tham_so ("khoa=gia_tri").
  2. Đo đạc: phan_vi (theo hạng gần nhất), ThongKe (đếm yêu cầu, độ trễ xử lý µs, yc/s).
  3. KhoTranhChap: kho các "ô" tài nguyên và BỐN cách xử lý tranh chấp khi n luồng cùng ghi:
        khong_khoa  đọc – xử lý – ghi không đồng bộ        -> tranh chấp (race condition), mất cập nhật
        khoa        khóa loại trừ (mutex) theo từng ô, giữ suốt đọc – xử lý – ghi
        lac_quan    đọc (giá trị, phiên bản) -> xử lý ngoài khóa -> so sánh-và-đổi (CAS) dưới khóa ngắn
                    -> phiên bản đã đổi thì thử lại (tối đa 5 lần, nghỉ ngẫu nhiên 0–1 ms)
        hang_doi    mọi yêu cầu ghi xếp hàng cho MỘT luồng xử lý (mô hình actor / vòng lặp sự kiện)
  4. SoKiemToan: sổ ghi mọi giao dịch được chấp nhận (khóa riêng) và đếm sai phạm theo luật của bài.
  5. MayChuCoSo: máy chủ TCP đa luồng (một luồng mỗi kết nối): chào, quản trị, ma_yc lặp lại,
     đẩy cap_nhat gộp tối đa 10 lần/giây; lớp con chỉ viết 4 hàm nghiệp vụ.
  6. Định dạng in: tien(), dong_thong_ke(), bang_so_sanh() – các bản chuyển ngữ in GIỐNG HỆT.

Giao thức (TCP, mỗi dòng một JSON):
  client -> server  {"loai":"chao","ten":"An","vai_tro":"nguoi_dung"|"quan_tri"[,"so_nguoi":m]}  (đầu tiên)
                    {"loai":"<thao_tac>","ma_yc":k,["nguoi":"u17"],...}   (yêu cầu nghiệp vụ)
                    {"loai":"che_do","ma_yc":k,"gia_tri":"khoa"} | {"loai":"tre","ma_yc":k,"ms":2}
                    {"loai":"dat_lai","ma_yc":k,...tham số} | {"loai":"thong_ke","ma_yc":k}  (chỉ quan_tri)
  server -> client  {"loai":"chao","ma":id,"che_do":"khoa","may_chu":"..."} rồi ngay một cap_nhat
                    {"loai":"kq","ma_yc":k,"ok":true|false,["ma_loi":"..","ly_do":".."],...}
                    {"loai":"cap_nhat","che_do":"khoa",...trạng thái nghiệp vụ}  (gộp ≤ 10 lần/s)
                    {"loai":"su_kien","noi_dung":"..."} ; {"loai":"thong_ke","ma_yc":k,...}
                    {"loai":"loi","ly_do":"..."} (sai giao thức)
"so_nguoi" = m: kết nối gộp của bộ mô phỏng chở m người dùng ảo; mỗi yêu cầu ghi tên người ở "nguoi".
Một kết nối được gửi nhiều yêu cầu liên tiếp không chờ trả lời (pipelining) – trả lời ghép theo ma_yc.
Gửi lại một ma_yc đã xử lý: máy chủ KHÔNG làm lại mà gửi lại kết quả cũ kèm "lap_lai": true (idempotency).
"""
import collections
import contextlib
import itertools
import json
import math
import queue
import random
import socket
import sys
import threading
import time

GIOI_HAN_DONG = 64 * 1024            # một dòng JSON tối đa 64 KB
CHE_DO = ("khong_khoa", "khoa", "lac_quan", "hang_doi")
TEN_CHE_DO = {"khong_khoa": "Không khóa (lỗi tranh chấp)",
              "khoa": "Khóa loại trừ (mutex)",
              "lac_quan": "Lạc quan (phiên bản + CAS)",
              "hang_doi": "Hàng đợi một luồng (actor)"}
VAI_TRO = ("nguoi_dung", "quan_tri")
SO_LAN_THU = 5                       # lac_quan: số lần thử tối đa
CHO_KHOA_TOI_DA_S = 2.0              # khoa: chờ một khóa quá 2 s thì bỏ (nghi khóa chết – deadlock)
NHO_KQ = 1024                        # mỗi kết nối nhớ kết quả của 1024 ma_yc gần nhất
GIU_DO_TRE = 200_000                 # giữ tối đa 200 000 mẫu độ trễ cho phân vị
NHIP_DAY_S = 0.1                     # đẩy cap_nhat tối đa 10 lần/giây


# ================================================================ 1. Giao thức
def gui_json(sock, obj):
    """Mỗi thông điệp là một dòng JSON UTF-8 kết thúc '\\n'."""
    sock.sendall((json.dumps(obj, ensure_ascii=False) + "\n").encode("utf-8"))


def ma_hoa(obj):
    """bytes của một dòng – dùng khi gửi nhiều thông điệp trong MỘT lần sendall (pipelining)."""
    return (json.dumps(obj, ensure_ascii=False) + "\n").encode("utf-8")


def doc_dong(tep):
    """Đọc một dòng JSON từ tệp nhị phân của socket (sock.makefile("rb")).
    Trả về dict, hoặc None khi kết nối đóng; dòng > 64 KB hoặc sai dạng -> ValueError."""
    dong = tep.readline(GIOI_HAN_DONG + 1)
    if not dong:
        return None
    if len(dong) > GIOI_HAN_DONG or not dong.endswith(b"\n"):
        raise ValueError("dòng quá dài hoặc bị cắt")
    tin = json.loads(dong.decode("utf-8"))
    if not isinstance(tin, dict):
        raise ValueError("thông điệp không phải đối tượng JSON")
    return tin


def ten_hop_le(ten):
    return isinstance(ten, str) and 1 <= len(ten) <= 16 and ten.strip() == ten and ten.isprintable()


def la_so_nguyen(x):
    return isinstance(x, int) and not isinstance(x, bool)


def doc_lenh_stdin(t0, xu_ly, ket_thuc=None):
    """Đọc lệnh từ stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ t0 (đồng hồ đơn điệu)."""
    for dong in sys.stdin:
        dong = dong.strip()
        if dong.startswith("@"):
            moc, _, dong = dong.partition(" ")
            time.sleep(max(0.0, float(moc[1:]) - (time.monotonic() - t0)))
            dong = dong.strip()
        if dong:
            xu_ly(dong)
    if ket_thuc:
        ket_thuc()


def doc_tham_so(ds):
    """["thoi_gian_s=5", "ten_mon=Bình"] -> {"thoi_gian_s": 5, "ten_mon": "Bình"} (số nếu đọc được)."""
    kq = {}
    for muc in ds:
        k, dau, v = muc.partition("=")
        if not dau:
            raise ValueError(f"cần dạng khóa=giá_trị: {muc}")
        for kieu in (int, float):
            try:
                v = kieu(v)
                break
            except ValueError:
                pass
        kq[k] = v
    return kq


# ================================================================ 2. Đo đạc
def phan_vi(ds_da_sap_xep, p):
    """Phân vị p (0–100) theo HẠNG GẦN NHẤT: phần tử thứ ⌈p/100 · n⌉ (đếm từ 1) của dãy đã sắp xếp.
    Ví dụ n = 1000: p99 là phần tử thứ 990. Dãy rỗng -> None."""
    if not ds_da_sap_xep:
        return None
    return ds_da_sap_xep[max(1, math.ceil(p / 100 * len(ds_da_sap_xep))) - 1]


class ThongKe:
    """Số liệu của máy chủ từ lần đặt lại gần nhất. Mọi hàm an toàn đa luồng (một khóa nhỏ riêng)."""

    DEM = ("so_yc", "thanh_cong", "tu_choi", "xung_dot", "thu_lai", "het_lan_thu", "khoa_cho_lau",
           "hang_doi_max")

    def __init__(self):
        self.khoa = threading.Lock()
        self.dat_lai()

    def dat_lai(self):
        with self.khoa:
            self.dem = dict.fromkeys(self.DEM, 0)
            self.ma_loi = collections.Counter()          # lý do từ chối -> số lần
            self.tre_us = []                             # độ trễ xử lý từng yêu cầu (µs)
            self.t_dau = self.t_cuoi = None
            self._giay, self._dem_giay, self.yc_s = int(time.monotonic()), 0, 0

    def cong(self, ten, n=1):
        with self.khoa:
            self.dem[ten] += n

    def dat_max(self, ten, x):
        with self.khoa:
            if x > self.dem[ten]:
                self.dem[ten] = x

    def bat_dau_yc(self):
        with self.khoa:
            self.dem["so_yc"] += 1
            if self.t_dau is None:
                self.t_dau = time.monotonic()

    def xong_yc(self, kq, tre_us):
        bay_gio = time.monotonic()
        with self.khoa:
            if kq.get("ok"):
                self.dem["thanh_cong"] += 1
            else:
                self.dem["tu_choi"] += 1
                self.ma_loi[kq.get("ma_loi", "khac")] += 1
            if len(self.tre_us) < GIU_DO_TRE:
                self.tre_us.append(tre_us)
            self.t_cuoi = bay_gio
            self._sang_giay(bay_gio)
            self._dem_giay += 1

    def _sang_giay(self, bay_gio):
        """yc_s = số yêu cầu xong trong giây (đồng hồ) liền trước."""
        g = int(bay_gio)
        if g != self._giay:
            self.yc_s = self._dem_giay if g == self._giay + 1 else 0
            self._giay, self._dem_giay = g, 0

    def anh_chup(self):
        with self.khoa:
            self._sang_giay(time.monotonic())
            d = dict(self.dem)
            tre = sorted(self.tre_us)
            d["ma_loi"] = dict(self.ma_loi)
            d["yc_s"] = self.yc_s
            xong = d["thanh_cong"] + d["tu_choi"]
            khoang = (self.t_cuoi - self.t_dau) if self.t_dau is not None and self.t_cuoi else 0
            d["thong_luong"] = round(xong / khoang) if khoang > 0 else 0
        for p in (50, 95, 99):
            v = phan_vi(tre, p)
            d[f"p{p}_us"] = None if v is None else round(v)
        return d


# ================================================================ 3. Kho tài nguyên + 4 chế độ
class _O:
    """Một ô tài nguyên (món hàng, ghế, tài khoản…): giá trị, phiên bản (cho lac_quan), khóa riêng (cho khoa)."""
    __slots__ = ("gia_tri", "phien_ban", "khoa")

    def __init__(self, gia_tri):
        self.gia_tri, self.phien_ban, self.khoa = gia_tri, 0, threading.Lock()


class KhoTranhChap:
    """Kho các ô tài nguyên; mọi thao tác GHI đi qua thuc_hien() và được xử lý theo chế độ hiện tại.

    Nghiệp vụ viết hàm THUẦN  xu_ly(doc) -> (kq, ghi):
      - doc: {khóa ô: giá trị đọc được} cho các ô trong cac_khoa;
      - kq:  dict trả lời client ({"ok": True/False, "ma_loi", "ly_do", ...});
      - ghi: None (không ghi) hoặc {khóa ô: GIÁ TRỊ MỚI} – chỉ các ô trong cac_khoa. KHÔNG sửa doc tại
             chỗ (giá trị là dict thì tạo dict mới: dict(cu, gia=...)) – lac_quan có thể gọi xu_ly nhiều lần.
    khi_ghi(kq) (tùy chọn) được gọi NGAY SAU khi ghi, vẫn trong vùng găng của chế độ (dưới khóa ô với
    khoa, dưới khóa CAS với lac_quan, trong luồng xử lý với hang_doi): nơi ghi sổ kiểm toán đúng thứ tự.
    tra_loi(kq) được gọi đúng một lần khi xong (luồng gọi, hoặc luồng xử lý của hang_doi).

    "Cổng" (tam_dung): đổi chế độ / đặt lại chỉ diễn ra khi không còn thao tác nào đang chạy hay đang
    xếp hàng – không bao giờ có hai chế độ chạy lẫn vào nhau.
    """

    def __init__(self, thong_ke, che_do="khoa", tre_ms=2.0):
        if che_do not in CHE_DO:
            raise ValueError(f"chế độ không hợp lệ: {che_do}")
        self.thong_ke = thong_ke
        self.che_do = che_do
        self.tre_ms = float(tre_ms)
        self.khoa_ngay_tho = False         # khoa: True = khóa theo thứ tự cac_khoa (có thể khóa chết)
        self._o = {}
        self._khoa_cas = threading.Lock()  # lac_quan: khóa rất ngắn quanh đọc ảnh chụp và so sánh-và-đổi
        self._cong = threading.Condition()
        self._dong_cong, self._dang_chay = False, 0
        self._hang = queue.Queue()
        threading.Thread(target=self._vong_hang_doi, daemon=True).start()

    # ------------------------------------------------------------ ô tài nguyên
    def dat_o(self, khoa, gia_tri):
        self._o[khoa] = _O(gia_tri)

    def xoa_het(self):
        self._o = {}

    def doc(self, khoa):
        """Đọc không khóa (để hiển thị): giá trị được thay nguyên đối tượng nên luôn là một bản trọn vẹn."""
        return self._o[khoa].gia_tri

    def anh_chup(self):
        return {k: o.gia_tri for k, o in list(self._o.items())}

    def do_dai_hang(self):
        return self._hang.qsize()

    # ------------------------------------------------------------ cấu hình
    @contextlib.contextmanager
    def tam_dung(self):
        """Đóng cổng, chờ mọi thao tác đang chạy/đang xếp hàng xong; mở lại khi ra khỏi khối with."""
        with self._cong:
            while self._dong_cong:
                self._cong.wait()
            self._dong_cong = True
            while self._dang_chay:
                self._cong.wait()
        try:
            yield
        finally:
            with self._cong:
                self._dong_cong = False
                self._cong.notify_all()

    def dat_che_do(self, che_do):
        if che_do not in CHE_DO:
            raise ValueError(f"chế độ không hợp lệ: {che_do}")
        with self.tam_dung():
            self.che_do = che_do

    def dat_tre(self, ms):
        ms = float(ms)
        if not 0 <= ms <= 100:
            raise ValueError("độ trễ 0–100 ms")
        self.tre_ms = ms

    def _tre(self):
        """Giả lập thời gian xử lý (đọc cơ sở dữ liệu, kiểm tra nghiệp vụ): ngẫu nhiên 0 … tre_ms."""
        if self.tre_ms > 0:
            time.sleep(random.uniform(0, self.tre_ms) / 1000)

    # ------------------------------------------------------------ thực hiện một thao tác
    def thuc_hien(self, cac_khoa, xu_ly, tra_loi, khi_ghi=None):
        cac_khoa = list(dict.fromkeys(cac_khoa))          # bỏ trùng, giữ thứ tự
        if any(k not in self._o for k in cac_khoa):
            tra_loi({"ok": False, "ma_loi": "khong_co", "ly_do": "không có tài nguyên"})
            return
        with self._cong:
            while self._dong_cong:
                self._cong.wait()
            self._dang_chay += 1
            che_do = self.che_do
        if che_do == "hang_doi":
            self._hang.put((cac_khoa, xu_ly, khi_ghi, tra_loi))
            self.thong_ke.dat_max("hang_doi_max", self._hang.qsize())
            return                                        # luồng xử lý sẽ gọi tra_loi
        try:
            if che_do == "khong_khoa":
                kq = self._mot_lan(cac_khoa, xu_ly, khi_ghi)
            elif che_do == "khoa":
                kq = self._co_khoa(cac_khoa, xu_ly, khi_ghi)
            else:
                kq = self._lac_quan(cac_khoa, xu_ly, khi_ghi)
        except Exception as e:                            # lỗi nghiệp vụ: trả lời lỗi, không làm sập luồng
            kq = {"ok": False, "ma_loi": "loi", "ly_do": f"lỗi xử lý: {e}"}
        finally:
            self._xong_mot()
        tra_loi(kq)

    def _xong_mot(self):
        with self._cong:
            self._dang_chay -= 1
            if self._dang_chay == 0:
                self._cong.notify_all()

    def _ghi(self, ghi, kq, khi_ghi):
        for k, v in ghi.items():
            o = self._o[k]
            o.gia_tri = v
            o.phien_ban += 1
        if khi_ghi:
            khi_ghi(kq)

    def _mot_lan(self, cac_khoa, xu_ly, khi_ghi):
        """khong_khoa (và luồng hang_doi): đọc -> trễ -> xử lý -> ghi, KHÔNG khóa.
        Hai luồng cùng đọc giá trị cũ rồi cùng ghi: bản ghi sau đè bản trước (mất cập nhật)."""
        doc = {k: self._o[k].gia_tri for k in cac_khoa}
        self._tre()
        kq, ghi = xu_ly(doc)
        if ghi:
            self._ghi(ghi, kq, khi_ghi)
        return kq

    def _co_khoa(self, cac_khoa, xu_ly, khi_ghi):
        """khoa: giữ khóa của MỌI ô liên quan suốt đọc – xử lý – ghi (vùng găng – critical section).
        Khóa theo thứ tự khóa ô đã sắp xếp => mọi luồng xin khóa cùng một thứ tự => không khóa chết.
        khoa_ngay_tho: khóa theo thứ tự yêu cầu (ví dụ nguồn rồi đích), nghỉ giữa hai lần khóa."""
        thu_tu = cac_khoa if self.khoa_ngay_tho else sorted(cac_khoa, key=str)
        da_giu = []
        try:
            for i, k in enumerate(thu_tu):
                if i and self.khoa_ngay_tho:
                    self._tre()
                if not self._o[k].khoa.acquire(timeout=CHO_KHOA_TOI_DA_S):
                    self.thong_ke.cong("khoa_cho_lau")
                    return {"ok": False, "ma_loi": "khoa_cho_lau",
                            "ly_do": f"chờ khóa {k} quá {CHO_KHOA_TOI_DA_S:g} s (khóa chết?)"}
                da_giu.append(k)
            return self._mot_lan(cac_khoa, xu_ly, khi_ghi)
        finally:
            for k in reversed(da_giu):
                self._o[k].khoa.release()

    def _lac_quan(self, cac_khoa, xu_ly, khi_ghi):
        """lac_quan (Kung & Robinson 1981): không giữ khóa khi xử lý; lúc ghi kiểm tra phiên bản."""
        for lan in range(SO_LAN_THU):
            with self._khoa_cas:                         # ảnh chụp nhất quán của các ô
                doc = {k: self._o[k].gia_tri for k in cac_khoa}
                phien_ban = {k: self._o[k].phien_ban for k in cac_khoa}
            self._tre()
            kq, ghi = xu_ly(doc)
            if not ghi:
                return kq                                # chỉ đọc / từ chối: không cần so sánh-và-đổi
            with self._khoa_cas:                         # so sánh-và-đổi (compare-and-swap)
                if all(self._o[k].phien_ban == phien_ban[k] for k in cac_khoa):
                    self._ghi(ghi, kq, khi_ghi)
                    return kq
            self.thong_ke.cong("xung_dot")               # có luồng khác ghi trước: làm lại từ đầu
            if lan < SO_LAN_THU - 1:
                self.thong_ke.cong("thu_lai")
                time.sleep(random.uniform(0, 0.001))
        self.thong_ke.cong("het_lan_thu")
        return {"ok": False, "ma_loi": "xung_dot", "ly_do": "xung đột, thử lại sau"}

    def _vong_hang_doi(self):
        """hang_doi: MỘT luồng lấy lần lượt từng việc (đến trước phục vụ trước) – không cần khóa vì
        chỉ có luồng này chạm vào trạng thái."""
        while True:
            viec = self._hang.get()
            if viec is None:
                return
            cac_khoa, xu_ly, khi_ghi, tra_loi = viec
            try:
                kq = self._mot_lan(cac_khoa, xu_ly, khi_ghi)
            except Exception as e:
                kq = {"ok": False, "ma_loi": "loi", "ly_do": f"lỗi xử lý: {e}"}
            self._xong_mot()
            try:
                tra_loi(kq)
            except Exception:
                pass

    def dong(self):
        self._hang.put(None)


# ================================================================ 4. Sổ kiểm toán
class SoKiemToan:
    """Ghi mọi giao dịch ĐƯỢC CHẤP NHẬN (dưới khóa riêng của sổ – không ảnh hưởng tranh chấp của kho) và
    đếm sai phạm. Lớp con đặt TEN_SAI_PHAM và viết hai luật:
      _kiem_tra(gd)        – gọi khi ghi từng giao dịch (theo thứ tự ghi sổ), gọi _sai_pham(ma, mo_ta);
      _kiem_tra_cuoi(doc)  – đối chiếu sổ với trạng thái hiện tại của kho, trả về {ma: số}.
    """
    TEN_SAI_PHAM = {}                    # mã -> tên ngắn (hiện trong bảng, dòng in)

    def __init__(self):
        self.khoa = threading.Lock()
        self.giao_dich = []
        self.dem = dict.fromkeys(self.TEN_SAI_PHAM, 0)
        self.vi_du = []                  # mô tả vài sai phạm đầu tiên

    def ghi(self, **gd):
        with self.khoa:
            self.giao_dich.append(gd)
            self._kiem_tra(gd)

    def _sai_pham(self, ma, mo_ta):
        self.dem[ma] += 1
        if len(self.vi_du) < 5:
            self.vi_du.append(mo_ta)

    def _kiem_tra(self, gd):
        pass

    def _kiem_tra_cuoi(self, doc):
        return {}

    def tong_ket(self, doc):
        """-> (danh sách {"ma","ten","so"}, tổng số sai phạm, vài ví dụ)."""
        with self.khoa:
            dem = dict(self.dem)
            for ma, so in self._kiem_tra_cuoi(doc).items():
                dem[ma] += so
            vi_du = list(self.vi_du)
        ds = [{"ma": m, "ten": t, "so": dem[m]} for m, t in self.TEN_SAI_PHAM.items()]
        return ds, sum(dem.values()), vi_du


# ================================================================ 5. Máy chủ TCP đa luồng
class KhachKetNoi:
    """Một kết nối tới máy chủ (người dùng thật, kết nối gộp của bộ mô phỏng, hoặc quản trị)."""

    def __init__(self, ma, ten, vai_tro, conn, so_nguoi):
        self.ma, self.ten, self.vai_tro, self.conn = ma, ten, vai_tro, conn
        self.so_nguoi = so_nguoi                 # None: người dùng thật; m: kết nối gộp m người ảo
        self.khoa_gui = threading.Lock()         # nhiều luồng (kết nối, hàng đợi, đẩy) cùng gửi
        self.da_tra = collections.OrderedDict()  # ma_yc -> kết quả đã gửi (None: đang xử lý)
        self.so_yc = 0

    def gui(self, obj):
        try:
            with self.khoa_gui:
                gui_json(self.conn, obj)
            return True
        except OSError:
            return False

    def nhan_yc(self, ma_yc):
        """True nếu ma_yc mới. ma_yc đã gặp: gửi lại kết quả cũ (nếu đã có), trả về False."""
        with self.khoa_gui:
            if ma_yc in self.da_tra:
                cu = self.da_tra[ma_yc]
                if cu is not None:
                    try:
                        gui_json(self.conn, dict(cu, lap_lai=True))
                    except OSError:
                        pass
                return False
            self.da_tra[ma_yc] = None
            if len(self.da_tra) > NHO_KQ:
                self.da_tra.popitem(last=False)
            return True

    def tra(self, ma_yc, tin):
        with self.khoa_gui:
            self.da_tra[ma_yc] = tin
            try:
                gui_json(self.conn, tin)
            except OSError:
                pass


class MayChuCoSo:
    """Khung máy chủ. Lớp con (may_chu.py) đặt TEN, LOAI_YEU_CAU, THAM_SO_MAC_DINH và viết:
      _khoi_tao()                              tạo các ô trong self.kho, tạo self.so (SoKiemToan)
      _xu_ly_yeu_cau(khach, nguoi, tin, tra_loi)  kiểm tra tin, gọi self.kho.thuc_hien(...)
      _trang_thai()                            dict trạng thái nghiệp vụ cho cap_nhat (đọc không khóa)
      _tom_tat()                               một dòng ngắn (≤ 48 ký tự) mô tả kết quả
      _dinh_ky(bay_gio)  (tùy chọn)            gọi mỗi 100 ms trên luồng nền (đồng hồ, hết hạn…)
    Báo đổi trạng thái: bao_doi() (đẩy cap_nhat ở nhịp kế tiếp); thông báo: thong_bao(text).
    """
    TEN = "Máy chủ"
    LOAI_YEU_CAU = ()
    THAM_SO_MAC_DINH = {}

    def __init__(self, che_do="khoa", tre_ms=2.0, bao=None, **tham_so):
        self.bao = bao or (lambda text: None)
        self.thong_ke = ThongKe()
        self.kho = KhoTranhChap(self.thong_ke, che_do, tre_ms)
        self.tham_so = dict(self.THAM_SO_MAC_DINH)
        self._kiem_tham_so(tham_so)
        self.tham_so.update(tham_so)
        self.khoa = threading.Lock()             # bảo vệ self.ds (danh sách kết nối)
        self.ds = {}
        self.chay = False
        self.dia_chi = ""
        self._doi = False
        self._su_kien = collections.deque()
        self._luong, self._srv = [], None
        self.so = None
        self._khoi_tao_lai()

    # ------------------------------------------------------------ phần nghiệp vụ (lớp con viết)
    def _khoi_tao(self):
        raise NotImplementedError

    def _xu_ly_yeu_cau(self, khach, nguoi, tin, tra_loi):
        raise NotImplementedError

    def _trang_thai(self):
        return {}

    def _tom_tat(self):
        return ""

    def _dinh_ky(self, bay_gio):
        pass

    def _kiem_tham_so(self, ts):
        """Kiểm tra tham số đặt lại; lớp con mở rộng (raise ValueError)."""
        for k in ts:
            if k not in self.THAM_SO_MAC_DINH:
                raise ValueError(f"tham số lạ: {k}")

    # ------------------------------------------------------------ điều khiển
    def _khoi_tao_lai(self):
        with self.kho.tam_dung():                # không còn thao tác nào đang chạy
            self.kho.xoa_het()
            self.thong_ke.dat_lai()
            self._khoi_tao()
        self.bao_doi()

    def dat_lai(self, **tham_so):
        ts = {}
        for k, v in tham_so.items():             # ép kiểu theo giá trị mặc định
            if k not in self.THAM_SO_MAC_DINH:
                raise ValueError(f"tham số lạ: {k}")
            ts[k] = type(self.THAM_SO_MAC_DINH[k])(v)
        self._kiem_tham_so(ts)
        tk = self.thong_ke.anh_chup()
        if tk["so_yc"]:
            _, sp, _ = self.so.tong_ket(self.kho.anh_chup())
            self.bao(f"Lượt trước: {self.kho.che_do}, {tk['so_yc']} yc, tc {tk['thanh_cong']},"
                     f" sai phạm {sp}")
        self.tham_so.update(ts)
        self._khoi_tao_lai()
        self.thong_bao("Đặt lại" + ("" if not ts else " (" + ", ".join(f"{k}={v}" for k, v in ts.items()) + ")"))

    def dat_che_do(self, che_do):
        if che_do == self.kho.che_do:
            return
        self.kho.dat_che_do(che_do)
        self.thong_bao(f"Chế độ: {che_do} – {TEN_CHE_DO[che_do]}")
        self.bao_doi()

    def dat_tre(self, ms):
        self.kho.dat_tre(ms)
        self.bao(f"Độ trễ xử lý: ngẫu nhiên 0–{self.kho.tre_ms:g} ms")

    def bao_doi(self):
        self._doi = True

    def thong_bao(self, text):
        """Ghi nhật ký máy chủ và gửi {"loai":"su_kien"} cho mọi client (ở nhịp đẩy kế tiếp)."""
        self.bao(text)
        self._su_kien.append(text)

    # ------------------------------------------------------------ khởi động / dừng
    def bat_dau(self, host, cong):
        srv = socket.create_server((host, cong), backlog=1024)
        srv.settimeout(0.5)
        self._srv, self.dia_chi = srv, f"{host}:{cong}"
        self._ma = itertools.count(1)
        self.chay = True
        self._luong = [threading.Thread(target=self._nhan_ket_noi, daemon=True),
                       threading.Thread(target=self._vong_nen, daemon=True)]
        for t in self._luong:
            t.start()
        self.bao(f"Lắng nghe {host}:{cong}; chế độ {self.kho.che_do}, trễ 0–{self.kho.tre_ms:g} ms")

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
        self._srv.close()
        self.bao("Máy chủ đã dừng")

    # ------------------------------------------------------------ kết nối
    def _nhan_ket_noi(self):
        while self.chay:
            try:
                conn, addr = self._srv.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            threading.Thread(target=self._phuc_vu, args=(conn,), daemon=True).start()

    def _phuc_vu(self, conn):
        """Luồng của một kết nối: chào, rồi đọc và xử lý lần lượt từng yêu cầu (có thể pipelining)."""
        conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        tep = conn.makefile("rb")
        k = None
        try:
            tin = doc_dong(tep)
            so_nguoi = tin.get("so_nguoi") if tin else None
            if (not tin or tin.get("loai") != "chao" or not ten_hop_le(tin.get("ten"))
                    or tin.get("vai_tro", "nguoi_dung") not in VAI_TRO
                    or (so_nguoi is not None and not (la_so_nguyen(so_nguoi) and so_nguoi > 0))):
                gui_json(conn, {"loai": "loi", "ly_do": "cần chào: tên 1–16 ký tự, vai_tro hợp lệ"})
                return
            k = KhachKetNoi(next(self._ma), tin["ten"], tin.get("vai_tro", "nguoi_dung"), conn, so_nguoi)
            with k.khoa_gui:                   # luồng đẩy chưa gửi được gì cho k trước lời chào
                with self.khoa:
                    self.ds[k.ma] = k
                gui_json(conn, {"loai": "chao", "ma": k.ma, "che_do": self.kho.che_do, "may_chu": self.TEN})
                gui_json(conn, self._goi_cap_nhat())
            if so_nguoi is None and k.vai_tro == "nguoi_dung":   # kết nối gộp, quản trị: không in
                self.bao(f"{k.ten} vào")
            while self.chay:
                tin = doc_dong(tep)
                if tin is None:
                    break
                self._nhan_tin(k, tin)
        except (OSError, ValueError) as e:
            if k is not None and self.chay and k.so_nguoi is None:
                self.bao(f"{k.ten}: lỗi kết nối ({e})")
        finally:
            if k is not None:
                with self.khoa:
                    self.ds.pop(k.ma, None)
                if k.so_nguoi is None and k.vai_tro == "nguoi_dung" and self.chay:
                    self.bao(f"{k.ten} rời")
            try:
                conn.close()
            except OSError:
                pass

    def _nhan_tin(self, k, tin):
        loai, ma_yc = tin.get("loai"), tin.get("ma_yc")
        if not la_so_nguyen(ma_yc):
            k.gui({"loai": "loi", "ly_do": "thiếu ma_yc (số nguyên)"})
            return
        if not k.nhan_yc(ma_yc):
            return                                       # ma_yc lặp lại: đã gửi lại kết quả cũ
        k.so_yc += 1
        if loai in self.LOAI_YEU_CAU:
            nguoi = tin.get("nguoi") if k.so_nguoi is not None else k.ten
            if not ten_hop_le(nguoi):
                k.tra(ma_yc, {"loai": "kq", "ma_yc": ma_yc, "ok": False, "ma_loi": "sai_du_lieu",
                              "ly_do": "thiếu tên người (nguoi)"})
                return
            t0 = time.perf_counter()
            self.thong_ke.bat_dau_yc()

            def tra_loi(kq):                             # gọi từ luồng kết nối hoặc luồng hàng đợi
                self.thong_ke.xong_yc(kq, (time.perf_counter() - t0) * 1e6)
                k.tra(ma_yc, {"loai": "kq", "ma_yc": ma_yc, **kq})
            self._xu_ly_yeu_cau(k, nguoi, tin, tra_loi)
        elif loai in ("che_do", "tre", "dat_lai", "thong_ke"):
            if k.vai_tro != "quan_tri":
                k.tra(ma_yc, {"loai": "kq", "ma_yc": ma_yc, "ok": False, "ma_loi": "khong_quyen",
                              "ly_do": "chỉ quản trị"})
                return
            try:
                if loai == "thong_ke":
                    k.tra(ma_yc, {"loai": "thong_ke", "ma_yc": ma_yc, **self.thong_ke_day_du()})
                    return
                if loai == "che_do":
                    self.dat_che_do(tin.get("gia_tri"))
                elif loai == "tre":
                    self.dat_tre(tin.get("ms"))
                else:
                    self.dat_lai(**{x: v for x, v in tin.items() if x not in ("loai", "ma_yc")})
                k.tra(ma_yc, {"loai": "kq", "ma_yc": ma_yc, "ok": True})
            except (TypeError, ValueError) as e:
                k.tra(ma_yc, {"loai": "kq", "ma_yc": ma_yc, "ok": False, "ma_loi": "sai_du_lieu",
                              "ly_do": str(e)})
        else:
            k.tra(ma_yc, {"loai": "kq", "ma_yc": ma_yc, "ok": False, "ma_loi": "khong_ho_tro",
                          "ly_do": f"loại không hỗ trợ: {loai}"})

    # ------------------------------------------------------------ đẩy trạng thái (gộp ≤ 10 lần/giây)
    def _goi_cap_nhat(self):
        return {"loai": "cap_nhat", "che_do": self.kho.che_do, **self._trang_thai()}

    def _vong_nen(self):
        while self.chay:
            time.sleep(NHIP_DAY_S)
            try:
                self._dinh_ky(time.monotonic())
            except Exception as e:
                self.bao(f"lỗi định kỳ: {e}")
            goi = []
            while self._su_kien:
                goi.append(ma_hoa({"loai": "su_kien", "noi_dung": self._su_kien.popleft()}))
            if self._doi:
                self._doi = False                        # xóa cờ TRƯỚC khi chụp: đổi sau đó vẫn được đẩy
                goi.append(ma_hoa(self._goi_cap_nhat()))
            if not goi:
                continue
            du_lieu = b"".join(goi)
            with self.khoa:
                ds = list(self.ds.values())
            for k in ds:
                try:
                    with k.khoa_gui:
                        k.conn.sendall(du_lieu)
                except OSError:
                    pass

    # ------------------------------------------------------------ số liệu
    def trang_thai(self):
        """Trạng thái hiện tại (như cap_nhat gửi client) – giao diện máy chủ đọc định kỳ."""
        return self._goi_cap_nhat()

    def so_ket_noi(self):
        with self.khoa:
            ds = list(self.ds.values())
        nd = [k for k in ds if k.vai_tro != "quan_tri"]
        return len(ds), sum(k.so_nguoi or 1 for k in nd)

    def thong_ke_day_du(self):
        tk = self.thong_ke.anh_chup()
        ds_sp, tong_sp, vi_du = self.so.tong_ket(self.kho.anh_chup())
        so_kn, so_nguoi = self.so_ket_noi()
        return {"che_do": self.kho.che_do, "tre_ms": self.kho.tre_ms, **tk,
                "hang_doi": self.kho.do_dai_hang(), "sai_pham": ds_sp, "sai_pham_tong": tong_sp,
                "vi_du_sai_pham": vi_du[:3], "tom_tat": self._tom_tat(),
                "so_ket_noi": so_kn, "so_nguoi": so_nguoi}


# ================================================================ 6. Định dạng in (giống nhau ở 5 ngôn ngữ)
def tien(x):
    """5300000 -> "5.300.000 đ" (dấu chấm ngăn nghìn)."""
    return f"{x:,}".replace(",", ".") + " đ"


def so_us(x):
    return "–" if x is None else f"{x}"


def dong_sai_pham(tk):
    """"Sai phạm 37: giá thấp hơn 36, giá cuối sai 1"."""
    return f"Sai phạm {tk['sai_pham_tong']}: " + ", ".join(f"{s['ten']} {s['so']}" for s in tk["sai_pham"])


def dong_thong_ke(tk):
    """Các dòng (≤ 64 ký tự) in số liệu máy chủ – dùng ở server.py, khach.py, mo_phong.py."""
    d = tk
    return [f"Chế độ {d['che_do']}, trễ xử lý 0–{d['tre_ms']:g} ms",
            f"Yêu cầu {d['so_yc']}: thành công {d['thanh_cong']}, từ chối {d['tu_choi']}",
            f"Xung đột {d['xung_dot']}, thử lại {d['thu_lai']}, hết lượt thử {d['het_lan_thu']}",
            f"Hàng đợi max {d['hang_doi_max']}, chờ khóa quá hạn {d['khoa_cho_lau']}",
            f"Xử lý µs: p50 {so_us(d['p50_us'])} p95 {so_us(d['p95_us'])} p99 {so_us(d['p99_us'])}",
            f"Thông lượng {d['thong_luong']} yc/s",
            dong_sai_pham(d),
            d["tom_tat"]]


TIEU_DE_SO_SANH = "  Chế độ       Thành công Sai phạm Xung đột  yc/s  p99 ms"


def bang_so_sanh(hang):
    """hang: danh sách dict {che_do, thanh_cong, sai_pham, xung_dot, yc_s, p99_ms} -> các dòng bảng."""
    ds = [TIEU_DE_SO_SANH]
    for h in hang:
        p99 = "–" if h["p99_ms"] is None else f"{h['p99_ms']:.1f}"
        ds.append(f"  {h['che_do']:<12} {h['thanh_cong']:>10} {h['sai_pham']:>8} {h['xung_dot']:>8}"
                  f" {h['yc_s']:>5} {p99:>7}")
    return ds
