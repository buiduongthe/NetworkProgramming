"""Bài mở rộng MR6.6 – Lõi máy chủ đấu giá: nghiệp vụ + sổ kiểm toán (khung ở chung.py).

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
Một món hàng, n người cùng trả giá. Thao tác ghi duy nhất:
    {"loai":"tra_gia","ma_yc":k,"gia":5300000[,"nguoi":"u17"]}
Luật: giá ≥ giá hiện tại + bước giá; phiên còn hạn. Trả giá khi phiên còn < gia_han_s giây
(mặc định 10 s) thì phiên được gia hạn tới "bây giờ + gia_han_s" ("đóng mềm" – soft close, chống
"bắn tỉa" phút cuối – anti-sniping). Hết giờ: máy chủ đóng phiên, công bố người thắng.

Trạng thái là MỘT ô "mon" của KhoTranhChap: {"gia","nguoi","so_lan","het_han","ket_thuc"}; mọi thay đổi
tạo dict mới (xu_ly thuần) nên cả 4 chế độ dùng chung đúng một hàm nghiệp vụ.

Sổ kiểm toán đếm hai sai phạm:
  gia_thap_hon – một giá được chấp nhận mà thấp hơn (giá cao nhất đã chấp nhận trước + bước giá):
                 hai người cùng đọc giá cũ, cùng "thắng" kiểm tra, người ghi sau đè người ghi trước;
  gia_cuoi_sai – giá/người dẫn đầu cuối cùng khác giá cao nhất đã chấp nhận (giá cao bị ghi đè mất).
"""
import collections
import threading
import time

from chung import MayChuCoSo, SoKiemToan, la_so_nguyen, tien


class SoKiemToanDauGia(SoKiemToan):
    TEN_SAI_PHAM = {"gia_thap_hon": "giá thấp hơn", "gia_cuoi_sai": "giá cuối sai"}

    def __init__(self, buoc_gia):
        super().__init__()
        self.buoc = buoc_gia
        self.cao_nhat = None             # giá cao nhất đã chấp nhận (theo sổ)
        self.nguoi_cao_nhat = None

    def _kiem_tra(self, gd):
        gia = gd["gia"]
        if self.cao_nhat is not None and gia < self.cao_nhat + self.buoc:
            self._sai_pham("gia_thap_hon", f"{gd['nguoi']} {tien(gia)} sau {tien(self.cao_nhat)}")
        if self.cao_nhat is None or gia > self.cao_nhat:
            self.cao_nhat, self.nguoi_cao_nhat = gia, gd["nguoi"]

    def _kiem_tra_cuoi(self, doc):
        m = doc.get("mon")
        if m is None or self.cao_nhat is None:
            return {}
        sai = m["gia"] != self.cao_nhat or m["nguoi"] != self.nguoi_cao_nhat
        return {"gia_cuoi_sai": 1} if sai else {}


class MayChuDauGia(MayChuCoSo):
    TEN = "Đấu giá MR6.6"
    LOAI_YEU_CAU = ("tra_gia",)
    THAM_SO_MAC_DINH = {"ten_mon": "Bình gốm hoa lam", "gia_khoi_diem": 5_000_000, "buoc_gia": 100_000,
                        "thoi_gian_s": 0.0, "gia_han_s": 10.0}   # thoi_gian_s = 0: không giới hạn

    # ------------------------------------------------------------ tạo / đặt lại phiên
    def _kiem_tham_so(self, ts):
        super()._kiem_tham_so(ts)
        if ts.get("buoc_gia", 1) <= 0 or ts.get("gia_khoi_diem", 0) < 0:
            raise ValueError("bước giá > 0, giá khởi điểm ≥ 0")
        if ts.get("thoi_gian_s", 0) < 0 or ts.get("gia_han_s", 0) < 0:
            raise ValueError("thời gian ≥ 0")
        if "ten_mon" in ts and not 1 <= len(str(ts["ten_mon"])) <= 40:
            raise ValueError("tên món 1–40 ký tự")

    def _khoi_tao(self):
        ts = self.tham_so
        het_han = time.monotonic() + ts["thoi_gian_s"] if ts["thoi_gian_s"] > 0 else None
        self.kho.dat_o("mon", {"gia": ts["gia_khoi_diem"], "nguoi": None, "so_lan": 0,
                               "het_han": het_han, "ket_thuc": False})
        self.so = SoKiemToanDauGia(ts["buoc_gia"])
        self.lich_su = collections.deque(maxlen=50)   # giá đã chấp nhận, mới nhất trước (để hiển thị)
        self._dang_dong = False
        self._luc_bao_gia_han = 0.0

    # ------------------------------------------------------------ trả giá
    def _xu_ly_yeu_cau(self, khach, nguoi, tin, tra_loi):
        gia = tin.get("gia")
        if not la_so_nguyen(gia) or gia <= 0:
            tra_loi({"ok": False, "ma_loi": "sai_du_lieu", "ly_do": "giá phải là số nguyên dương"})
            return
        buoc, gia_han_s = self.tham_so["buoc_gia"], self.tham_so["gia_han_s"]

        def xu_ly(doc):                                 # hàm THUẦN: đọc doc, trả về (kq, ghi)
            m = doc["mon"]
            bay_gio = time.monotonic()
            if m["ket_thuc"] or (m["het_han"] is not None and bay_gio >= m["het_han"]):
                return {"ok": False, "ma_loi": "het_phien", "ly_do": "phiên đã kết thúc",
                        "gia": m["gia"], "nguoi_dan_dau": m["nguoi"]}, None
            if gia < m["gia"] + buoc:
                return {"ok": False, "ma_loi": "gia_thap", "ly_do": f"giá phải ≥ {tien(m['gia'] + buoc)}",
                        "gia": m["gia"], "nguoi_dan_dau": m["nguoi"]}, None
            moi = dict(m, gia=gia, nguoi=nguoi, so_lan=m["so_lan"] + 1)
            gia_han = m["het_han"] is not None and m["het_han"] - bay_gio < gia_han_s
            if gia_han:
                moi["het_han"] = bay_gio + gia_han_s
            return {"ok": True, "gia": gia, "nguoi_dan_dau": nguoi, "gia_han": gia_han}, {"mon": moi}

        self.kho.thuc_hien(["mon"], xu_ly, tra_loi, khi_ghi=self._da_chap_nhan)

    def _da_chap_nhan(self, kq):
        """Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi."""
        self.so.ghi(nguoi=kq["nguoi_dan_dau"], gia=kq["gia"])
        self.lich_su.appendleft({"nguoi": kq["nguoi_dan_dau"], "gia": kq["gia"],
                                 "gio": time.strftime("%H:%M:%S"), "gia_han": kq["gia_han"]})
        if kq["gia_han"] and time.monotonic() - self._luc_bao_gia_han > 1.0:
            self._luc_bao_gia_han = time.monotonic()
            self.thong_bao(f"Gia hạn: {kq['nguoi_dan_dau']} trả giá phút cuối,"
                           f" còn {self.tham_so['gia_han_s']:g} s")
        self.bao_doi()

    # ------------------------------------------------------------ đồng hồ phiên (luồng nền, 100 ms)
    def _dinh_ky(self, bay_gio):
        m = self.kho.doc("mon")
        if m["ket_thuc"] or m["het_han"] is None or bay_gio < m["het_han"] or self._dang_dong:
            return
        self._dang_dong = True

        def xu_ly(doc):                                 # đóng phiên cũng là một thao tác ghi
            m = doc["mon"]
            if m["ket_thuc"] or m["het_han"] is None or time.monotonic() < m["het_han"]:
                return {"ok": False}, None              # vừa được gia hạn hoặc đã đóng
            return {"ok": True, "nguoi": m["nguoi"], "gia": m["gia"]}, {"mon": dict(m, ket_thuc=True)}

        def xong(kq):
            self._dang_dong = False
        self.kho.thuc_hien(["mon"], xu_ly, xong, khi_ghi=self._da_dong)

    def _da_dong(self, kq):
        if kq["nguoi"]:
            self.thong_bao(f"Kết thúc phiên: {kq['nguoi']} thắng với {tien(kq['gia'])}")
        else:
            self.thong_bao("Kết thúc phiên: không có ai trả giá")
        self.bao_doi()

    # ------------------------------------------------------------ trạng thái gửi client
    def _trang_thai(self):
        m = self.kho.doc("mon")
        ts = self.tham_so
        con_lai = None if m["het_han"] is None else max(0, round((m["het_han"] - time.monotonic()) * 1000))
        return {"mon": {"ten": ts["ten_mon"], "gia_khoi_diem": ts["gia_khoi_diem"], "buoc_gia": ts["buoc_gia"],
                        "gia_han_s": ts["gia_han_s"]},
                "gia": m["gia"], "nguoi_dan_dau": m["nguoi"], "so_lan": m["so_lan"],
                "con_lai_ms": con_lai, "ket_thuc": m["ket_thuc"],
                "lich_su": list(self.lich_su)[:12]}

    def _tom_tat(self):
        m = self.kho.doc("mon")
        ai = m["nguoi"] or "–"
        return f"Giá cuối {tien(m['gia'])}, {'thắng' if m['ket_thuc'] else 'dẫn đầu'} {ai}"


_khoa_in = threading.Lock()


def in_dong(tien_to, text):
    """Nhiều luồng cùng in: mỗi dòng in trọn vẹn."""
    with _khoa_in:
        print(f"[{tien_to}] {text}", flush=True)
