"""Bài mở rộng MR6.8 – Người đặt vé xem phim (bản dòng lệnh).

Kết nối máy chủ, in diễn biến (ghế mình giữ hết hạn, bị người khác lấy) và nhận lệnh:
  giu E7 [E8 …]   giữ chỗ (có thời hạn)          tt [E7 …]    thanh toán (mặc định: các ghế vừa giữ)
  nha [E7 …]      nhả ghế đang giữ               xem          in sơ đồ ghế          thoat
Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).

Chạy:  python3 dat_ve.py <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
"""
import argparse
import threading
import time

from chung import doc_lenh_stdin, tien
from khach import KetNoiKhach
from may_chu import in_dong


def so_do_chu(tt, ten):
    """Các dòng sơ đồ ghế: . trống, g người khác giữ, b đã bán, G mình giữ, V vé của mình."""
    ds = ["      [ MÀN HÌNH ]", "   " + " ".join(f"{c % 10}" for c in range(1, tt["cot"] + 1))]
    for r, dong in zip(tt["hang"], tt["so_do"]):
        o = []
        for c, x in enumerate(dong):
            cua_toi = tt["chu"].get(f"{r}{c + 1}") == ten
            o.append({"g": "G", "b": "V"}.get(x, x) if cua_toi else x)
        ds.append(f"{r}  " + " ".join(o))
    return ds


class NguoiDatVe:
    def __init__(self, ten):
        self.ten = ten
        self.tt = None                  # cap_nhat mới nhất
        self.gio_hang = []              # các ghế vừa giữ được (để "tt" không cần gõ lại)
        self.phien_gio = 0              # số thứ tự lần ghi (kq) của lần giữ chỗ gần nhất
        self.da_bao = set()             # ghế trong giỏ đã báo hết hạn / bị lấy
        self.khoa = threading.Lock()
        self.kn = KetNoiKhach(khi_cap_nhat=self.cap_nhat, khi_tin=self.tin_khac, khi_dong=self.mat_ket_noi)

    def log(self, text):
        in_dong(self.ten, text)

    def ket_noi(self, host, cong):
        self.kn.ket_noi(host, cong, self.ten)
        tt = self.tt = self.kn.trang_thai
        d = tt["dem"]
        self.log(f"Vào rạp: {tt['phim']}, suất {tt['suat']} ({tt['che_do']})")
        self.log(f"  trống {d['trong']}, giữ {d['giu']}, bán {d['ban']}; vé {tien(tt['gia_ve'])},"
                 f" giữ {tt['ttl_s']:g} s")
        self.kn.bat_dau_doc()

    # ------------------------------------------------------------ nhận
    def cap_nhat(self, tt):
        """Ghế trong giỏ không còn mang tên mình: hết hạn (trống) hoặc bị người khác lấy. Chỉ tin cap_nhat
        chụp SAU lần giữ của mình (tt["phien"] ≥ phien của kq). Giỏ vẫn giữ nguyên – lệnh "tt" sau đó sẽ bị
        máy chủ từ chối, đúng như khi trang thanh toán mở từ trước bấm "Thanh toán" quá hạn."""
        with self.khoa:
            self.tt = tt
            gio = [g for g in self.gio_hang if g not in self.da_bao]
            if tt["phien"] < self.phien_gio:
                return
        for g in gio:
            ai = tt["chu"].get(g)
            if ai == self.ten:
                continue
            if ai is None:
                self.log(f"  Hết thời gian giữ chỗ: {g} đã được nhả")
            else:
                self.log(f"  Ghế {g} đã bị {ai} lấy mất!")
            with self.khoa:
                self.da_bao.add(g)

    def tin_khac(self, tin):
        if tin.get("loai") == "su_kien":
            self.log(f"* {tin['noi_dung']}")

    def mat_ket_noi(self):
        self.log("Mất kết nối máy chủ")

    # ------------------------------------------------------------ gửi
    def giu(self, ghe):
        self.log(f"> giữ {', '.join(ghe)}")

        def kq(tin, rtt):
            if tin.get("ok"):
                with self.khoa:
                    self.gio_hang = list(tin["ghe"])
                    self.phien_gio, self.da_bao = tin.get("phien", 0), set()
                self.log(f"  ĐÃ GIỮ ({rtt:.1f} ms), thanh toán trong {tin['het_han_ms'] / 1000:.0f} s")
            else:
                self.log(f"  TỪ CHỐI: {tin.get('ly_do')}")
        self.kn.gui_yc("giu_cho", kq, ghe=ghe)

    def thanh_toan(self, ghe):
        self.log(f"> thanh toán {', '.join(ghe)}")

        def kq(tin, rtt):
            if tin.get("ok"):
                with self.khoa:
                    self.gio_hang = [g for g in self.gio_hang if g not in tin["ghe"]]
                self.log(f"  ĐẶT VÉ THÀNH CÔNG ({rtt:.1f} ms): {tien(tin['tien'])}")
                self.log(f"  mã vé {', '.join(tin['ma_ve'])}")
            else:
                self.log(f"  TỪ CHỐI: {tin.get('ly_do')}")
        self.kn.gui_yc("thanh_toan", kq, ghe=ghe)

    def nha(self, ghe):
        self.log(f"> nhả {', '.join(ghe)}")

        def kq(tin, rtt):
            if tin.get("ok"):
                with self.khoa:
                    self.gio_hang = [g for g in self.gio_hang if g not in tin["ghe"]]
                self.log(f"  Đã nhả {', '.join(tin['ghe'])}")
            else:
                self.log(f"  TỪ CHỐI: {tin.get('ly_do')}")
        self.kn.gui_yc("nha", kq, ghe=ghe)

    def lenh(self, dong):
        p = dong.split()
        try:
            with self.khoa:
                gio = list(self.gio_hang)
            if p[0] == "giu" and len(p) >= 2:
                self.giu([g.upper() for g in p[1:]])
            elif p[0] in ("tt", "nha"):
                ghe = [g.upper() for g in p[1:]] or gio
                if not ghe:
                    self.log("chưa giữ ghế nào")
                elif p[0] == "tt":
                    self.thanh_toan(ghe)
                else:
                    self.nha(ghe)
            elif p[0] == "xem":
                with self.khoa:
                    tt = self.tt
                for d in so_do_chu(tt, self.ten):
                    self.log(d)
            elif p[0] == "thoat":
                self.kn.khi_dong = lambda: None
                self.kn.dong()
            else:
                self.log("lệnh: giu E7 [E8 …] | tt [ghế…] | nha [ghế…] | xem | thoat")
        except (ValueError, OSError) as e:
            self.log(f"lỗi: {e}")


def main():
    ap = argparse.ArgumentParser(description="Người đặt vé xem phim")
    ap.add_argument("ten")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41720)
    ap.add_argument("--thoi-gian", type=float, default=0.0)
    a = ap.parse_args()
    t0 = time.monotonic()
    ng = NguoiDatVe(a.ten)
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
