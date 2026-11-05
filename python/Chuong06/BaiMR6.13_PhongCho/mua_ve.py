"""Bài mở rộng MR6.13 – Người mua vé (bản dòng lệnh).

Kết nối máy chủ, in thông tin sự kiện, vị trí trong phòng chờ (số người phía trước thay đổi theo cap_nhat),
lúc tới lượt và kết quả mua; nhận lệnh:
  xep_hang        vào phòng chờ (rồi tự "chờ dài" cho_luot tới khi tới lượt)
  mua <n>         mua n vé          mua2 <n>   bấm ĐÚP: gửi hai yêu cầu mua liền nhau        thoat
--tu-dong: vào hàng ngay (phòng chờ tắt thì mua ngay), tới lượt thì tự mua --so-ve vé.
Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).

Chạy:  python3 mua_ve.py <tên> [host] [cổng] [--tu-dong] [--so-ve 2] [--thoi-gian 0]
"""
import argparse
import threading
import time

from chung import doc_lenh_stdin, tien
from khach import KetNoiKhach
from may_chu import in_dong


class NguoiMua:
    def __init__(self, ten, tu_dong=False, so_ve=2):
        self.ten, self.tu_dong, self.so_ve = ten, tu_dong, so_ve
        self.tt = None
        self.so = None                   # số thứ tự trong hàng
        self.truoc_cu = None
        self.khoa = threading.Lock()
        self.kn = KetNoiKhach(khi_cap_nhat=self.cap_nhat, khi_tin=self.tin_khac, khi_dong=self.mat_ket_noi)

    def log(self, text):
        in_dong(self.ten, text)

    def ket_noi(self, host, cong):
        self.kn.ket_noi(host, cong, self.ten)
        tt = self.tt = self.kn.trang_thai
        sk = tt["su_kien"]
        self.log(f"Trang bán vé: {sk['ten']} ({tt['che_do']})")
        self.log(f"  còn {tt['con_lai']}/{sk['tong_ve']} vé, {tien(sk['gia_ve'])}/vé")
        self.log("  phòng chờ " + (f"BẬT, cấp lượt {tt['toc_do']:g} người/s" if tt["phong_cho"] else "TẮT"))
        self.kn.bat_dau_doc()

    # ------------------------------------------------------------ nhận
    def cap_nhat(self, tt):
        with self.khoa:
            self.tt = tt
            if self.so is None or self.truoc_cu is None:
                return
            truoc = max(0, self.so - tt["dang_phuc_vu"] - 1)
            doi, self.truoc_cu = truoc != self.truoc_cu, truoc
        if doi and truoc > 0:
            self.log(f"  (hàng chờ) còn {truoc} người phía trước, ~{(truoc + 1) / tt['toc_do']:.1f} s")

    def tin_khac(self, tin):
        if tin.get("loai") == "su_kien":
            self.log(f"* {tin['noi_dung']}")

    def mat_ket_noi(self):
        self.log("Mất kết nối máy chủ")

    # ------------------------------------------------------------ gửi
    def xep_hang(self):
        self.log("> vào phòng chờ")

        def kq(tin, rtt):
            if not tin.get("ok"):
                self.log(f"  TỪ CHỐI: {tin.get('ly_do')}")
                return
            with self.khoa:
                self.so, self.truoc_cu = tin["so"], tin["truoc"]
            self.log(f"  số thứ tự #{tin['so']}, còn {tin['truoc']} người phía trước, ~{tin['cho_s']:.1f} s")
            self.kn.gui_yc("cho_luot", den_luot)

        def den_luot(tin, rtt):
            with self.khoa:
                self.truoc_cu = None
            if not tin.get("ok"):
                self.log(f"  {tin.get('ly_do')}")
                return
            self.log(f"  ĐẾN LƯỢT! (chờ {rtt / 1000:.1f} s) còn {tin['han_ms'] / 1000:.1f} s để mua")
            if self.tu_dong:
                self.mua(self.so_ve)
        self.kn.gui_yc("xep_hang", kq)

    def mua(self, n, lan=1):
        self.log(f"> mua {n} vé" + (" (bấm đúp)" if lan > 1 else ""))

        def kq(tin, rtt):
            if tin.get("ok"):
                self.log(f"  THÀNH CÔNG ({rtt:.1f} ms): đơn #{tin['ma_don']}, {tin['so_ve']} vé,"
                         f" {tien(tin['thanh_tien'])}")
            else:
                self.log(f"  TỪ CHỐI ({rtt:.1f} ms): {tin.get('ly_do')}")
        for _ in range(lan):                     # bấm đúp: hai yêu cầu, không chờ trả lời
            self.kn.gui_yc("mua", kq, so_ve=n)

    def lenh(self, dong):
        p = dong.split()
        try:
            if p[0] == "xep_hang":
                self.xep_hang()
            elif p[0] in ("mua", "mua2") and len(p) == 2:
                self.mua(int(p[1]), 2 if p[0] == "mua2" else 1)
            elif p[0] == "thoat":
                self.kn.khi_dong = lambda: None
                self.kn.dong()
            else:
                self.log("lệnh: xep_hang | mua <n> | mua2 <n> | thoat")
        except (ValueError, OSError) as e:
            self.log(f"lỗi: {e}")


def main():
    ap = argparse.ArgumentParser(description="Người mua vé")
    ap.add_argument("ten")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41770)
    ap.add_argument("--tu-dong", action="store_true", help="vào hàng ngay, tới lượt thì tự mua")
    ap.add_argument("--so-ve", type=int, default=2)
    ap.add_argument("--thoi-gian", type=float, default=0.0)
    a = ap.parse_args()
    t0 = time.monotonic()
    ng = NguoiMua(a.ten, a.tu_dong, a.so_ve)
    ng.ket_noi(a.host, a.cong)
    if a.tu_dong:
        if ng.tt["phong_cho"]:
            ng.xep_hang()
        else:
            ng.mua(a.so_ve)
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
