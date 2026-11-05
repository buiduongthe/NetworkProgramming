"""Bài mở rộng MR6.13 – Lõi máy chủ bán vé có phòng chờ ảo: nghiệp vụ + sổ kiểm toán (khung ở chung.py).

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
Bán 500 vé concert cho rất nhiều người cùng lúc. Ba thao tác của người dùng:
    {"loai":"xep_hang","ma_yc":k}            vào phòng chờ -> số thứ tự, số người phía trước, thời gian chờ ước tính
    {"loai":"cho_luot","ma_yc":k}            "chờ dài" (long polling): máy chủ CHỈ trả lời khi tới lượt (kèm
                                             han_ms – thời gian còn lại để mua) hoặc khi hết vé
    {"loai":"mua","ma_yc":k,"so_ve":2}       mua 1–4 vé; khi phòng chờ bật phải đang có lượt còn hạn
(kết nối gộp của bộ mô phỏng thêm "nguoi":"u17").

Phòng chờ ảo (virtual waiting room): máy chủ cấp lượt theo THÙNG TOKEN (token bucket): token rót vào đều
toc_do token/giây, thùng chứa tối đa dung_luong token; mỗi 100 ms luồng nền lấy người đầu hàng (số thứ tự
nhỏ nhất) ra, mỗi người tốn một token -> lượt có hạn luot_s giây (mặc định 20 s) để mua. Nhờ vậy phía "mua"
(phần đắt: kiểm tra, ghi kho) chỉ nhận đều đặn vài chục người mỗi giây dù hàng chờ có hàng nghìn người.
Tắt phòng chờ (phong_cho = 0): ai cũng gọi mua ngay -> dồn cục; máy chủ chỉ cho tối đa gioi_han_xu_ly yêu
cầu mua đang xử lý cùng lúc, quá số đó từ chối ngay "qua_tai" (như HTTP 503) -> nhiều từ chối, độ trễ cao.

Hai ô của KhoTranhChap (đi qua 4 cách xử lý tranh chấp):
  "hang" {"cuoi": số thứ tự cuối đã phát}  – xep_hang đọc, +1, ghi: phát số thứ tự;
  "kho"  {"con": vé còn, "da_ban": đã bán} – mua đọc, kiểm tra số vé, trừ kho;
  "luot:<tên>" {"con": còn lượt?, "han": hạn} – tạo khi cấp lượt; mua đọc cùng "kho" và ghi con = False
               trong CÙNG thao tác (tiêu lượt nguyên tử với trừ kho).
Sổ kiểm toán đếm ba sai phạm:
  ban_qua    "vượt vé"  – số vé bán vượt tổng số vé (theo sổ: cộng số vé mọi đơn được chấp nhận);
  khong_luot "sai lượt" – một đơn được chấp nhận khi người đó không còn lượt (lượt đã dùng ở tab khác);
  dao_thu_tu "đảo số"   – người vào hàng sau nhận số thứ tự không lớn hơn người vào trước (FIFO bị phá).
"""
import collections
import heapq
import itertools
import threading
import time

from chung import MayChuCoSo, SoKiemToan, la_so_nguyen


class SoKiemToanPhongCho(SoKiemToan):
    TEN_SAI_PHAM = {"ban_qua": "vượt vé", "khong_luot": "sai lượt", "dao_thu_tu": "đảo số"}

    def __init__(self, tong_ve):
        super().__init__()
        self.tong_ve = tong_ve
        self.da_ban = 0                  # tổng vé theo sổ
        self.so_max = 0                  # số thứ tự lớn nhất đã phát (theo thứ tự ghi sổ)
        self.luot = set()                # người đang có lượt chưa dùng (theo sổ)

    def _kiem_tra(self, gd):
        loai = gd["loai"]
        if loai == "xep":
            if gd["so"] <= self.so_max:
                self._sai_pham("dao_thu_tu", f"{gd['nguoi']} vào sau nhưng nhận số {gd['so']} ≤ {self.so_max}")
            else:
                self.so_max = gd["so"]
        elif loai == "luot":
            self.luot.add(gd["nguoi"])
        elif loai == "mua":
            if gd["can_luot"]:
                if gd["nguoi"] in self.luot:
                    self.luot.discard(gd["nguoi"])
                else:
                    self._sai_pham("khong_luot", f"{gd['nguoi']} mua {gd['so_ve']} vé khi lượt đã dùng")
            truoc, self.da_ban = self.da_ban, self.da_ban + gd["so_ve"]
            vuot = self.da_ban - max(truoc, self.tong_ve)
            if vuot > 0:                 # đếm theo SỐ VÉ bán vượt
                self.dem["ban_qua"] += vuot - 1
                self._sai_pham("ban_qua", f"{gd['nguoi']} mua {gd['so_ve']} vé, sổ đã bán {self.da_ban}")


KQ_HET_VE = {"ok": False, "ma_loi": "het_ve", "ly_do": "đã bán hết vé"}


class MayChuPhongCho(MayChuCoSo):
    TEN = "Bán vé MR6.13"
    LOAI_YEU_CAU = ("xep_hang", "cho_luot", "mua")
    THAM_SO_MAC_DINH = {"ten_su_kien": "Đêm nhạc Mùa Đông 2026", "gia_ve": 1_500_000, "tong_ve": 500,
                        "ve_toi_da": 4, "phong_cho": 1, "toc_do": 50.0, "dung_luong": 50, "luot_s": 20.0,
                        "gioi_han_xu_ly": 64}

    # ------------------------------------------------------------ tạo / đặt lại
    def _kiem_tham_so(self, ts):
        super()._kiem_tham_so(ts)
        if ts.get("tong_ve", 1) < 1 or not 1 <= ts.get("ve_toi_da", 1) <= 10 or ts.get("gia_ve", 0) < 0:
            raise ValueError("tổng vé ≥ 1, vé tối đa 1–10, giá ≥ 0")
        if ts.get("phong_cho", 0) not in (0, 1):
            raise ValueError("phong_cho = 0 hoặc 1")
        if not 0 < ts.get("toc_do", 1) <= 100_000 or ts.get("dung_luong", 1) < 1:
            raise ValueError("tốc độ cấp lượt 0–100000 người/s, dung lượng thùng ≥ 1")
        if ts.get("luot_s", 1) <= 0 or ts.get("gioi_han_xu_ly", 1) < 1:
            raise ValueError("thời hạn lượt > 0, giới hạn xử lý ≥ 1")
        if "ten_su_kien" in ts and not 1 <= len(str(ts["ten_su_kien"])) <= 40:
            raise ValueError("tên sự kiện 1–40 ký tự")

    def _khoi_tao(self):
        ts = self.tham_so
        self.kho.dat_o("kho", {"con": ts["tong_ve"], "da_ban": 0})
        self.kho.dat_o("hang", {"cuoi": 0})
        self.so = SoKiemToanPhongCho(ts["tong_ve"])
        if not hasattr(self, "khoa_pc"):
            self.khoa_pc = threading.Lock()          # bảo vệ phòng chờ (danh sách người, hàng, thùng token)
            self._ma_don = itertools.count(1)        # mã đơn tăng mãi, kể cả qua các lần đặt lại
            self.nguoi = {}
        with self.khoa_pc:
            cu = [cb for ng in self.nguoi.values() for cb in ng["cho"]]
            self.nguoi = {}                          # tên -> {"so","trang_thai","han","cho":[tra_loi…]}
            self.heap = []                           # (số thứ tự, thứ tự ghi, tên) của người đang chờ
            self._stt = itertools.count()
            self.co_luot = collections.deque()       # (hạn, tên) theo thứ tự cấp lượt – để thu hồi lượt quá hạn
            self.dang_phuc_vu = 0                    # số thứ tự lớn nhất đã được cấp lượt
            self.dang_cho = self.da_cap_luot = self.het_han = 0
            self.token, self.t_token = float(ts["dung_luong"]), time.monotonic()
            self.het_ve = False
            self.dang_xu_ly = self.xu_ly_max = self.qua_tai = 0
            self.so_don = 0
            self.lich_su = collections.deque(maxlen=30)
        for cb in cu:                                # ai đang "chờ dài" từ phiên trước: trả lời ngay
            cb({"ok": False, "ma_loi": "dat_lai", "ly_do": "phiên bán vé được đặt lại"})

    def dat_toc_do(self, toc_do):
        """Đổi tốc độ cấp lượt ngay khi đang chạy (không đặt lại phiên)."""
        toc_do = float(toc_do)
        self._kiem_tham_so({"toc_do": toc_do})
        self.tham_so["toc_do"] = toc_do
        self.thong_bao(f"Tốc độ cấp lượt: {toc_do:g} người/s")
        self.bao_doi()

    # ------------------------------------------------------------ yêu cầu
    def _xu_ly_yeu_cau(self, khach, nguoi, tin, tra_loi):
        loai = tin["loai"]
        if loai == "mua":
            self._mua(nguoi, tin, tra_loi)
        elif not self.tham_so["phong_cho"]:
            tra_loi({"ok": False, "ma_loi": "pc_tat", "ly_do": "phòng chờ đang tắt – mua trực tiếp"})
        elif loai == "xep_hang":
            self._xep_hang(nguoi, tra_loi)
        else:
            self._cho_luot(nguoi, tra_loi)

    def _vi_tri(self, ng):
        """Số người phía trước và thời gian chờ ước tính (gọi dưới khoa_pc)."""
        truoc = max(0, ng["so"] - self.dang_phuc_vu - 1) if ng["trang_thai"] == "cho" else 0
        return {"so": ng["so"], "truoc": truoc, "cho_s": round((truoc + 1) / self.tham_so["toc_do"], 1),
                "trang_thai": ng["trang_thai"]}

    def _xep_hang(self, nguoi, tra_loi):
        kq = None
        with self.khoa_pc:
            if self.het_ve:
                kq = KQ_HET_VE
            elif nguoi in self.nguoi:                # đã trong hàng (tải lại trang, tab thứ hai): giữ chỗ cũ
                kq = {"ok": True, "da_co": True, **self._vi_tri(self.nguoi[nguoi])}
        if kq:
            tra_loi(kq)
            return

        def xu_ly(doc):                              # phát số thứ tự: đọc – cộng 1 – ghi
            so = doc["hang"]["cuoi"] + 1
            return {"ok": True, "so": so}, {"hang": {"cuoi": so}}

        def khi_ghi(kq):                             # trong vùng găng: đưa vào hàng, ghi sổ
            with self.khoa_pc:
                ng = self.nguoi.get(nguoi)
                moi = ng is None
                if moi and self.het_ve:              # vừa hết vé trong lúc xếp hàng: cho_luot báo hết vé ngay
                    ng = self.nguoi[nguoi] = {"so": kq["so"], "trang_thai": "het_ve", "han": None, "cho": []}
                elif moi:
                    ng = self.nguoi[nguoi] = {"so": kq["so"], "trang_thai": "cho", "han": None, "cho": []}
                    heapq.heappush(self.heap, (kq["so"], next(self._stt), nguoi))
                    self.dang_cho += 1
                else:
                    kq["da_co"] = True
                kq.update(self._vi_tri(ng))
            if moi:
                self.so.ghi(loai="xep", nguoi=nguoi, so=kq["so"])
            self.bao_doi()
        self.kho.thuc_hien(["hang"], xu_ly, tra_loi, khi_ghi=khi_ghi)

    def _cho_luot(self, nguoi, tra_loi):
        """Chờ dài: chưa tới lượt thì GIỮ tra_loi, luồng nền gọi khi cấp lượt / hết vé."""
        with self.khoa_pc:
            ng = self.nguoi.get(nguoi)
            if ng is None:
                kq = {"ok": False, "ma_loi": "chua_xep_hang", "ly_do": "chưa vào hàng chờ"}
            elif ng["trang_thai"] == "cho":
                ng["cho"].append(tra_loi)
                return
            elif ng["trang_thai"] == "luot":
                kq = {"ok": True, "luot": True, "so": ng["so"],
                      "han_ms": max(0, round((ng["han"] - time.monotonic()) * 1000))}
            elif ng["trang_thai"] == "da_mua":
                kq = {"ok": False, "ma_loi": "da_mua", "ly_do": "lượt này đã dùng để mua"}
            elif ng["trang_thai"] == "het_han":
                kq = {"ok": False, "ma_loi": "het_luot", "ly_do": "lượt mua đã hết hạn"}
            else:
                kq = KQ_HET_VE
        tra_loi(kq)

    def _mua(self, nguoi, tin, tra_loi):
        ts = self.tham_so
        so_ve = tin.get("so_ve")
        if not la_so_nguyen(so_ve) or not 1 <= so_ve <= ts["ve_toi_da"]:
            tra_loi({"ok": False, "ma_loi": "sai_du_lieu", "ly_do": f"số vé 1–{ts['ve_toi_da']}"})
            return
        with self.khoa_pc:                           # giới hạn số yêu cầu mua đang xử lý cùng lúc
            qua_tai = self.dang_xu_ly >= ts["gioi_han_xu_ly"]
            if qua_tai:
                self.qua_tai += 1
            else:
                self.dang_xu_ly += 1
                self.xu_ly_max = max(self.xu_ly_max, self.dang_xu_ly)
        if qua_tai:
            tra_loi({"ok": False, "ma_loi": "qua_tai", "ly_do": "hệ thống quá tải, vui lòng thử lại"})
            return
        pc, gia = ts["phong_cho"], ts["gia_ve"]
        o_luot = f"luot:{nguoi}"                     # ô lượt của người này (tạo khi được cấp lượt)

        def xu_ly(doc):                              # kiểm tra lượt + số vé còn; trừ kho VÀ tiêu lượt
            k = doc["kho"]
            if pc:
                luot = doc[o_luot]
                if not luot["con"]:
                    return {"ok": False, "ma_loi": "da_mua", "ly_do": "lượt này đã dùng để mua"}, None
                if time.monotonic() > luot["han"]:
                    return {"ok": False, "ma_loi": "het_luot", "ly_do": "lượt mua đã hết hạn"}, None
            if k["con"] <= 0:
                return dict(KQ_HET_VE, con=0), None
            if k["con"] < so_ve:
                return {"ok": False, "ma_loi": "het_ve", "ly_do": f"chỉ còn {k['con']} vé",
                        "con": k["con"]}, None
            ghi = {"kho": {"con": k["con"] - so_ve, "da_ban": k["da_ban"] + so_ve}}
            if pc:
                ghi[o_luot] = dict(doc[o_luot], con=False)
            return {"ok": True, "so_ve": so_ve, "thanh_tien": so_ve * gia, "con": k["con"] - so_ve}, ghi

        def khi_ghi(kq):                             # trong vùng găng: tiêu lượt, ghi sổ
            kq["ma_don"] = next(self._ma_don)
            if pc:
                with self.khoa_pc:
                    ng = self.nguoi.get(nguoi)
                    if ng:
                        ng["trang_thai"] = "da_mua"
            self.so.ghi(loai="mua", nguoi=nguoi, so_ve=so_ve, can_luot=bool(pc))
            self.so_don += 1
            self.lich_su.appendleft({"ma": kq["ma_don"], "gio": time.strftime("%H:%M:%S"), "nguoi": nguoi,
                                     "so_ve": so_ve})
            self.bao_doi()

        def tra(kq):
            with self.khoa_pc:
                self.dang_xu_ly -= 1
            if kq.get("ma_loi") == "khong_co":       # chưa có ô lượt: chưa được cấp lượt
                kq = {"ok": False, "ma_loi": "khong_luot", "ly_do": "chưa tới lượt – hãy vào hàng chờ"}
            tra_loi(kq)
        self.kho.thuc_hien(["kho", o_luot] if pc else ["kho"], xu_ly, tra, khi_ghi=khi_ghi)

    # ------------------------------------------------------------ luồng nền (100 ms): thùng token
    def _dinh_ky(self, bay_gio):
        ts = self.tham_so
        k = self._doc("kho")
        if k is None:
            return                                   # đang đặt lại
        tha, bao_het = [], False
        with self.khoa_pc:
            while self.co_luot and self.co_luot[0][0] <= bay_gio:      # thu hồi lượt quá hạn
                han, ten = self.co_luot.popleft()
                ng = self.nguoi.get(ten)
                if ng and ng["trang_thai"] == "luot" and ng["han"] == han:
                    ng["trang_thai"] = "het_han"
                    self.het_han += 1
            if k["con"] <= 0 and not self.het_ve:   # hết vé: báo mọi người đang chờ, đóng hàng
                self.het_ve = bao_het = True
                for ng in self.nguoi.values():
                    if ng["trang_thai"] == "cho":
                        ng["trang_thai"] = "het_ve"
                        tha += [(cb, KQ_HET_VE) for cb in ng["cho"]]
                        ng["cho"] = []
                self.heap, self.dang_cho = [], 0
            elif ts["phong_cho"] and not self.het_ve:
                self.token = min(float(ts["dung_luong"]), self.token + ts["toc_do"] * (bay_gio - self.t_token))
                while self.heap and self.token >= 1:
                    so, _, ten = heapq.heappop(self.heap)
                    ng = self.nguoi.get(ten)
                    if ng is None or ng["trang_thai"] != "cho" or ng["so"] != so:
                        continue
                    self.token -= 1
                    self.dang_cho -= 1
                    self.so.ghi(loai="luot", nguoi=ten)        # sổ biết lượt TRƯỚC khi người đó thấy lượt
                    ng["trang_thai"], ng["han"] = "luot", bay_gio + ts["luot_s"]
                    self.kho.dat_o(f"luot:{ten}", {"con": True, "han": ng["han"]})   # ô mới: chưa ai chạm tới
                    self.co_luot.append((ng["han"], ten))
                    self.dang_phuc_vu = max(self.dang_phuc_vu, so)
                    self.da_cap_luot += 1
                    kq = {"ok": True, "luot": True, "so": so, "han_ms": round(ts["luot_s"] * 1000)}
                    tha += [(cb, kq) for cb in ng["cho"]]
                    ng["cho"] = []
            self.t_token = bay_gio
        if bao_het:
            self.thong_bao(f"Đã bán hết {ts['tong_ve']} vé")
        for cb, kq in tha:
            cb(kq)
        if tha:
            self.bao_doi()

    # ------------------------------------------------------------ trạng thái gửi client
    def _doc(self, o):
        try:
            return self.kho.doc(o)
        except KeyError:                             # đang đặt lại
            return None

    def _trang_thai(self):
        ts = self.tham_so
        k = self._doc("kho") or {"con": 0, "da_ban": 0}
        return {"su_kien": {"ten": ts["ten_su_kien"], "gia_ve": ts["gia_ve"], "tong_ve": ts["tong_ve"],
                            "ve_toi_da": ts["ve_toi_da"], "luot_s": ts["luot_s"]},
                "phong_cho": bool(ts["phong_cho"]), "con_lai": k["con"], "da_ban": k["da_ban"],
                "toc_do": ts["toc_do"], "dung_luong": ts["dung_luong"], "token": int(self.token),
                "dang_phuc_vu": self.dang_phuc_vu, "so_cap": (self._doc("hang") or {"cuoi": 0})["cuoi"],
                "dang_cho": self.dang_cho, "da_cap_luot": self.da_cap_luot, "het_han": self.het_han,
                "het_ve": self.het_ve, "dang_xu_ly": self.dang_xu_ly, "xu_ly_max": self.xu_ly_max,
                "gioi_han": ts["gioi_han_xu_ly"], "qua_tai": self.qua_tai, "so_don": self.so_don,
                "ban_so": self.so.da_ban, "lich_su": list(self.lich_su)[:10]}

    def _tom_tat(self):
        k = self._doc("kho") or {"con": 0}
        return f"Sổ: bán {self.so.da_ban}/{self.tham_so['tong_ve']} vé, {self.so_don} đơn; kho còn {k['con']}"


_khoa_in = threading.Lock()


def in_dong(tien_to, text):
    """Nhiều luồng cùng in: mỗi dòng in trọn vẹn."""
    with _khoa_in:
        print(f"[{tien_to}] {text}", flush=True)
