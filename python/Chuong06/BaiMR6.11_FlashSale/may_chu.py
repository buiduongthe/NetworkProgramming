"""Bài mở rộng MR6.11 – Lõi máy chủ flash sale: nghiệp vụ + sổ kiểm toán (khung ở chung.py).

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
"Giờ vàng" flash sale: kho có 100 sản phẩm, mở bán lúc T (đếm ngược), hàng nghìn người cùng bấm
"MUA NGAY" đúng lúc mở. Thao tác ghi duy nhất:
    {"loai":"mua","ma_yc":k,"so_luong":1|2[,"nguoi":"u17"]}
Luật: chưa tới giờ mở bán -> "chua_mo" (kèm con_ms); mỗi đơn 1–toi_da (mặc định 2) sản phẩm;
tồn kho phải đủ (còn 1 mà mua 2 -> "khong_du"); hết -> "het_hang".

Trạng thái là MỘT ô "kho" của KhoTranhChap: {"ton","da_ban","so_don"} – điểm nóng mà mọi đơn hàng cùng
tranh. Hai việc KHÔNG cần đi qua vùng găng: kiểm tra giờ mở bán (chỉ đọc đồng hồ) và "hết hàng nhanh"
(đọc tồn kho không khóa: tồn chỉ giảm, nên đã thấy 0 thì chắc chắn là 0) – giúp hàng nghìn yêu cầu
đến sau khi cháy hàng được trả lời ngay mà không phải xếp hàng chờ khóa.

Sổ kiểm toán (tổng đã bán theo sổ, ghi trong vùng găng) đếm hai sai phạm:
  ton_am  – một đơn được chấp nhận làm tổng đã bán vượt tồn đầu kỳ (tồn kho thật bị âm – bán vượt);
  lech_so – cuối buổi: tổng đã bán theo sổ ≠ tồn đầu kỳ − tồn hiện tại (đếm số sản phẩm lệch).
"""
import collections
import threading
import time

from chung import MayChuCoSo, SoKiemToan, la_so_nguyen


class SoKiemToanFlashSale(SoKiemToan):
    TEN_SAI_PHAM = {"ton_am": "tồn âm", "lech_so": "lệch sổ"}

    def __init__(self, ton_dau):
        super().__init__()
        self.ton_dau = ton_dau
        self.tong_ban = 0

    def _kiem_tra(self, gd):
        self.tong_ban += gd["so_luong"]
        if self.tong_ban > self.ton_dau:
            self._sai_pham("ton_am", f"{gd['nguoi']} mua {gd['so_luong']}: bán {self.tong_ban}/{self.ton_dau}")

    def _kiem_tra_cuoi(self, doc):
        k = doc.get("kho")
        if k is None:
            return {}
        lech = abs((self.ton_dau - k["ton"]) - self.tong_ban)
        return {"lech_so": lech} if lech else {}


class MayChuFlashSale(MayChuCoSo):
    TEN = "Flash sale MR6.11"
    LOAI_YEU_CAU = ("mua",)
    THAM_SO_MAC_DINH = {"ten_sp": "Tai nghe không dây TWS Pro", "gia_goc": 1_990_000, "gia_sale": 199_000,
                        "ton_dau": 100, "mo_sau_s": 30.0, "toi_da": 2}

    # ------------------------------------------------------------ tạo / đặt lại đợt sale
    def _kiem_tham_so(self, ts):
        super()._kiem_tham_so(ts)
        if not 1 <= ts.get("ton_dau", 1) <= 1_000_000:
            raise ValueError("tồn đầu kỳ 1–1 000 000")
        if ts.get("mo_sau_s", 0) < 0:
            raise ValueError("mo_sau_s ≥ 0")
        if not 1 <= ts.get("toi_da", 1) <= 10:
            raise ValueError("tối đa mỗi đơn 1–10")
        if ts.get("gia_sale", 1) <= 0 or ts.get("gia_goc", 1) <= 0:
            raise ValueError("giá > 0")

    def _khoi_tao(self):
        ts = self.tham_so
        self.kho.dat_o("kho", {"ton": ts["ton_dau"], "da_ban": 0, "so_don": 0})
        self.so = SoKiemToanFlashSale(ts["ton_dau"])
        self.mo_luc = time.monotonic() + ts["mo_sau_s"]
        self.don_gan = collections.deque(maxlen=30)       # đơn mới nhất trước (để hiển thị)
        self._da_bao_mo = ts["mo_sau_s"] <= 0
        self._da_bao_het = False

    # ------------------------------------------------------------ mua
    def _xu_ly_yeu_cau(self, khach, nguoi, tin, tra_loi):
        sl, toi_da = tin.get("so_luong"), self.tham_so["toi_da"]
        if not la_so_nguyen(sl) or not 1 <= sl <= toi_da:
            tra_loi({"ok": False, "ma_loi": "sai_du_lieu", "ly_do": f"số lượng 1–{toi_da}"})
            return
        con = self.mo_luc - time.monotonic()
        if con > 0:                                        # chưa tới giờ: chỉ đọc đồng hồ, không chạm kho
            tra_loi({"ok": False, "ma_loi": "chua_mo", "ly_do": f"chưa mở bán, còn {con:.1f} s",
                     "con_ms": round(con * 1000)})
            return
        if self.kho.doc("kho")["ton"] <= 0:               # hết hàng nhanh: tồn chỉ giảm, đọc không khóa
            tra_loi({"ok": False, "ma_loi": "het_hang", "ly_do": "hết hàng", "ton": 0})
            return

        def xu_ly(doc):                                    # hàm THUẦN: đọc doc, trả về (kq, ghi)
            k = doc["kho"]
            if k["ton"] <= 0:
                return {"ok": False, "ma_loi": "het_hang", "ly_do": "hết hàng", "ton": 0}, None
            if k["ton"] < sl:
                return {"ok": False, "ma_loi": "khong_du", "ly_do": f"chỉ còn {k['ton']} sản phẩm",
                        "ton": k["ton"]}, None
            moi = {"ton": k["ton"] - sl, "da_ban": k["da_ban"] + sl, "so_don": k["so_don"] + 1}
            return ({"ok": True, "ma_don": f"FS{moi['so_don']:05d}", "so_luong": sl, "ton": moi["ton"],
                     "nguoi": nguoi, "thanh_tien": sl * self.tham_so["gia_sale"]}, {"kho": moi})

        gop = khach.so_nguoi is not None
        self.kho.thuc_hien(["kho"], xu_ly, tra_loi, khi_ghi=lambda kq: self._da_ban(kq, gop))

    def _da_ban(self, kq, gop):
        """Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi."""
        self.so.ghi(nguoi=kq["nguoi"], so_luong=kq["so_luong"], ma_don=kq["ma_don"])
        self.don_gan.appendleft({"nguoi": kq["nguoi"], "so_luong": kq["so_luong"], "gio": time.strftime("%H:%M:%S")})
        if not gop:
            self.thong_bao(f"{kq['nguoi']} mua {kq['so_luong']} – đơn {kq['ma_don']}, còn {kq['ton']}")
        if kq["ton"] == 0 and not self._da_bao_het:
            self._da_bao_het = True
            self.thong_bao(f"HẾT HÀNG sau {time.monotonic() - self.mo_luc:.2f} s mở bán")
        self.bao_doi()

    # ------------------------------------------------------------ đồng hồ mở bán (luồng nền, 100 ms)
    def _dinh_ky(self, bay_gio):
        if not self._da_bao_mo and bay_gio >= self.mo_luc:
            self._da_bao_mo = True
            self.thong_bao(f"MỞ BÁN! {self.tham_so['ton_dau']} sản phẩm")
            self.bao_doi()

    # ------------------------------------------------------------ trạng thái gửi client
    def _trang_thai(self):
        ts, k = self.tham_so, self.kho.doc("kho")
        con = self.mo_luc - time.monotonic()
        return {"sp": {"ten": ts["ten_sp"], "gia_goc": ts["gia_goc"], "gia_sale": ts["gia_sale"],
                       "toi_da": ts["toi_da"]},
                "ton_dau": ts["ton_dau"], "ton": k["ton"], "da_ban": k["da_ban"], "so_don": k["so_don"],
                "ban_theo_so": self.so.tong_ban, "mo_ban": con <= 0, "mo_con_ms": max(0, round(con * 1000)),
                "don_gan": list(self.don_gan)[:10]}

    def _tom_tat(self):
        k = self.kho.doc("kho")
        return f"Bán {self.so.tong_ban}/{self.tham_so['ton_dau']} sp ({len(self.so.giao_dich)} đơn), tồn {k['ton']}"


_khoa_in = threading.Lock()


def in_dong(tien_to, text):
    """Nhiều luồng cùng in: mỗi dòng in trọn vẹn."""
    with _khoa_in:
        print(f"[{tien_to}] {text}", flush=True)

