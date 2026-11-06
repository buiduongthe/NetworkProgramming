"""Bài mở rộng MR6.14 – Lõi máy chủ đặt vé xe khách theo chặng: nghiệp vụ + sổ kiểm toán (khung ở chung.py).

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối) và MR6.8.
Một chuyến xe chạy qua các trạm Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang (3 chặng). Hai loại xe:
    giuong – xe giường nằm 2 tầng, 40 giường: tầng dưới D01–D20, tầng trên T01–T20 (mỗi tầng 3 dãy × 7 hàng,
             hàng cuối 2 giường);
    ghe    – xe ghế ngồi 50 chỗ 01–50 (11 hàng × 4 ghế, lối đi ở giữa, hàng cuối 6 ghế liền).
Khách đặt một ĐOẠN đường [tu, den) (chỉ số trạm, tu < den), nên cùng một giường bán được cho nhiều người
nếu các đoạn không chồng nhau (An Sài Gòn → Đà Lạt, Chi Đà Lạt → Nha Trang). Ba thao tác ghi:
    {"loai":"giu_cho","ma_yc":k,"ghe":["D01"],"tu":0,"den":2[,"nguoi":"u17"]}   giữ chỗ có thời hạn (TTL)
    {"loai":"thanh_toan","ma_yc":k,"ghe":["D01"],"tu":0,"den":2}                 chỗ mình đang giữ -> vé
    {"loai":"nha","ma_yc":k,"ghe":["D01"],"tu":0,"den":2}                        trả lại chỗ đang giữ
Mỗi cặp (ghế, chặng) là MỘT ô của KhoTranhChap, tên ô "D01#1" (chặng 1 = Bảo Lộc → Đà Lạt):
    {"tt":"trong"|"giu"|"ban","nguoi","het_han"(đồng hồ đơn điệu),"ma_ve","nguoi_het_han"}
Giữ 2 giường cho đoạn 3 chặng = khóa 6 ô theo thứ tự tên ô (không khóa chết). Hai đoạn không chồng nhau
chạm vào các ô khác nhau, nên ở chế độ khoa chúng chạy song song dù là cùng một giường.
Luật như MR6.8: giữ được khi MỌI ô đều trống (hoặc lượt giữ cũ đã hết hạn, hoặc đã là của mình); giữ hết
hạn sau ttl_s giây – luồng nền (100 ms/lần) dọn ô quá hạn; thanh toán chỉ nhận ô mình giữ, còn hạn.

Sổ kiểm toán đếm hai sai phạm:
  trung_cho – một (ghế, chặng) được giữ/bán cho người thứ hai khi người khác đang giữ (còn hạn) hoặc đã mua;
  tt_sai    – thanh toán được chấp nhận cho chỗ không do mình giữ hoặc lượt giữ đã hết hạn.
"""
import itertools
import threading
import time

from chung import MayChuCoSo, SoKiemToan, tien

TRAM = ["Sài Gòn", "Bảo Lộc", "Đà Lạt", "Nha Trang"]
MA_TRAM = ["SG", "BL", "DL", "NT"]
SO_CHANG = len(TRAM) - 1
GIA_CHANG = {"giuong": [220_000, 130_000, 170_000],     # giá minh họa mỗi chặng, đồng/chỗ
             "ghe": [170_000, 100_000, 130_000]}
TEN_XE = {"giuong": "Xe giường nằm 40 chỗ", "ghe": "Xe ghế ngồi 50 chỗ"}
TOI_DA_GHE = 6                                    # một yêu cầu tối đa 6 chỗ


def bo_tri(loai_xe):
    """Sơ đồ xe: danh sách (mã, tầng, hàng, cột) theo thứ tự hiển thị.
    giuong: tầng 0 = dưới (D), 1 = trên (T); cột 0, 1, 2 (dãy trái, giữa, phải); hàng 6 chỉ có cột 0 và 2.
    ghe: tầng 0; 6 cột: hàng 0–10 dùng cột 0, 1 | lối đi | 4, 5; hàng cuối (11) đủ 6 cột."""
    ds = []
    if loai_xe == "giuong":
        for tang, chu in enumerate("DT"):
            n = 0
            for hang in range(7):
                for cot in ((0, 1, 2) if hang < 6 else (0, 2)):
                    n += 1
                    ds.append((f"{chu}{n:02d}", tang, hang, cot))
    else:
        n = 0
        for hang in range(12):
            for cot in ((0, 1, 4, 5) if hang < 11 else range(6)):
                n += 1
                ds.append((f"{n:02d}", 0, hang, cot))
    return ds


def ds_ma_ghe(loai_xe):
    return [x[0] for x in bo_tri(loai_xe)]


def ten_chang(k):
    return f"{TRAM[k]}–{TRAM[k + 1]}"


def ten_doan(tu, den):
    return f"{TRAM[tu]} → {TRAM[den]}"


def _trong(lan_cuoi=None):
    return {"tt": "trong", "nguoi": None, "het_han": None, "ma_ve": None, "nguoi_het_han": lan_cuoi}


def _ten_o(o):
    g, k = o.split("#")
    return f"{g} ({ten_chang(int(k))})"


class SoKiemToanDatVeXe(SoKiemToan):
    TEN_SAI_PHAM = {"trung_cho": "trùng chỗ", "tt_sai": "thanh toán sai"}

    def __init__(self):
        super().__init__()
        self.o = {}                    # theo sổ: ô -> {"tt": "giu"|"ban", "nguoi", "het_han"}
        self.so_ve = 0                 # số vé (ghế × đoạn) đã bán theo sổ

    def _kiem_tra(self, gd):
        nguoi, luc = gd["nguoi"], gd.get("luc")
        if gd["loai"] == "ban":
            self.so_ve += gd.get("so_ve", 0)
        for o in gd["o"]:
            cu = self.o.get(o)
            if gd["loai"] == "giu":
                if cu and cu["tt"] == "ban":
                    self._sai_pham("trung_cho", f"{_ten_o(o)} giữ cho {nguoi} khi đã bán cho {cu['nguoi']}")
                    continue
                if cu and cu["nguoi"] != nguoi and luc < cu["het_han"]:
                    self._sai_pham("trung_cho", f"{_ten_o(o)} giữ cho {nguoi} khi {cu['nguoi']} đang giữ")
                self.o[o] = {"tt": "giu", "nguoi": nguoi, "het_han": gd["het_han"]}
            elif gd["loai"] == "ban":
                if cu and cu["tt"] == "ban":
                    self._sai_pham("trung_cho", f"{_ten_o(o)} bán cho {nguoi} khi đã bán cho {cu['nguoi']}")
                    continue
                if not cu or cu["nguoi"] != nguoi:
                    self._sai_pham("tt_sai", f"{_ten_o(o)}: {nguoi} trả tiền chỗ không do mình giữ")
                elif luc >= cu["het_han"]:
                    self._sai_pham("tt_sai", f"{_ten_o(o)}: {nguoi} trả tiền khi đã hết hạn giữ")
                self.o[o] = {"tt": "ban", "nguoi": nguoi, "het_han": None}
            elif cu and cu["tt"] == "giu" and cu["nguoi"] == nguoi:       # nhả / hết hạn
                del self.o[o]


class MayChuDatVeXe(MayChuCoSo):
    TEN = "Đặt vé xe khách MR6.14"
    LOAI_YEU_CAU = ("giu_cho", "thanh_toan", "nha")
    THAM_SO_MAC_DINH = {"ttl_s": 30.0, "loai_xe": "giuong", "ghe_nong": "D01", "chuyen": "07:30",
                        "nha_xe": "Xe khách Cao Nguyên"}

    # ------------------------------------------------------------ tạo / đặt lại
    def _kiem_tham_so(self, ts):
        super()._kiem_tham_so(ts)
        if not 1 <= ts.get("ttl_s", 30) <= 600:
            raise ValueError("thời hạn giữ chỗ 1–600 s")
        loai = ts.get("loai_xe", self.tham_so["loai_xe"])
        if loai not in GIA_CHANG:
            raise ValueError("loại xe: giuong hoặc ghe")
        if ts.get("ghe_nong", "") not in set(ds_ma_ghe(loai)) | {""}:
            raise ValueError(f"ghế nóng phải là mã chỗ của {TEN_XE[loai].lower()} hoặc rỗng")
        if "nha_xe" in ts and not 1 <= len(str(ts["nha_xe"])) <= 40:
            raise ValueError("tên nhà xe 1–40 ký tự")

    def ghe_nong(self):
        """Ghế nóng đang dùng: tham số ghe_nong nếu là chỗ của loại xe hiện tại, không thì chỗ đầu (D01 / 01) –
        đổi loại xe mà không đặt ghe_nong thì ghế nóng tự về chỗ đầu; rỗng = không có ghế nóng."""
        g = self.tham_so["ghe_nong"]
        return g if not g or g in self.ghe_hop_le else self.ds_ghe[0]

    def _khoi_tao(self):
        self.bo_tri = bo_tri(self.tham_so["loai_xe"])
        self.ds_ghe = [x[0] for x in self.bo_tri]
        self.ghe_hop_le = set(self.ds_ghe)
        self.tat_ca_o = [f"{g}#{k}" for g in self.ds_ghe for k in range(SO_CHANG)]
        for o in self.tat_ca_o:
            self.kho.dat_o(o, _trong())
        self.so = SoKiemToanDatVeXe()
        self._dang_don = set()                    # ô quá hạn đang được luồng nền dọn
        self.nguoi_that = set()                   # tên người dùng thật (in nhật ký cho họ)
        if not hasattr(self, "_dem_ghi"):          # số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
            self._dem_ghi, self.phien = itertools.count(1), 0

    # ------------------------------------------------------------ kiểm tra yêu cầu
    def _doc_yeu_cau(self, tin):
        """(ghế, tu, den) hợp lệ, hoặc chuỗi lý do."""
        ghe, tu, den = tin.get("ghe"), tin.get("tu"), tin.get("den")
        if not isinstance(ghe, list) or not 1 <= len(ghe) <= TOI_DA_GHE or len(set(map(str, ghe))) != len(ghe) \
                or not all(isinstance(g, str) and g in self.ghe_hop_le for g in ghe):
            return f"ghế: danh sách 1–{TOI_DA_GHE} mã chỗ của {TEN_XE[self.tham_so['loai_xe']].lower()}"
        if type(tu) is not int or type(den) is not int or not 0 <= tu < den <= SO_CHANG:
            return f"đoạn đường: 0 ≤ tu < den ≤ {SO_CHANG} (chỉ số trạm)"
        return ghe, tu, den

    # ------------------------------------------------------------ giữ chỗ / thanh toán / nhả
    def _xu_ly_yeu_cau(self, khach, nguoi, tin, tra_loi):
        doc_yc = self._doc_yeu_cau(tin)
        if isinstance(doc_yc, str):
            tra_loi({"ok": False, "ma_loi": "sai_du_lieu", "ly_do": doc_yc})
            return
        ghe, tu, den = doc_yc
        o_yc = [f"{g}#{k}" for g in ghe for k in range(tu, den)]
        that = khach.so_nguoi is None
        if that:
            self.nguoi_that.add(nguoi)
        ts = self.tham_so
        ttl, gia = ts["ttl_s"], sum(GIA_CHANG[ts["loai_xe"]][tu:den]) * len(ghe)
        chung = {"ghe": ghe, "tu": tu, "den": den, "nguoi": nguoi}

        def tra(kq):                                     # bỏ các trường nội bộ ("_…") trước khi gửi
            tra_loi({k: v for k, v in kq.items() if not k.startswith("_")})

        if tin["loai"] == "giu_cho":
            def xu_ly(doc):                              # hàm THUẦN: mọi ô phải trống / quá hạn / của mình
                luc = time.monotonic()
                moi, con_lai = [], ttl
                for o in o_yc:
                    v, g, k = doc[o], o.split("#")[0], int(o.split("#")[1])
                    if v["tt"] == "ban":
                        return {"ok": False, "ma_loi": "da_ban", "ly_do": f"chỗ {g} chặng {ten_chang(k)} đã bán",
                                "ghe_ban": [g], "tu": tu, "den": den}, None
                    if v["tt"] == "giu" and luc < v["het_han"]:
                        if v["nguoi"] != nguoi:
                            return {"ok": False, "ma_loi": "dang_giu",
                                    "ly_do": f"chỗ {g} chặng {ten_chang(k)} đang có người giữ", "ghe_ban": [g],
                                    "tu": tu, "den": den}, None
                        con_lai = min(con_lai, v["het_han"] - luc)
                    else:
                        moi.append(o)
                het_han = luc + ttl
                ghi = {o: {"tt": "giu", "nguoi": nguoi, "het_han": het_han, "ma_ve": None, "nguoi_het_han": None}
                       for o in moi}
                return ({"ok": True, **chung, "het_han_ms": round(con_lai * 1000), "tien": gia,
                         "_moi": moi, "_luc": luc, "_het_han": het_han}, ghi or None)

            self.kho.thuc_hien(o_yc, xu_ly, tra, khi_ghi=lambda kq: self._da_giu(kq, that))
        elif tin["loai"] == "thanh_toan":
            def xu_ly(doc):
                luc = time.monotonic()
                het_han = khong_giu = None              # ưu tiên báo "hết hạn" nếu có dấu vết lượt giữ của mình
                for o in o_yc:
                    v, g = doc[o], o.split("#")[0]
                    if v["tt"] == "giu" and v["nguoi"] == nguoi and luc < v["het_han"]:
                        continue
                    if (v["tt"] == "giu" and v["nguoi"] == nguoi) or (v["tt"] == "trong" and v["nguoi_het_han"] == nguoi):
                        het_han = het_han or g
                    else:
                        khong_giu = khong_giu or g
                if het_han:
                    return {"ok": False, "ma_loi": "het_han", "ly_do": f"giữ chỗ {het_han} đã hết hạn"}, None
                if khong_giu:
                    return {"ok": False, "ma_loi": "khong_giu", "ly_do": f"chỗ {khong_giu} không do bạn giữ"}, None
                ma = f"XK{ts['chuyen'].replace(':', '')}"
                ve = [f"{ma}-{g}-{MA_TRAM[tu]}{MA_TRAM[den]}" for g in ghe]
                ghi = {o: {"tt": "ban", "nguoi": nguoi, "het_han": None, "ma_ve": ve[ghe.index(o.split('#')[0])],
                           "nguoi_het_han": None} for o in o_yc}
                return {"ok": True, **chung, "ma_ve": ve, "tien": gia, "_luc": luc}, ghi

            self.kho.thuc_hien(o_yc, xu_ly, tra, khi_ghi=lambda kq: self._da_ban(kq, that))
        else:
            def xu_ly(doc):
                nha = [o for o in o_yc if doc[o]["tt"] == "giu" and doc[o]["nguoi"] == nguoi]
                if not nha:
                    return {"ok": False, "ma_loi": "khong_giu", "ly_do": "bạn không giữ chỗ nào trong số này"}, None
                return {"ok": True, **chung, "_o": nha}, {o: _trong() for o in nha}

            self.kho.thuc_hien(o_yc, xu_ly, tra, khi_ghi=lambda kq: self._da_nha(kq, that))

    # khi_ghi: gọi trong vùng găng ngay sau khi ghi – ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ
    # tự lần ghi; cap_nhat mang "phien" lúc chụp -> client biết cap_nhat chụp trước hay sau lần ghi của mình.
    def _danh_so(self, kq):
        self.phien = kq["phien"] = next(self._dem_ghi)

    def _da_giu(self, kq, that):
        self._danh_so(kq)
        self.so.ghi(loai="giu", o=kq["_moi"], nguoi=kq["nguoi"], luc=kq["_luc"], het_han=kq["_het_han"])
        if that:
            self.thong_bao(f"{kq['nguoi']} giữ {', '.join(kq['ghe'])} ({ten_doan(kq['tu'], kq['den'])},"
                           f" {self.tham_so['ttl_s']:g} s)")
        self.bao_doi()

    def _da_ban(self, kq, that):
        self._danh_so(kq)
        o = [f"{g}#{k}" for g in kq["ghe"] for k in range(kq["tu"], kq["den"])]
        self.so.ghi(loai="ban", o=o, nguoi=kq["nguoi"], luc=kq["_luc"], so_ve=len(kq["ghe"]))
        if that:
            self.thong_bao(f"{kq['nguoi']} mua {len(kq['ghe'])} vé {', '.join(kq['ghe'])}"
                           f" {ten_doan(kq['tu'], kq['den'])} ({tien(kq['tien'])})")
        self.bao_doi()

    def _da_nha(self, kq, that):
        self._danh_so(kq)
        self.so.ghi(loai="nha", o=kq["_o"], nguoi=kq["nguoi"])
        if that:
            self.thong_bao(f"{kq['nguoi']} nhả {', '.join(kq['ghe'])} ({ten_doan(kq['tu'], kq['den'])})")
        self.bao_doi()

    # ------------------------------------------------------------ dọn ô quá hạn (luồng nền, 100 ms)
    def _dinh_ky(self, bay_gio):
        for o in self.tat_ca_o:
            v = self.kho.doc(o)
            if v["tt"] != "giu" or bay_gio < v["het_han"] or o in self._dang_don:
                continue
            self._dang_don.add(o)

            def xu_ly(doc, o=o):                         # nhả ô quá hạn cũng là một thao tác ghi
                v = doc[o]
                if v["tt"] != "giu" or time.monotonic() < v["het_han"]:
                    return {"ok": False}, None           # vừa được thanh toán / giữ lại
                return {"ok": True, "o": [o], "nguoi": v["nguoi"]}, {o: _trong(v["nguoi"])}

            self.kho.thuc_hien([o], xu_ly, lambda kq, o=o: self._dang_don.discard(o), khi_ghi=self._da_het_han)

    def _da_het_han(self, kq):
        self._danh_so(kq)
        self.so.ghi(loai="het_han", o=kq["o"], nguoi=kq["nguoi"])
        if kq["nguoi"] in self.nguoi_that:
            self.thong_bao(f"Hết hạn giữ {_ten_o(kq['o'][0])} ({kq['nguoi']}), chỗ trống lại")
        self.bao_doi()

    # ------------------------------------------------------------ trạng thái gửi client
    def _trang_thai(self):
        """so_do[i] = chuỗi SO_CHANG ký tự cho chỗ ds_ghe[i]: '.' trống, 'g' đang giữ, 'b' đã bán (từng chặng);
        chu / con_lai_ms theo tên ô "D01#1"."""
        phien = self.phien                              # đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
        bay_gio = time.monotonic()
        so_do, chu, con_lai = [], {}, {}
        dem = {"trong": 0, "giu": 0, "ban": 0}
        for g in self.ds_ghe:
            dong = []
            for k in range(SO_CHANG):
                o = f"{g}#{k}"
                v = self.kho.doc(o)
                if v["tt"] == "giu" and bay_gio < v["het_han"]:
                    dong.append("g")
                    chu[o] = v["nguoi"]
                    con_lai[o] = round((v["het_han"] - bay_gio) * 1000)
                    dem["giu"] += 1
                elif v["tt"] == "ban":
                    dong.append("b")
                    chu[o] = v["nguoi"]
                    dem["ban"] += 1
                else:
                    dong.append(".")
                    dem["trong"] += 1
            so_do.append("".join(dong))
        ts = self.tham_so
        return {"phien": phien, "nha_xe": ts["nha_xe"], "chuyen": ts["chuyen"], "loai_xe": ts["loai_xe"],
                "ten_xe": TEN_XE[ts["loai_xe"]], "tram": TRAM, "gia_chang": GIA_CHANG[ts["loai_xe"]],
                "ttl_s": ts["ttl_s"], "ghe_nong": self.ghe_nong(), "ghe": self.ds_ghe,
                "bo_tri": [[t, h, c] for _, t, h, c in self.bo_tri], "so_do": so_do, "chu": chu,
                "con_lai_ms": con_lai, "dem": dem}

    def _tom_tat(self):
        ban = giu = 0
        bay_gio = time.monotonic()
        for o in self.tat_ca_o:
            v = self.kho.doc(o)
            ban += v["tt"] == "ban"
            giu += v["tt"] == "giu" and bay_gio < v["het_han"]
        return f"Bán {ban}/{len(self.tat_ca_o)} chỗ-chặng, giữ {giu}, sổ {self.so.so_ve} vé"


_khoa_in = threading.Lock()


def in_dong(tien_to, text):
    """Nhiều luồng cùng in: mỗi dòng in trọn vẹn."""
    with _khoa_in:
        print(f"[{tien_to}] {text}", flush=True)
