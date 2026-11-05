"""Bài mở rộng MR6.10 – Lõi máy chủ đăng ký học phần: nghiệp vụ + sổ kiểm toán (khung ở chung.py).

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
Cổng đăng ký học phần ngày mở đăng ký: vài lớp học phần (mặc định 3 lớp, sĩ số 50), hàng nghìn sinh viên
cùng bấm "Đăng ký". Hai thao tác ghi:
    {"loai":"dang_ky","ma_yc":k,"ma_lop":"123013-01"[,"nguoi":"u17"]}
    {"loai":"huy","ma_yc":k,"ma_lop":"123013-01"[,"nguoi":"u17"]}
Luật: lớp còn chỗ (số đã đăng ký < sĩ số) mới nhận; sinh viên đã có trong lớp thì không đăng ký lại
("da_dang_ky"); chỉ hủy được lớp mình đã đăng ký ("chua_dang_ky").

Mỗi lớp là MỘT ô của KhoTranhChap: {"sv": frozenset(tên sinh viên)}; ghi = tạo frozenset mới (xu_ly
thuần). Khóa theo tài nguyên: hai lớp khác nhau được xử lý song song; một lớp "hot" là điểm nóng.

Sổ kiểm toán (danh sách lớp theo sổ, ghi trong vùng găng) đếm hai sai phạm:
  vuot_si_so – một lượt đăng ký được chấp nhận khi lớp (theo sổ) đã đủ sĩ số;
  dk_trung   – một sinh viên được chấp nhận đăng ký một lớp mà sổ ghi là đã có tên (hai thiết bị cùng
               bấm, hoặc bấm đúp, cùng đọc "chưa có tên").
"""
import collections
import threading

from chung import MayChuCoSo, SoKiemToan

# mã lớp, tên học phần, giảng viên, lịch học, số tín chỉ
DANH_MUC = [("123013-01", "Lập trình mạng", "TS. Lê Hoàng Nam", "T2, tiết 1–3, E301", 3),
            ("123005-01", "Quản trị mạng", "ThS. Phạm Thu Hà", "T3, tiết 4–6, E302", 3),
            ("006124-01", "Tin học cơ bản", "ThS. Vũ Minh Đức", "T4, tiết 7–9, E303", 2),
            ("123013-02", "Lập trình mạng", "TS. Lê Hoàng Nam", "T5, tiết 1–3, E301", 3),
            ("123005-02", "Quản trị mạng", "ThS. Phạm Thu Hà", "T6, tiết 4–6, E302", 3),
            ("006124-02", "Tin học cơ bản", "ThS. Vũ Minh Đức", "T7, tiết 7–9, E303", 2)]


class SoKiemToanDangKy(SoKiemToan):
    TEN_SAI_PHAM = {"vuot_si_so": "vượt sĩ số", "dk_trung": "ĐK trùng"}

    def __init__(self, si_so):
        super().__init__()
        self.si_so = si_so
        self.lop = collections.defaultdict(set)        # mã lớp -> sinh viên theo sổ

    def _kiem_tra(self, gd):
        ds = self.lop[gd["ma_lop"]]
        if gd["thao_tac"] == "huy":
            ds.discard(gd["nguoi"])
            return
        if gd["nguoi"] in ds:
            self._sai_pham("dk_trung", f"{gd['nguoi']} ĐK lại {gd['ma_lop']}")
        elif len(ds) >= self.si_so:
            self._sai_pham("vuot_si_so", f"{gd['ma_lop']}: {gd['nguoi']} là SV thứ {len(ds) + 1}")
        ds.add(gd["nguoi"])

    def tong_so(self):
        with self.khoa:
            return sum(len(x) for x in self.lop.values())

    def so_lop(self, ma):
        with self.khoa:
            return len(self.lop.get(ma, ()))


class MayChuDangKy(MayChuCoSo):
    TEN = "Đăng ký học phần MR6.10"
    LOAI_YEU_CAU = ("dang_ky", "huy")
    THAM_SO_MAC_DINH = {"so_lop": 3, "si_so": 50}

    # ------------------------------------------------------------ tạo / đặt lại đợt đăng ký
    def _kiem_tham_so(self, ts):
        super()._kiem_tham_so(ts)
        if not 1 <= ts.get("so_lop", 1) <= len(DANH_MUC):
            raise ValueError(f"số lớp 1–{len(DANH_MUC)}")
        if not 1 <= ts.get("si_so", 1) <= 500:
            raise ValueError("sĩ số 1–500")

    def _khoi_tao(self):
        self.lop = DANH_MUC[:self.tham_so["so_lop"]]
        for ma, *_ in self.lop:
            self.kho.dat_o("lop:" + ma, {"sv": frozenset()})
        self.so = SoKiemToanDangKy(self.tham_so["si_so"])

    # ------------------------------------------------------------ đăng ký / hủy
    def _xu_ly_yeu_cau(self, khach, nguoi, tin, tra_loi):
        ma = tin.get("ma_lop")
        if not any(ma == x[0] for x in self.lop):
            tra_loi({"ok": False, "ma_loi": "sai_du_lieu", "ly_do": f"không có lớp {ma}"})
            return
        k, si_so, thao_tac = "lop:" + ma, self.tham_so["si_so"], tin["loai"]

        def xu_ly(doc):                                    # hàm THUẦN: đọc doc, trả về (kq, ghi)
            sv = doc[k]["sv"]
            if thao_tac == "dang_ky":
                if nguoi in sv:
                    return {"ok": False, "ma_loi": "da_dang_ky", "ly_do": "đã đăng ký lớp này",
                            "ma_lop": ma, "da_dk": len(sv)}, None
                if len(sv) >= si_so:
                    return {"ok": False, "ma_loi": "lop_day", "ly_do": "lớp đã đủ", "ma_lop": ma,
                            "da_dk": len(sv)}, None
                moi = sv | {nguoi}
            else:
                if nguoi not in sv:
                    return {"ok": False, "ma_loi": "chua_dang_ky", "ly_do": "chưa đăng ký lớp này",
                            "ma_lop": ma, "da_dk": len(sv)}, None
                moi = sv - {nguoi}
            return ({"ok": True, "thao_tac": thao_tac, "ma_lop": ma, "nguoi": nguoi, "da_dk": len(moi)},
                    {k: {"sv": moi}})

        gop = khach.so_nguoi is not None
        self.kho.thuc_hien([k], xu_ly, tra_loi, khi_ghi=lambda kq: self._da_ghi(kq, gop))

    def _da_ghi(self, kq, gop):
        """Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi."""
        self.so.ghi(thao_tac=kq["thao_tac"], nguoi=kq["nguoi"], ma_lop=kq["ma_lop"])
        if not gop:                                        # người dùng thật: báo lên nhật ký
            viec = "đăng ký" if kq["thao_tac"] == "dang_ky" else "hủy"
            self.thong_bao(f"{kq['nguoi']} {viec} {kq['ma_lop']} ({kq['da_dk']}/{self.tham_so['si_so']})")
        self.bao_doi()

    # ------------------------------------------------------------ trạng thái gửi client
    def _trang_thai(self):
        si_so = self.tham_so["si_so"]
        ds = []
        for ma, ten, gv, lich, tc in self.lop:
            sv = self.kho.doc("lop:" + ma)["sv"]
            ds.append({"ma": ma, "ten": ten, "gv": gv, "lich": lich, "tc": tc, "si_so": si_so,
                       "da_dk": len(sv), "theo_so": self.so.so_lop(ma), "sv": sorted(sv)})
        return {"si_so": si_so, "lop": ds}

    def _tom_tat(self):
        cho = self.tham_so["so_lop"] * self.tham_so["si_so"]
        return f"Đã ĐK {self.so.tong_so()}/{cho} chỗ ({self.tham_so['so_lop']} lớp × {self.tham_so['si_so']})"


_khoa_in = threading.Lock()


def in_dong(tien_to, text):
    """Nhiều luồng cùng in: mỗi dòng in trọn vẹn."""
    with _khoa_in:
        print(f"[{tien_to}] {text}", flush=True)
