"""Bài mở rộng MR6.12 – Khách hàng ngân hàng (bản dòng lệnh).

Kết nối máy chủ, đăng nhập một tài khoản (mặc định tài khoản mang tên mình, ví dụ An -> TK01), in số dư,
tiền đến ("+ Nhận …") và kết quả chuyển khoản; nhận lệnh:
  chuyen <TK đích> <số tiền> [nội dung]     so_du     thoat
Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).

Chạy:  python3 ngan_hang.py <tên> [host] [cổng] [--tk TK01] [--thoi-gian 0]   (0 = tới khi hết stdin/thoat)
"""
import argparse
import threading
import time

from chung import doc_lenh_stdin, tien
from khach import KetNoiKhach
from may_chu import in_dong


class KhachHang:
    def __init__(self, ten, tk=None):
        self.ten, self.tk = ten, tk
        self.tt = None
        self.da_thay = set()             # ma_gd đã biết (không báo tiền đến hai lần)
        self.khoa = threading.Lock()
        self.kn = KetNoiKhach(khi_cap_nhat=self.cap_nhat, khi_tin=self.tin_khac, khi_dong=self.mat_ket_noi)

    def log(self, text):
        in_dong(self.ten, text)

    def so_du(self, tt=None):
        tt = tt or self.tt
        return next(t["so_du"] for t in tt["tai_khoan"] if t["ma"] == self.tk)

    def chu(self, ma):
        return next((t["chu"] for t in self.tt["tai_khoan"] if t["ma"] == ma), "?")

    def ket_noi(self, host, cong):
        self.kn.ket_noi(host, cong, self.ten)
        tt = self.tt = self.kn.trang_thai
        if self.tk is None:              # tài khoản mang tên mình, không có thì TK01
            self.tk = next((t["ma"] for t in tt["tai_khoan"] if t["chu"] == self.ten), tt["tai_khoan"][0]["ma"])
        self.da_thay = {g["ma"] for g in tt["lich_su"]}
        self.log(f"Đăng nhập {self.tk} – {self.chu(self.tk)} ({tt['che_do']})")
        self.log(f"  số dư {tien(self.so_du())}")
        self.kn.bat_dau_doc()

    # ------------------------------------------------------------ nhận
    def cap_nhat(self, tt):
        with self.khoa:
            self.tt = tt
            moi = [g for g in reversed(tt["lich_su"]) if g["ma"] not in self.da_thay]
            self.da_thay.update(g["ma"] for g in moi)
        for g in moi:
            if g["den"] == self.tk:
                self.log(f"+ Nhận {tien(g['so_tien'])} từ {g['tu']} ({self.chu(g['tu'])}),"
                         f" số dư {tien(self.so_du(tt))}")

    def tin_khac(self, tin):
        if tin.get("loai") == "su_kien":
            self.log(f"* {tin['noi_dung']}")

    def mat_ket_noi(self):
        self.log("Mất kết nối máy chủ")

    # ------------------------------------------------------------ gửi
    def chuyen(self, den, so_tien, noi_dung=""):
        self.log(f"> chuyển {tien(so_tien)} {self.tk}→{den}")

        def kq(tin, rtt):
            if tin.get("ok"):
                with self.khoa:
                    self.da_thay.add(tin["ma_gd"])
                self.log(f"  THÀNH CÔNG ({rtt:.1f} ms), số dư {tien(tin['so_du'])}")
            else:
                self.log(f"  TỪ CHỐI ({rtt:.0f} ms): {tin.get('ly_do')}")
        self.kn.gui_yc("chuyen", kq, tu=self.tk, den=den, so_tien=so_tien, noi_dung=noi_dung)

    def lenh(self, dong):
        p = dong.split()
        try:
            if p[0] == "chuyen" and len(p) >= 3:
                self.chuyen(p[1], int(p[2]), " ".join(p[3:]))
            elif p[0] == "so_du":
                with self.khoa:
                    self.log(f"Số dư {self.tk}: {tien(self.so_du())}")
            elif p[0] == "thoat":
                self.kn.khi_dong = lambda: None
                self.kn.dong()
            else:
                self.log("lệnh: chuyen <TK> <số tiền> [nội dung] | so_du | thoat")
        except (ValueError, OSError) as e:
            self.log(f"lỗi: {e}")


def main():
    ap = argparse.ArgumentParser(description="Khách hàng ngân hàng")
    ap.add_argument("ten")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41760)
    ap.add_argument("--tk", default=None, help="mã tài khoản (mặc định: tài khoản mang tên mình)")
    ap.add_argument("--thoi-gian", type=float, default=0.0)
    a = ap.parse_args()
    t0 = time.monotonic()
    kh = KhachHang(a.ten, a.tk)
    kh.ket_noi(a.host, a.cong)
    het_stdin = threading.Event()
    threading.Thread(target=doc_lenh_stdin, args=(t0, kh.lenh, het_stdin.set), daemon=True).start()
    try:
        if a.thoi_gian > 0:                      # chạy đúng --thoi-gian giây (hoặc tới khi thoat)
            while kh.kn.mo and time.monotonic() - t0 < a.thoi_gian:
                time.sleep(0.1)
        else:                                    # tới khi hết stdin / thoat
            while kh.kn.mo and not het_stdin.wait(0.1):
                pass
            time.sleep(0.3)                      # chờ trả lời của lệnh cuối
    except KeyboardInterrupt:
        pass
    kh.kn.khi_dong = lambda: None
    kh.kn.dong()


if __name__ == "__main__":
    main()
