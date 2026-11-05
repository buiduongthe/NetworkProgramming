"""Bài mở rộng MR6.9 – Người tham gia vòng quay may mắn (bản dòng lệnh).

Kết nối máy chủ, in diễn biến (ai trúng gì, số quà còn lại) và nhận lệnh:
  quay        quay một lượt
  quay x2     "bấm đúp": gửi HAI yêu cầu quay liên tiếp không chờ (hai ma_yc khác nhau) – máy chủ phải
              chấp nhận đúng một (idempotency theo tên người)
  gui_lai [k] gửi lại ĐÚNG ma_yc k (mặc định: của lượt quay gần nhất), như khi mạng chập chờn client
              gửi lại – máy chủ không quay lại mà trả lại kết quả cũ kèm "lap_lai": true
              (idempotency theo ma_yc)
  thoat
Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).

Chạy:  python3 quay_so.py <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
"""
import argparse
import threading
import time

from chung import doc_lenh_stdin, gui_json
from khach import KetNoiKhach
from may_chu import chu_p, in_dong


def mo_ta_kq(tin):
    if tin.get("ok"):
        return f"TRÚNG {tin['ten_qua']}!" if tin.get("trung") else "Chúc may mắn lần sau"
    return f"TỪ CHỐI: {tin.get('ly_do')}"


class NguoiQuay:
    def __init__(self, ten):
        self.ten = ten
        self.tt = None
        self.ma_cuoi = None                 # ma_yc của lượt quay gần nhất (cho gui_lai)
        self.kn = KetNoiKhach(khi_cap_nhat=self.cap_nhat, khi_tin=self.tin_khac, khi_dong=self.mat_ket_noi)

    def log(self, text):
        in_dong(self.ten, text)

    def ket_noi(self, host, cong):
        self.kn.ket_noi(host, cong, self.ten)
        tt = self.tt = self.kn.trang_thai
        self.log(f"Vào vòng quay ({tt['che_do']}): còn {tt['con_lai']}/{tt['so_qua']} quà,"
                 f" p {chu_p(tt['p'])}")
        self.kn.bat_dau_doc()

    # ------------------------------------------------------------ nhận
    def cap_nhat(self, tt):
        cu, self.tt = self.tt, tt
        if tt["con_lai"] != cu["con_lai"]:
            self.log(f"  còn {tt['con_lai']}/{tt['so_qua']} quà")

    def tin_khac(self, tin):
        if tin.get("loai") == "su_kien":
            self.log(f"* {tin['noi_dung']}")
        elif tin.get("loai") == "kq" and tin.get("lap_lai"):
            self.log(f"  (ma_yc {tin['ma_yc']}, lap_lai) {mo_ta_kq(tin)}")

    def mat_ket_noi(self):
        self.log("Mất kết nối máy chủ")

    # ------------------------------------------------------------ gửi
    def quay(self):
        def kq(tin, rtt):
            self.log(f"  ma_yc {tin['ma_yc']}: {mo_ta_kq(tin)} ({rtt:.1f} ms)")
        self.ma_cuoi = self.kn.gui_yc("quay", kq)

    def gui_lai(self, ma=None):
        ma = ma or self.ma_cuoi
        if ma is None:
            self.log("chưa quay lượt nào")
            return
        self.log(f"> gửi lại ma_yc {ma}")
        gui_json(self.kn.sock, {"loai": "quay", "ma_yc": ma})

    def lenh(self, dong):
        p = dong.split()
        try:
            if p == ["quay"]:
                self.log("> quay")
                self.quay()
            elif p == ["quay", "x2"]:
                self.log("> quay x2 (bấm đúp: 2 yêu cầu liền nhau)")
                self.quay()
                self.quay()
            elif p[0] == "gui_lai" and len(p) <= 2:
                self.gui_lai(int(p[1]) if len(p) == 2 else None)
            elif p[0] == "thoat":
                self.kn.khi_dong = lambda: None
                self.kn.dong()
            else:
                self.log("lệnh: quay | quay x2 | gui_lai [k] | thoat")
        except (ValueError, OSError) as e:
            self.log(f"lỗi: {e}")


def main():
    ap = argparse.ArgumentParser(description="Người tham gia vòng quay may mắn")
    ap.add_argument("ten")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41730)
    ap.add_argument("--thoi-gian", type=float, default=0.0)
    a = ap.parse_args()
    t0 = time.monotonic()
    ng = NguoiQuay(a.ten)
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
