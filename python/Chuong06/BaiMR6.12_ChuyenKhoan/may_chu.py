"""Bài mở rộng MR6.12 – Lõi máy chủ ngân hàng: chuyển khoản + sổ kiểm toán (khung ở chung.py).

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
5–20 tài khoản, mỗi tài khoản là MỘT ô của KhoTranhChap (giá trị = số dư, số nguyên đồng). Thao tác ghi:
    {"loai":"chuyen","ma_yc":k,"tu":"TK01","den":"TK02","so_tien":300000[,"noi_dung":"..."][,"nguoi":"u17"]}
Luật: trừ tài khoản nguồn, cộng tài khoản đích trong CÙNG một thao tác (hai ô); số dư nguồn không đủ thì
từ chối. Nhiều người chuyển chéo cùng lúc (A→B và B→A) nên một thao tác luôn đụng HAI tài nguyên:
  khoa            khóa hai tài khoản theo THỨ TỰ MÃ (TK01 trước TK02) -> mọi luồng xin khóa cùng thứ tự,
                  không thể có vòng chờ -> không khóa chết;
  khoa_ngay_tho=1 khóa nguồn rồi mới khóa đích: A→B giữ A chờ B, B→A giữ B chờ A -> KHÓA CHẾT (deadlock).
                  Khung phát hiện bằng hết thời gian chờ khóa (2 s): giao dịch "nạn nhân" bị hủy, nhả khóa,
                  giao dịch kia đi tiếp; máy chủ báo "khóa chết: …";
  lac_quan        đọc (số dư, phiên bản) của cả hai tài khoản, rồi so sánh-và-đổi CẢ HAI một cách nguyên tử.

Sổ kiểm toán ghi mọi giao dịch được chấp nhận và tự tính số dư theo sổ; đếm ba sai phạm:
  so_du_am  "thấu chi"  – theo sổ, một giao dịch làm tài khoản nguồn âm (hai luồng cùng thấy "đủ tiền"
                         rồi cùng trừ);
  so_du_sai "lệch TK"   – số tài khoản có số dư cuối khác số dư theo sổ (mất cập nhật);
  tong_lech "tổng lệch" – tổng tiền toàn hệ thống khác tổng ban đầu (tiền tự sinh ra / mất đi).
"""
import collections
import itertools
import threading
import time

from chung import CHO_KHOA_TOI_DA_S, MayChuCoSo, SoKiemToan, la_so_nguyen, tien

CHU_TK = ("An", "Bình", "Chi", "Dũng", "Giang", "Hà", "Khoa", "Lan", "Minh", "Nam",
          "Oanh", "Phúc", "Quân", "Sơn", "Tâm", "Uyên", "Vy", "Xuân", "Yến", "Thảo")


def ma_tk(i):
    return f"TK{i + 1:02d}"


def tien_dau(x):
    """+300.000 đ / -300.000 đ / 0 đ."""
    return ("+" if x > 0 else "") + tien(x)


class SoKiemToanNganHang(SoKiemToan):
    TEN_SAI_PHAM = {"tong_lech": "tổng lệch", "so_du_sai": "lệch TK", "so_du_am": "thấu chi"}

    def __init__(self, so_du_dau):
        super().__init__()
        self.so_du = dict(so_du_dau)          # số dư theo sổ (cộng trừ đúng từng giao dịch đã chấp nhận)
        self.tong_dau = sum(so_du_dau.values())

    def _kiem_tra(self, gd):
        tu, den, x = gd["tu"], gd["den"], gd["so_tien"]
        self.so_du[tu] -= x
        self.so_du[den] += x
        if self.so_du[tu] < 0:
            self._sai_pham("so_du_am", f"{gd['nguoi']} {tu}→{den}: {tu} còn {tien(self.so_du[tu])}")

    def _kiem_tra_cuoi(self, doc):
        if any(k not in doc for k in self.so_du):
            return {}
        sai = sum(1 for k, v in self.so_du.items() if doc[k] != v)
        tong = sum(doc[k] for k in self.so_du)
        return {"so_du_sai": sai, "tong_lech": 1 if tong != self.tong_dau else 0}


class MayChuNganHang(MayChuCoSo):
    TEN = "Ngân hàng MR6.12"
    LOAI_YEU_CAU = ("chuyen",)
    THAM_SO_MAC_DINH = {"so_tk": 10, "so_du_dau": 5_000_000, "khoa_ngay_tho": 0}

    # ------------------------------------------------------------ tạo / đặt lại
    def _kiem_tham_so(self, ts):
        super()._kiem_tham_so(ts)
        if not 5 <= ts.get("so_tk", 10) <= len(CHU_TK):
            raise ValueError(f"số tài khoản 5–{len(CHU_TK)}")
        if ts.get("so_du_dau", 0) < 0:
            raise ValueError("số dư ban đầu ≥ 0")
        if ts.get("khoa_ngay_tho", 0) not in (0, 1):
            raise ValueError("khoa_ngay_tho = 0 hoặc 1")

    def _khoi_tao(self):
        ts = self.tham_so
        ds = [ma_tk(i) for i in range(ts["so_tk"])]
        for m in ds:
            self.kho.dat_o(m, ts["so_du_dau"])
        self.kho.khoa_ngay_tho = bool(ts["khoa_ngay_tho"])
        self.so = SoKiemToanNganHang({m: ts["so_du_dau"] for m in ds})
        self.chu = {m: CHU_TK[i] for i, m in enumerate(ds)}
        self.ds_tk = ds
        self.tong_dau = ts["so_tk"] * ts["so_du_dau"]
        self.lich_su = collections.deque(maxlen=50)   # giao dịch đã chấp nhận, mới nhất trước
        if not hasattr(self, "_ma_gd"):              # mã giao dịch tăng mãi, kể cả qua các lần đặt lại
            self._ma_gd = itertools.count(1)
        self.so_gd = 0
        self._khoa_kc = threading.Lock()
        self.khoa_chet, self._kc_chua_bao, self._luc_bao_kc = 0, 0, 0.0

    # ------------------------------------------------------------ chuyển khoản
    def _xu_ly_yeu_cau(self, khach, nguoi, tin, tra_loi):
        tu, den, x = tin.get("tu"), tin.get("den"), tin.get("so_tien")
        noi_dung = tin.get("noi_dung", "")
        if not la_so_nguyen(x) or x <= 0:
            tra_loi({"ok": False, "ma_loi": "sai_du_lieu", "ly_do": "số tiền phải là số nguyên dương"})
            return
        if tu not in self.chu or den not in self.chu:
            tra_loi({"ok": False, "ma_loi": "khong_co", "ly_do": "tài khoản không tồn tại"})
            return
        if tu == den:
            tra_loi({"ok": False, "ma_loi": "sai_du_lieu", "ly_do": "tài khoản nguồn và đích trùng nhau"})
            return
        if not isinstance(noi_dung, str) or len(noi_dung) > 40:
            tra_loi({"ok": False, "ma_loi": "sai_du_lieu", "ly_do": "nội dung tối đa 40 ký tự"})
            return

        def xu_ly(doc):                                 # hàm THUẦN: đọc hai số dư, trả về (kq, ghi)
            a, b = doc[tu], doc[den]
            if a < x:
                return {"ok": False, "ma_loi": "khong_du", "ly_do": f"không đủ số dư ({tu} còn {tien(a)})",
                        "so_du": a}, None
            return ({"ok": True, "tu": tu, "den": den, "so_tien": x, "so_du": a - x, "nguoi": nguoi,
                     "noi_dung": noi_dung}, {tu: a - x, den: b + x})

        def tra(kq):
            if kq.get("ma_loi") == "khoa_cho_lau":
                kq = self._bao_khoa_chet(nguoi, tu, den, kq)
            tra_loi(kq)
        # thứ tự [nguồn, đích]: chế độ khoa sắp xếp lại theo mã, khóa ngây thơ thì giữ nguyên
        self.kho.thuc_hien([tu, den], xu_ly, tra, khi_ghi=self._da_chuyen)

    def _da_chuyen(self, kq):
        """Gọi trong vùng găng ngay sau khi ghi hai số dư: ghi sổ kiểm toán đúng thứ tự ghi."""
        kq["ma_gd"] = next(self._ma_gd)
        self.so_gd += 1
        self.so.ghi(nguoi=kq["nguoi"], tu=kq["tu"], den=kq["den"], so_tien=kq["so_tien"])
        self.lich_su.appendleft({"ma": kq["ma_gd"], "gio": time.strftime("%H:%M:%S"), "nguoi": kq["nguoi"],
                                 "tu": kq["tu"], "den": kq["den"], "so_tien": kq["so_tien"],
                                 "noi_dung": kq["noi_dung"]})
        self.bao_doi()

    def _bao_khoa_chet(self, nguoi, tu, den, kq):
        """Chờ khóa quá 2 s: với khóa ngây thơ đó là khóa chết. Báo (tối đa 1 dòng/giây) và hủy giao dịch."""
        k = kq["ly_do"].split()[2]                     # "chờ khóa TK02 quá 2 s (khóa chết?)"
        ngay_tho = self.kho.khoa_ngay_tho
        with self._khoa_kc:
            self.khoa_chet += 1
            bay_gio = time.monotonic()
            bao = bay_gio - self._luc_bao_kc >= 1.0
            if bao:
                self._luc_bao_kc, them, self._kc_chua_bao = bay_gio, self._kc_chua_bao, 0
            else:
                self._kc_chua_bao += 1
        if bao:
            self.thong_bao(f"{'khóa chết' if ngay_tho else 'chờ khóa quá hạn'}: {nguoi} {tu}→{den},"
                           f" chờ {k} quá {CHO_KHOA_TOI_DA_S:g} s" + (f" (+{them} vụ)" if them else ""))
        ly_do = ("khóa chết: " if ngay_tho else "") + f"chờ {k} quá {CHO_KHOA_TOI_DA_S:g} s, đã hủy giao dịch"
        return dict(kq, ly_do=ly_do)

    # ------------------------------------------------------------ trạng thái gửi client
    def _trang_thai(self):
        snap = self.kho.anh_chup()
        ds, chu = self.ds_tk, self.chu
        tk = [{"ma": m, "chu": chu[m], "so_du": snap.get(m, 0)} for m in ds]
        return {"tai_khoan": tk, "tong": sum(t["so_du"] for t in tk), "tong_dau": self.tong_dau,
                "khoa_ngay_tho": self.kho.khoa_ngay_tho, "khoa_chet": self.khoa_chet, "so_gd": self.so_gd,
                "lich_su": list(self.lich_su)[:20]}

    def _tom_tat(self):
        snap = self.kho.anh_chup()
        tong = sum(snap.get(m, 0) for m in self.ds_tk)
        lech = tong - self.tong_dau
        if lech:
            return f"Tổng tiền {tien(tong)}, LỆCH {tien_dau(lech)}"
        return f"Tổng tiền {tien(tong)} (khớp), {len(self.ds_tk)} tài khoản"


_khoa_in = threading.Lock()


def in_dong(tien_to, text):
    """Nhiều luồng cùng in: mỗi dòng in trọn vẹn."""
    with _khoa_in:
        print(f"[{tien_to}] {text}", flush=True)
