"""Bài mở rộng MR6.10 – Sinh viên đăng ký học phần (bản dòng lệnh).

Kết nối máy chủ, in bảng lớp (mã lớp, tên, đã đăng ký/sĩ số) và nhận lệnh:
  dk <mã lớp>       đăng ký lớp            dk <mã lớp> x2    "bấm đúp": gửi HAI yêu cầu liên tiếp không chờ
  huy <mã lớp>      hủy lớp đã đăng ký     xem               in lại bảng lớp            thoat
Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).

Chạy:  python3 dang_ky.py <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
"""
import argparse
import threading
import time

from chung import doc_lenh_stdin
from khach import KetNoiKhach
from may_chu import in_dong


class SinhVien:
    def __init__(self, ten):
        self.ten = ten
        self.tt = None
        self.kn = KetNoiKhach(khi_cap_nhat=self.cap_nhat, khi_tin=self.tin_khac, khi_dong=self.mat_ket_noi)

    def log(self, text):
        in_dong(self.ten, text)

    def ket_noi(self, host, cong):
        self.kn.ket_noi(host, cong, self.ten)
        self.tt = self.kn.trang_thai
        self.log(f"Vào cổng đăng ký ({self.tt['che_do']}), sĩ số {self.tt['si_so']}")
        self.xem()
        self.kn.bat_dau_doc()

    def xem(self):
        for x in self.tt["lop"]:
            cua_toi = " *" if self.ten in x["sv"] else ""
            self.log(f"  {x['ma']} {x['ten']:<15} {x['da_dk']:>3}/{x['si_so']}{cua_toi}")

    # ------------------------------------------------------------ nhận
    def cap_nhat(self, tt):
        self.tt = tt

    def tin_khac(self, tin):
        if tin.get("loai") == "su_kien":
            self.log(f"* {tin['noi_dung']}")

    def mat_ket_noi(self):
        self.log("Mất kết nối máy chủ")

    # ------------------------------------------------------------ gửi
    def gui(self, loai, ma):
        def kq(tin, rtt):
            if tin.get("ok"):
                viec = "ĐĂNG KÝ" if loai == "dang_ky" else "HỦY"
                self.log(f"  {viec} {ma} thành công ({tin['da_dk']}/{self.tt['si_so']}, {rtt:.1f} ms)")
            else:
                self.log(f"  {ma}: TỪ CHỐI – {tin.get('ly_do')}")
        self.kn.gui_yc(loai, kq, ma_lop=ma)

    def lenh(self, dong):
        p = dong.split()
        try:
            if p[0] == "dk" and len(p) == 2:
                self.log(f"> đăng ký {p[1]}")
                self.gui("dang_ky", p[1])
            elif p[0] == "dk" and len(p) == 3 and p[2] == "x2":
                self.log(f"> đăng ký {p[1]} (bấm đúp: 2 yêu cầu)")
                self.gui("dang_ky", p[1])
                self.gui("dang_ky", p[1])
            elif p[0] == "huy" and len(p) == 2:
                self.log(f"> hủy {p[1]}")
                self.gui("huy", p[1])
            elif p[0] == "xem":
                self.xem()
            elif p[0] == "thoat":
                self.kn.khi_dong = lambda: None
                self.kn.dong()
            else:
                self.log("lệnh: dk <mã> [x2] | huy <mã> | xem | thoat")
        except OSError as e:
            self.log(f"lỗi: {e}")


def main():
    ap = argparse.ArgumentParser(description="Sinh viên đăng ký học phần")
    ap.add_argument("ten")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41740)
    ap.add_argument("--thoi-gian", type=float, default=0.0)
    a = ap.parse_args()
    t0 = time.monotonic()
    sv = SinhVien(a.ten)
    sv.ket_noi(a.host, a.cong)
    het_stdin = threading.Event()
    threading.Thread(target=doc_lenh_stdin, args=(t0, sv.lenh, het_stdin.set), daemon=True).start()
    try:
        if a.thoi_gian > 0:
            while sv.kn.mo and time.monotonic() - t0 < a.thoi_gian:
                time.sleep(0.1)
        else:
            while sv.kn.mo and not het_stdin.wait(0.1):
                pass
            time.sleep(0.5)                      # chờ trả lời của lệnh cuối
    except KeyboardInterrupt:
        pass
    sv.kn.khi_dong = lambda: None
    sv.kn.dong()


if __name__ == "__main__":
    main()
