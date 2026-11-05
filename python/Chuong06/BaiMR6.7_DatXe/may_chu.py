"""Bài mở rộng MR6.7 – Lõi máy chủ đặt xe: nghiệp vụ + sổ kiểm toán (khung ở chung.py).

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
m chiếc xe (mặc định 1), mỗi xe MỘT chỗ; n người cùng bấm "Đặt xe". Hai thao tác ghi:
    {"loai":"dat_xe","ma_yc":k[,"don":[x,y],"den":[x,y]][,"nguoi":"u17"]}
    {"loai":"huy","ma_yc":k[,"nguoi":"u17"]}
Luật: mỗi người chỉ giữ một chuyến; máy chủ giao xe TRỐNG gần điểm đón nhất; hết xe trống -> "hết chỗ".
Hủy chuyến trả xe về trạng thái trống.

Bản đồ là lưới 5 × 5 đường (tọa độ 0–1, 1 đơn vị = 5 km); điểm đón/đến là ngã tư. Mỗi xe là MỘT ô
"xe1", "xe2"… của KhoTranhChap: {"ma","tai_xe","bien_so","mau","vi_tri","khach","don","den","gio",
"cuoc","phut"}. Vì phải biết người đặt đã có chuyến chưa và xe nào trống gần nhất, đặt xe đọc–ghi CẢ m
ô trong một thao tác (khoa: giữ khóa mọi xe theo thứ tự tên ô; lac_quan: so sánh phiên bản mọi xe).

Sổ kiểm toán đếm một sai phạm:
  trung_cho – một lần đặt được chấp nhận cho một xe mà (theo sổ) đang chở người khác: hai người cùng
              đọc "xe trống", cùng được báo "Đặt thành công – tài xế Hùng", người ghi sau đè người ghi
              trước, người kia đứng đường chờ một chiếc xe không đến.
"""
import collections
import itertools
import threading
import time

from chung import MayChuCoSo, SoKiemToan

DUONG = (0.1, 0.3, 0.5, 0.7, 0.9)               # tọa độ 5 đường dọc / 5 đường ngang
TEN_DOC = ("Lê Lợi", "Hai Bà Trưng", "Nguyễn Huệ", "Pasteur", "Đồng Khởi")
TEN_NGANG = ("Lý Tự Trọng", "Lê Thánh Tôn", "Nguyễn Du", "Võ Văn Tần", "Điện Biên Phủ")
NGA_TU = [(x, y) for y in DUONG for x in DUONG]
KM_MOI_DON_VI = 5.0
TAI_XE = [("Hùng", "51A-123.45", "trắng"), ("Lan", "51G-678.90", "đỏ"), ("Minh", "51F-246.80", "xanh"),
          ("Tuấn", "51H-135.79", "bạc"), ("Hoa", "51K-864.20", "vàng"), ("Phúc", "51A-975.31", "đen"),
          ("Quang", "51G-112.23", "cam"), ("Thảo", "51F-445.56", "tím"), ("Sơn", "51H-778.89", "xám"),
          ("Vy", "51K-990.01", "hồng")]
VI_TRI_XE = [(0.9, 0.1), (0.1, 0.9), (0.5, 0.3), (0.3, 0.7), (0.7, 0.5), (0.1, 0.3), (0.9, 0.7),
             (0.5, 0.9), (0.3, 0.1), (0.7, 0.9)]


def ten_diem(p):
    """(0.1, 0.5) -> "Lê Lợi × Nguyễn Du"."""
    return f"{TEN_DOC[DUONG.index(p[0])]} × {TEN_NGANG[DUONG.index(p[1])]}"


def km(a, b):
    """Đường đi trên lưới phố (Manhattan) giữa hai điểm, km."""
    return (abs(a[0] - b[0]) + abs(a[1] - b[1])) * KM_MOI_DON_VI


def cuoc(a, b):
    """Giá cước: 12.000 đ mở cửa + 10.000 đ/km, làm tròn tới 1.000 đ."""
    return int(round((12_000 + 10_000 * km(a, b)) / 1000)) * 1000


def phut_den(xe, don):
    """Thời gian xe tới điểm đón (30 km/h = 0,5 km/phút), tối thiểu 1 phút."""
    return max(1, round(km(xe, don) / 0.5))


def _diem(x):
    """Kiểm tra [x, y] là một ngã tư trên bản đồ; trả về tuple hoặc None."""
    if (isinstance(x, (list, tuple)) and len(x) == 2
            and all(isinstance(v, (int, float)) and not isinstance(v, bool) for v in x)):
        p = (round(float(x[0]), 2), round(float(x[1]), 2))
        if p in NGA_TU:
            return p
    return None


class SoKiemToanDatXe(SoKiemToan):
    TEN_SAI_PHAM = {"trung_cho": "trùng chỗ"}

    def __init__(self, so_xe):
        super().__init__()
        self.tren_xe = {i: [] for i in range(1, so_xe + 1)}   # theo sổ: ai đang "có" chỗ trên xe i
        self.so_dat = 0                                        # số lần đặt được chấp nhận

    def _kiem_tra(self, gd):
        ds = self.tren_xe[gd["xe"]]
        if gd["loai"] == "dat":
            self.so_dat += 1
            if ds:
                self._sai_pham("trung_cho", f"xe {gd['xe']}: {gd['nguoi']} khi {ds[-1]} đang đặt")
            ds.append(gd["nguoi"])
        elif gd["nguoi"] in ds:                                # hủy
            ds.remove(gd["nguoi"])


class MayChuDatXe(MayChuCoSo):
    TEN = "Đặt xe MR6.7"
    LOAI_YEU_CAU = ("dat_xe", "huy")
    THAM_SO_MAC_DINH = {"so_xe": 1}

    # ------------------------------------------------------------ tạo / đặt lại
    def _kiem_tham_so(self, ts):
        super()._kiem_tham_so(ts)
        if not 1 <= ts.get("so_xe", 1) <= 50:
            raise ValueError("số xe 1–50")

    def _khoi_tao(self):
        m = self.tham_so["so_xe"]
        self.cac_o = [f"xe{i}" for i in range(1, m + 1)]
        for i in range(m):
            ten, bien, mau = TAI_XE[i % len(TAI_XE)]
            if i >= len(TAI_XE):
                ten = f"{ten} {i // len(TAI_XE) + 1}"
            vi_tri = VI_TRI_XE[i] if i < len(VI_TRI_XE) else NGA_TU[(i * 7) % len(NGA_TU)]
            self.kho.dat_o(f"xe{i + 1}", {"ma": i + 1, "tai_xe": ten, "bien_so": bien, "mau": mau,
                                          "vi_tri": list(vi_tri), "khach": None, "don": None, "den": None,
                                          "gio": None, "cuoc": None, "phut": None})
        self.so = SoKiemToanDatXe(m)
        self.lich_su = collections.deque(maxlen=30)
        if not hasattr(self, "_dem_ghi"):          # số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
            self._dem_ghi, self.phien = itertools.count(1), 0

    # ------------------------------------------------------------ đặt xe / hủy
    def _xu_ly_yeu_cau(self, khach, nguoi, tin, tra_loi):
        that = khach.so_nguoi is None                   # người dùng thật (không phải bộ mô phỏng): ghi nhật ký
        if tin["loai"] == "dat_xe":
            don, den = tin.get("don"), tin.get("den")
            don = (0.5, 0.5) if don is None else _diem(don)
            den = (0.9, 0.9) if den is None else _diem(den)
            if don is None or den is None:
                tra_loi({"ok": False, "ma_loi": "sai_du_lieu", "ly_do": "điểm đón/đến phải là ngã tư [x, y]"})
                return

            def xu_ly(doc):                             # hàm THUẦN: đọc mọi xe, chọn xe trống gần nhất
                for xe in doc.values():
                    if xe["khach"] == nguoi:
                        return {"ok": False, "ma_loi": "da_dat", "ly_do": f"bạn đã đặt xe {xe['tai_xe']}"}, None
                trong = [xe for xe in doc.values() if xe["khach"] is None]
                if not trong:
                    return {"ok": False, "ma_loi": "het_cho", "ly_do": "hết chỗ – không còn xe trống"}, None
                xe = min(trong, key=lambda x: (km(x["vi_tri"], don), x["ma"]))
                moi = dict(xe, khach=nguoi, don=list(don), den=list(den), gio=time.strftime("%H:%M:%S"),
                           cuoc=cuoc(don, den), phut=phut_den(xe["vi_tri"], don))
                kq = {"ok": True, "xe": xe["ma"], "khach": nguoi, "tai_xe": xe["tai_xe"],
                      "bien_so": xe["bien_so"], "mau": xe["mau"], "phut": moi["phut"], "cuoc": moi["cuoc"]}
                return kq, {f"xe{xe['ma']}": moi}

            self.kho.thuc_hien(self.cac_o, xu_ly, tra_loi, khi_ghi=lambda kq: self._da_ghi("dat", kq, that))
        else:
            def xu_ly(doc):
                for xe in doc.values():
                    if xe["khach"] == nguoi:
                        moi = dict(xe, khach=None, don=None, den=None, gio=None, cuoc=None, phut=None)
                        return ({"ok": True, "xe": xe["ma"], "khach": nguoi, "tai_xe": xe["tai_xe"]},
                                {f"xe{xe['ma']}": moi})
                return {"ok": False, "ma_loi": "chua_dat", "ly_do": "bạn chưa đặt xe"}, None

            self.kho.thuc_hien(self.cac_o, xu_ly, tra_loi, khi_ghi=lambda kq: self._da_ghi("huy", kq, that))

    def _da_ghi(self, loai, kq, that):
        """Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ tự
        lần ghi; cap_nhat mang "phien" lúc chụp -> client biết một cap_nhat chụp trước hay sau lần ghi của mình."""
        self.phien = kq["phien"] = next(self._dem_ghi)
        self.so.ghi(loai=loai, xe=kq["xe"], nguoi=kq["khach"])
        self.lich_su.appendleft({"gio": time.strftime("%H:%M:%S"), "loai": loai, "xe": kq["xe"],
                                 "tai_xe": kq["tai_xe"], "nguoi": kq["khach"]})
        if that:
            if loai == "dat":
                self.thong_bao(f"Xe {kq['xe']} ({kq['tai_xe']}) nhận khách {kq['khach']}")
            else:
                self.thong_bao(f"{kq['khach']} hủy chuyến, xe {kq['xe']} trống")
        self.bao_doi()

    # ------------------------------------------------------------ trạng thái gửi client
    def _trang_thai(self):
        phien = self.phien                              # đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
        ds = [self.kho.doc(k) for k in self.cac_o]
        return {"phien": phien, "so_xe": len(ds), "trong": sum(1 for x in ds if x["khach"] is None), "xe": ds,
                "lich_su": list(self.lich_su)[:8]}

    def _tom_tat(self):
        ds = [self.kho.doc(k) for k in self.cac_o]
        da = sum(1 for x in ds if x["khach"] is not None)
        return f"Đã đặt {da}/{len(ds)} xe, {self.so.so_dat} lượt báo đặt được"


_khoa_in = threading.Lock()


def in_dong(tien_to, text):
    """Nhiều luồng cùng in: mỗi dòng in trọn vẹn."""
    with _khoa_in:
        print(f"[{tien_to}] {text}", flush=True)

