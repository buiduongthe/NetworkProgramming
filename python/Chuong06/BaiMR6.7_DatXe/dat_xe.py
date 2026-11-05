"""Bài mở rộng MR6.7 – Người gọi xe (bản dòng lệnh).

Kết nối máy chủ, in diễn biến (số xe trống, xe của mình bị "nhận khách khác") và nhận lệnh:
  dat            đặt xe từ điểm đón mặc định của mình (theo tên) tới điểm đến mặc định
  huy            hủy chuyến đã đặt                       thoat
Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).

Chạy:  python3 dat_xe.py <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
"""
import argparse
import threading
import time

from chung import doc_lenh_stdin, tien
from khach import KetNoiKhach
from may_chu import NGA_TU, cuoc, in_dong, km, ten_diem


def diem_mac_dinh(ten):
    """Điểm đón và điểm đến cố định theo tên (để mỗi người một chỗ đứng khác nhau)."""
    s = sum(ord(c) for c in ten)
    don = NGA_TU[(s * 7) % len(NGA_TU)]
    den = NGA_TU[(s * 11 + 9) % len(NGA_TU)]
    if km(don, den) < 5:                               # quá gần: chọn góc xa nhất
        den = (0.9 if don[0] < 0.5 else 0.1, 0.9 if don[1] < 0.5 else 0.1)
    return don, den


def xe_cua(tt, ten):
    return next((x for x in tt["xe"] if x["khach"] == ten), None)


class NguoiGoiXe:
    def __init__(self, ten):
        self.ten = ten
        self.don, self.den = diem_mac_dinh(ten)
        self.tt = None                  # cap_nhat mới nhất
        self.xe_toi = None              # mã xe mình được báo đặt thành công
        self.phien_toi = 0              # số thứ tự lần ghi (kq) của chuyến mình đặt
        self.dang_huy = False
        self.khoa = threading.Lock()
        self.kn = KetNoiKhach(khi_cap_nhat=self.cap_nhat, khi_tin=self.tin_khac, khi_dong=self.mat_ket_noi)

    def log(self, text):
        in_dong(self.ten, text)

    def ket_noi(self, host, cong):
        self.kn.ket_noi(host, cong, self.ten)
        tt = self.tt = self.kn.trang_thai
        self.log(f"Mở ứng dụng ({tt['che_do']}): {tt['trong']}/{tt['so_xe']} xe trống")
        self.log(f"  đón: {ten_diem(self.don)}")
        self.log(f"  đến: {ten_diem(self.den)}")
        self.kn.bat_dau_doc()

    # ------------------------------------------------------------ nhận
    def cap_nhat(self, tt):
        with self.khoa:
            cu, self.tt = self.tt, tt
        if tt["trong"] != cu["trong"]:
            self.log(f"Xe trống: {tt['trong']}/{tt['so_xe']}")
        if self.xe_toi is not None and not self.dang_huy and tt["phien"] >= self.phien_toi:   # bỏ qua ảnh chụp trước lần ghi của mình
            xe = next((x for x in tt["xe"] if x["ma"] == self.xe_toi), None)
            if xe is not None and xe["khach"] not in (None, self.ten):
                self.log(f"  Tài xế {xe['tai_xe']} đã nhận khách khác ({xe['khach']})!")
                self.xe_toi = None
            elif xe is None or xe["khach"] is None:
                self.log("  Chuyến đã kết thúc (máy chủ đặt lại)")
                self.xe_toi = None

    def tin_khac(self, tin):
        if tin.get("loai") == "su_kien":
            self.log(f"* {tin['noi_dung']}")

    def mat_ket_noi(self):
        self.log("Mất kết nối máy chủ")

    # ------------------------------------------------------------ gửi
    def dat(self):
        self.log(f"> Đặt xe – đang tìm tài xế… (dự kiến {tien(cuoc(self.don, self.den))})")

        def kq(tin, rtt):
            if tin.get("ok"):
                self.xe_toi, self.phien_toi = tin["xe"], tin["phien"]
                self.log(f"  ĐẶT THÀNH CÔNG – tài xế {tin['tai_xe']} ({rtt:.1f} ms)")
                self.log(f"  {tin['bien_so']}, xe {tin['mau']}, đến sau {tin['phut']} phút,"
                         f" cước {tien(tin['cuoc'])}")
            elif tin.get("ma_loi") == "het_cho":
                self.log(f"  HẾT CHỖ: {tin.get('ly_do')}")
            else:
                self.log(f"  TỪ CHỐI: {tin.get('ly_do')}")
        self.kn.gui_yc("dat_xe", kq, don=list(self.don), den=list(self.den))

    def huy(self):
        self.log("> Hủy chuyến")
        self.dang_huy = True

        def kq(tin, rtt):
            self.dang_huy = False
            if tin.get("ok"):
                self.xe_toi = None
                self.log(f"  Đã hủy, xe {tin['xe']} ({tin['tai_xe']}) trống")
            else:
                self.log(f"  TỪ CHỐI: {tin.get('ly_do')}")
        self.kn.gui_yc("huy", kq)

    def lenh(self, dong):
        p = dong.split()
        try:
            if p == ["dat"]:
                self.dat()
            elif p == ["huy"]:
                self.huy()
            elif p == ["thoat"]:
                self.kn.khi_dong = lambda: None
                self.kn.dong()
            else:
                self.log("lệnh: dat | huy | thoat")
        except (ValueError, OSError) as e:
            self.log(f"lỗi: {e}")


def main():
    ap = argparse.ArgumentParser(description="Người gọi xe")
    ap.add_argument("ten")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41710)
    ap.add_argument("--thoi-gian", type=float, default=0.0)
    a = ap.parse_args()
    t0 = time.monotonic()
    ng = NguoiGoiXe(a.ten)
    ng.ket_noi(a.host, a.cong)
    het_stdin = threading.Event()
    threading.Thread(target=doc_lenh_stdin, args=(t0, ng.lenh, het_stdin.set), daemon=True).start()
    try:
        if a.thoi_gian > 0:                      # chạy đúng --thoi-gian giây (hoặc tới khi thoat)
            while ng.kn.mo and time.monotonic() - t0 < a.thoi_gian:
                time.sleep(0.1)
        else:                                    # tới khi hết stdin / thoat
            while ng.kn.mo and not het_stdin.wait(0.1):
                pass
            time.sleep(0.3)                      # chờ trả lời của lệnh cuối
    except KeyboardInterrupt:
        pass
    ng.kn.khi_dong = lambda: None
    ng.kn.dong()


if __name__ == "__main__":
    main()
