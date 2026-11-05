"""Bài mở rộng MR6.8 – Lõi máy chủ đặt vé xem phim: nghiệp vụ + sổ kiểm toán (khung ở chung.py).

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
Phòng chiếu 8 hàng (A–H) × 12 ghế; n người cùng chọn ghế. Ba thao tác ghi:
    {"loai":"giu_cho","ma_yc":k,"ghe":["E7","E8"][,"nguoi":"u17"]}    giữ chỗ có thời hạn (TTL)
    {"loai":"thanh_toan","ma_yc":k,"ghe":["E7","E8"]}                  ghế mình đang giữ, còn hạn -> vé
    {"loai":"nha","ma_yc":k,"ghe":["E7"]}                              trả lại ghế đang giữ
Luật: giữ được khi MỌI ghế trong yêu cầu đều trống (hoặc lượt giữ cũ đã hết hạn, hoặc đã là của mình);
lượt giữ hết hạn sau ttl_s giây (mặc định 30 s) – luồng nền của máy chủ (100 ms/lần) "dọn" ghế quá hạn,
và một lượt giữ quá hạn cũng bị coi là trống ngay cả khi chưa kịp dọn; thanh toán chỉ nhận ghế mình
đang giữ và còn hạn (quá hạn -> "het_han"). Mỗi ghế là MỘT ô của KhoTranhChap:
    {"tt":"trong"|"giu"|"ban","nguoi","het_han"(đồng hồ đơn điệu),"ma_ve","nguoi_het_han"}
nên khóa theo từng ghế: giữ nhiều ghế = khóa nhiều ô theo thứ tự tên ô (không khóa chết).

Sổ kiểm toán đếm hai sai phạm:
  trung_ghe – một ghế được giữ/bán cho người thứ hai khi đang được người khác giữ (còn hạn) hoặc đã bán;
  tt_sai    – thanh toán được chấp nhận cho ghế không do mình giữ hoặc lượt giữ đã hết hạn.
"""
import itertools
import threading
import time

from chung import MayChuCoSo, SoKiemToan, tien

HANG = "ABCDEFGH"
COT = 12
TAT_CA_GHE = [f"{r}{c}" for r in HANG for c in range(1, COT + 1)]
GHE_HOP_LE = set(TAT_CA_GHE)
TOI_DA_GHE = 8                                    # một yêu cầu tối đa 8 ghế


def ds_ghe(x):
    """Kiểm tra danh sách ghế: 1–8 mã ghế hợp lệ, không trùng; trả về list hoặc None."""
    if not isinstance(x, list) or not 1 <= len(x) <= TOI_DA_GHE or len(set(map(str, x))) != len(x):
        return None
    return list(x) if all(isinstance(g, str) and g in GHE_HOP_LE for g in x) else None


def _trong(lan_cuoi=None):
    return {"tt": "trong", "nguoi": None, "het_han": None, "ma_ve": None, "nguoi_het_han": lan_cuoi}


class SoKiemToanDatVe(SoKiemToan):
    TEN_SAI_PHAM = {"trung_ghe": "ghế trùng", "tt_sai": "thanh toán sai"}

    def __init__(self):
        super().__init__()
        self.ghe = {}                  # theo sổ: ghế -> {"tt": "giu"|"ban", "nguoi", "het_han"}
        self.so_ve = 0                 # số vé đã bán theo sổ

    def _kiem_tra(self, gd):
        nguoi, luc = gd["nguoi"], gd.get("luc")
        for g in gd["ghe"]:
            cu = self.ghe.get(g)
            if gd["loai"] == "giu":
                if cu and cu["tt"] == "ban":
                    self._sai_pham("trung_ghe", f"{g} giữ cho {nguoi} khi đã bán cho {cu['nguoi']}")
                    continue
                if cu and cu["nguoi"] != nguoi and luc < cu["het_han"]:
                    self._sai_pham("trung_ghe", f"{g} giữ cho {nguoi} khi {cu['nguoi']} đang giữ")
                self.ghe[g] = {"tt": "giu", "nguoi": nguoi, "het_han": gd["het_han"]}
            elif gd["loai"] == "ban":
                self.so_ve += 1
                if cu and cu["tt"] == "ban":
                    self._sai_pham("trung_ghe", f"{g} bán cho {nguoi} khi đã bán cho {cu['nguoi']}")
                    continue
                if not cu or cu["nguoi"] != nguoi:
                    self._sai_pham("tt_sai", f"{g}: {nguoi} trả tiền ghế không do mình giữ")
                elif luc >= cu["het_han"]:
                    self._sai_pham("tt_sai", f"{g}: {nguoi} trả tiền khi đã hết hạn giữ")
                self.ghe[g] = {"tt": "ban", "nguoi": nguoi, "het_han": None}
            elif cu and cu["tt"] == "giu" and cu["nguoi"] == nguoi:       # nhả / hết hạn
                del self.ghe[g]


class MayChuDatVe(MayChuCoSo):
    TEN = "Đặt vé xem phim MR6.8"
    LOAI_YEU_CAU = ("giu_cho", "thanh_toan", "nha")
    THAM_SO_MAC_DINH = {"ttl_s": 30.0, "ghe_nong": "E7", "gia_ve": 90_000, "phim": "Hành tinh xanh",
                        "suat": "19:30"}

    # ------------------------------------------------------------ tạo / đặt lại
    def _kiem_tham_so(self, ts):
        super()._kiem_tham_so(ts)
        if not 1 <= ts.get("ttl_s", 30) <= 600:
            raise ValueError("thời hạn giữ chỗ 1–600 s")
        if ts.get("ghe_nong", "") not in GHE_HOP_LE | {""}:
            raise ValueError("ghế nóng phải là mã ghế A1–H12 hoặc rỗng")
        if ts.get("gia_ve", 1) <= 0:
            raise ValueError("giá vé > 0")
        if "phim" in ts and not 1 <= len(str(ts["phim"])) <= 40:
            raise ValueError("tên phim 1–40 ký tự")

    def _khoi_tao(self):
        for g in TAT_CA_GHE:
            self.kho.dat_o(g, _trong())
        self.so = SoKiemToanDatVe()
        self._dang_don = set()                    # ghế quá hạn đang được luồng nền dọn
        self.nguoi_that = set()                   # tên người dùng thật (in nhật ký cho họ)
        if not hasattr(self, "_dem_ghi"):          # số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
            self._dem_ghi, self.phien = itertools.count(1), 0

    # ------------------------------------------------------------ giữ chỗ / thanh toán / nhả
    def _xu_ly_yeu_cau(self, khach, nguoi, tin, tra_loi):
        ghe = ds_ghe(tin.get("ghe"))
        if ghe is None:
            tra_loi({"ok": False, "ma_loi": "sai_du_lieu", "ly_do": f"ghế: danh sách 1–{TOI_DA_GHE} mã A1–H12"})
            return
        that = khach.so_nguoi is None
        if that:
            self.nguoi_that.add(nguoi)
        ttl, gia, suat = self.tham_so["ttl_s"], self.tham_so["gia_ve"], self.tham_so["suat"]

        def tra(kq):                                     # bỏ các trường nội bộ ("_…") trước khi gửi
            tra_loi({k: v for k, v in kq.items() if not k.startswith("_")})

        if tin["loai"] == "giu_cho":
            def xu_ly(doc):                              # hàm THUẦN: mọi ghế phải trống / quá hạn / của mình
                luc = time.monotonic()
                moi, con_lai = [], ttl
                for g in ghe:
                    v = doc[g]
                    if v["tt"] == "ban":
                        return {"ok": False, "ma_loi": "da_ban", "ly_do": f"ghế {g} đã bán", "ghe_ban": [g]}, None
                    if v["tt"] == "giu" and luc < v["het_han"]:
                        if v["nguoi"] != nguoi:
                            return {"ok": False, "ma_loi": "dang_giu", "ly_do": f"ghế {g} đang có người giữ",
                                    "ghe_ban": [g]}, None
                        con_lai = min(con_lai, v["het_han"] - luc)
                    else:
                        moi.append(g)
                het_han = luc + ttl
                ghi = {g: {"tt": "giu", "nguoi": nguoi, "het_han": het_han, "ma_ve": None, "nguoi_het_han": None}
                       for g in moi}
                return ({"ok": True, "ghe": ghe, "moi": moi, "het_han_ms": round(con_lai * 1000), "nguoi": nguoi,
                         "_luc": luc, "_het_han": het_han}, ghi or None)

            self.kho.thuc_hien(ghe, xu_ly, tra, khi_ghi=lambda kq: self._da_giu(kq, that))
        elif tin["loai"] == "thanh_toan":
            def xu_ly(doc):
                luc = time.monotonic()
                for g in ghe:
                    v = doc[g]
                    if v["tt"] == "giu" and v["nguoi"] == nguoi and luc < v["het_han"]:
                        continue
                    if (v["tt"] == "giu" and v["nguoi"] == nguoi) or (v["tt"] == "trong" and v["nguoi_het_han"] == nguoi):
                        return {"ok": False, "ma_loi": "het_han", "ly_do": f"giữ chỗ {g} đã hết hạn"}, None
                    return {"ok": False, "ma_loi": "khong_giu", "ly_do": f"ghế {g} không do bạn giữ"}, None
                ve = [f"S{suat.replace(':', '')}-{g}" for g in ghe]
                ghi = {g: {"tt": "ban", "nguoi": nguoi, "het_han": None, "ma_ve": v, "nguoi_het_han": None}
                       for g, v in zip(ghe, ve)}
                return {"ok": True, "ghe": ghe, "ma_ve": ve, "tien": gia * len(ghe), "nguoi": nguoi, "_luc": luc}, ghi

            self.kho.thuc_hien(ghe, xu_ly, tra, khi_ghi=lambda kq: self._da_ban(kq, that))
        else:
            def xu_ly(doc):
                nha = [g for g in ghe if doc[g]["tt"] == "giu" and doc[g]["nguoi"] == nguoi]
                if not nha:
                    return {"ok": False, "ma_loi": "khong_giu", "ly_do": "bạn không giữ ghế nào trong số này"}, None
                return {"ok": True, "ghe": nha, "nguoi": nguoi}, {g: _trong() for g in nha}

            self.kho.thuc_hien(ghe, xu_ly, tra, khi_ghi=lambda kq: self._da_nha(kq, that))

    # khi_ghi: gọi trong vùng găng ngay sau khi ghi – ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ
    # tự lần ghi; cap_nhat mang "phien" lúc chụp -> client biết cap_nhat chụp trước hay sau lần ghi của mình.
    def _danh_so(self, kq):
        self.phien = kq["phien"] = next(self._dem_ghi)

    def _da_giu(self, kq, that):
        self._danh_so(kq)
        self.so.ghi(loai="giu", ghe=kq["moi"], nguoi=kq["nguoi"], luc=kq["_luc"], het_han=kq["_het_han"])
        if that:
            self.thong_bao(f"{kq['nguoi']} giữ {', '.join(kq['moi'])} ({self.tham_so['ttl_s']:g} s)")
        self.bao_doi()

    def _da_ban(self, kq, that):
        self._danh_so(kq)
        self.so.ghi(loai="ban", ghe=kq["ghe"], nguoi=kq["nguoi"], luc=kq["_luc"])
        if that:
            self.thong_bao(f"{kq['nguoi']} mua {len(kq['ghe'])} vé {', '.join(kq['ghe'])} ({tien(kq['tien'])})")
        self.bao_doi()

    def _da_nha(self, kq, that):
        self._danh_so(kq)
        self.so.ghi(loai="nha", ghe=kq["ghe"], nguoi=kq["nguoi"])
        if that:
            self.thong_bao(f"{kq['nguoi']} nhả {', '.join(kq['ghe'])}")
        self.bao_doi()

    # ------------------------------------------------------------ dọn ghế quá hạn (luồng nền, 100 ms)
    def _dinh_ky(self, bay_gio):
        for g in TAT_CA_GHE:
            v = self.kho.doc(g)
            if v["tt"] != "giu" or bay_gio < v["het_han"] or g in self._dang_don:
                continue
            self._dang_don.add(g)

            def xu_ly(doc, g=g):                         # nhả ghế quá hạn cũng là một thao tác ghi
                v = doc[g]
                if v["tt"] != "giu" or time.monotonic() < v["het_han"]:
                    return {"ok": False}, None           # vừa được thanh toán / giữ lại
                return {"ok": True, "ghe": [g], "nguoi": v["nguoi"]}, {g: _trong(v["nguoi"])}

            self.kho.thuc_hien([g], xu_ly, lambda kq, g=g: self._dang_don.discard(g), khi_ghi=self._da_het_han)

    def _da_het_han(self, kq):
        self._danh_so(kq)
        self.so.ghi(loai="het_han", ghe=kq["ghe"], nguoi=kq["nguoi"])
        if kq["nguoi"] in self.nguoi_that:
            self.thong_bao(f"Hết hạn giữ {kq['ghe'][0]} ({kq['nguoi']}), ghế trống lại")
        self.bao_doi()

    # ------------------------------------------------------------ trạng thái gửi client
    def _trang_thai(self):
        phien = self.phien                              # đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
        bay_gio = time.monotonic()
        so_do, chu, con_lai = [], {}, {}
        dem = {"trong": 0, "giu": 0, "ban": 0}
        for r in HANG:
            dong = []
            for c in range(1, COT + 1):
                g = f"{r}{c}"
                v = self.kho.doc(g)
                if v["tt"] == "giu" and bay_gio < v["het_han"]:
                    dong.append("g")
                    chu[g] = v["nguoi"]
                    con_lai[g] = round((v["het_han"] - bay_gio) * 1000)
                    dem["giu"] += 1
                elif v["tt"] == "ban":
                    dong.append("b")
                    chu[g] = v["nguoi"]
                    dem["ban"] += 1
                else:
                    dong.append(".")
                    dem["trong"] += 1
            so_do.append("".join(dong))
        ts = self.tham_so
        return {"phien": phien, "phim": ts["phim"], "suat": ts["suat"], "gia_ve": ts["gia_ve"], "ttl_s": ts["ttl_s"],
                "ghe_nong": ts["ghe_nong"], "hang": HANG, "cot": COT, "so_do": so_do, "chu": chu,
                "con_lai_ms": con_lai, "dem": dem}

    def _tom_tat(self):
        ban = giu = 0
        bay_gio = time.monotonic()
        for g in TAT_CA_GHE:
            v = self.kho.doc(g)
            ban += v["tt"] == "ban"
            giu += v["tt"] == "giu" and bay_gio < v["het_han"]
        return f"Đã bán {ban}/{len(TAT_CA_GHE)} ghế, đang giữ {giu}, sổ {self.so.so_ve} vé"


_khoa_in = threading.Lock()


def in_dong(tien_to, text):
    """Nhiều luồng cùng in: mỗi dòng in trọn vẹn."""
    with _khoa_in:
        print(f"[{tien_to}] {text}", flush=True)
