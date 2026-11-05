"""Bài mở rộng MR6.11 – Người mua flash sale (bản dòng lệnh).

Kết nối máy chủ, in diễn biến (đếm ngược, mở bán, tồn kho, hết hàng) và nhận lệnh:
  mua [n]     đặt một đơn n sản phẩm (mặc định 1)          thoat
Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).

Chạy:  python3 flash_sale.py <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
"""
import argparse
import threading
import time

from chung import doc_lenh_stdin, tien
from khach import KetNoiKhach
from may_chu import in_dong


class NguoiMua:
    def __init__(self, ten):
        self.ten = ten
        self.tt = None
        self.kn = KetNoiKhach(khi_cap_nhat=self.cap_nhat, khi_tin=self.tin_khac, khi_dong=self.mat_ket_noi)

    def log(self, text):
        in_dong(self.ten, text)

    def ket_noi(self, host, cong):
        self.kn.ket_noi(host, cong, self.ten)
        tt = self.tt = self.kn.trang_thai
        sp = tt["sp"]
        self.log(f"Vào trang sale ({tt['che_do']}): {sp['ten']}")
        self.log(f"  {tien(sp['gia_goc'])} -> {tien(sp['gia_sale'])}, kho {tt['ton']}/{tt['ton_dau']}")
        if not tt["mo_ban"]:
            self.log(f"  mở bán sau {tt['mo_con_ms'] / 1000:.1f} s")
        self.kn.bat_dau_doc()

    # ------------------------------------------------------------ nhận
    def cap_nhat(self, tt):
        cu, self.tt = self.tt, tt
        if tt["ton"] == 0 and cu["ton"] != 0:
            self.log("  kho: HẾT HÀNG")

    def tin_khac(self, tin):
        if tin.get("loai") == "su_kien":
            self.log(f"* {tin['noi_dung']}")

    def mat_ket_noi(self):
        self.log("Mất kết nối máy chủ")

    # ------------------------------------------------------------ gửi
    def mua(self, sl):
        self.log(f"> MUA NGAY {sl} sản phẩm")

        def kq(tin, rtt):
            if tin.get("ok"):
                self.log(f"  ĐẶT HÀNG OK: đơn {tin['ma_don']}, {tien(tin['thanh_tien'])} ({rtt:.1f} ms)")
            else:
                self.log(f"  TỪ CHỐI: {tin.get('ly_do')}")
        self.kn.gui_yc("mua", kq, so_luong=sl)

    def lenh(self, dong):
        p = dong.split()
        try:
            if p[0] == "mua" and len(p) <= 2:
                self.mua(int(p[1]) if len(p) == 2 else 1)
            elif p[0] == "thoat":
                self.kn.khi_dong = lambda: None
                self.kn.dong()
            else:
                self.log("lệnh: mua [n] | thoat")
        except (ValueError, OSError) as e:
            self.log(f"lỗi: {e}")


def main():
    ap = argparse.ArgumentParser(description="Người mua flash sale")
    ap.add_argument("ten")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41750)
    ap.add_argument("--thoi-gian", type=float, default=0.0)
    a = ap.parse_args()
    t0 = time.monotonic()
    ng = NguoiMua(a.ten)
    ng.ket_noi(a.host, a.cong)
    het_stdin = threading.Event()
    threading.Thread(target=doc_lenh_stdin, args=(t0, ng.lenh, het_stdin.set), daemon=True).start()
    try:
        if a.thoi_gian > 0:
            while ng.kn.mo and time.monotonic() - t0 < a.thoi_gian:
                time.sleep(0.1)
        else:
            while ng.kn.mo and not het_stdin.wait(0.1):
                pass
            time.sleep(0.5)                      # chờ trả lời của lệnh cuối
    except KeyboardInterrupt:
        pass
    ng.kn.khi_dong = lambda: None
    ng.kn.dong()


if __name__ == "__main__":
    main()
