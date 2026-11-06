"""Bài mở rộng MR6.14 – Người đặt vé xe khách (bản dòng lệnh).

Kết nối máy chủ, in diễn biến (chỗ mình giữ hết hạn, bị người khác lấy) và nhận lệnh:
  giu SG DL D01 [D02 …]   giữ chỗ đoạn Sài Gòn → Đà Lạt (mã trạm SG, BL, DL, NT; có thời hạn)
  tt                      thanh toán các chỗ vừa giữ          nha       nhả các chỗ vừa giữ
  xem                     in sơ đồ xe (mỗi chỗ 3 ký tự, một ký tự mỗi chặng)            thoat
Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).

Chạy:  python3 dat_ve_xe.py <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
"""
import argparse
import threading
import time

from chung import doc_lenh_stdin, tien
from khach import KetNoiKhach
from may_chu import MA_TRAM, in_dong, ten_doan


def so_do_chu(tt, ten):
    """Các dòng sơ đồ xe. Mỗi chỗ: mã rồi một ký tự mỗi chặng (. trống, g người khác giữ, b đã bán,
    G mình giữ, V vé của mình)."""
    o = {}
    for g, dong in zip(tt["ghe"], tt["so_do"]):
        o[g] = "".join(({"g": "G", "b": "V"}.get(x, x) if tt["chu"].get(f"{g}#{k}") == ten else x)
                       for k, x in enumerate(dong))
    ds = [f"  {tt['ten_xe']} – chặng: " + " | ".join(f"{a}-{b}" for a, b in zip(MA_TRAM, MA_TRAM[1:]))]
    so_tang = 1 + max(t for t, _, _ in tt["bo_tri"])
    for tang in range(so_tang):
        if so_tang > 1:
            ds.append(f"  Tầng {'dưới' if tang == 0 else 'trên'}:")
        hang = {}
        for g, (t, h, c) in zip(tt["ghe"], tt["bo_tri"]):
            if t == tang:
                hang.setdefault(h, {})[c] = f"{g}:{o[g]}"
        so_cot = 1 + max(c for _, _, c in tt["bo_tri"])
        rong = max(len(x) for d in hang.values() for x in d.values())
        for h in sorted(hang):
            ds.append("   " + " ".join(hang[h].get(c, "").ljust(rong) for c in range(so_cot)).rstrip())
    return ds


class NguoiDatVeXe:
    def __init__(self, ten):
        self.ten = ten
        self.tt = None                  # cap_nhat mới nhất
        self.gio_hang = None            # (ghế, tu, den) vừa giữ được (để "tt"/"nha" không cần gõ lại)
        self.phien_gio = 0              # số thứ tự lần ghi (kq) của lần giữ chỗ gần nhất
        self.da_bao = set()             # ô trong giỏ đã báo hết hạn / bị lấy
        self.khoa = threading.Lock()
        self.kn = KetNoiKhach(khi_cap_nhat=self.cap_nhat, khi_tin=self.tin_khac, khi_dong=self.mat_ket_noi)

    def log(self, text):
        in_dong(self.ten, text)

    def ket_noi(self, host, cong):
        self.kn.ket_noi(host, cong, self.ten)
        tt = self.tt = self.kn.trang_thai
        d = tt["dem"]
        self.log(f"Vào {tt['nha_xe']}: chuyến {tt['chuyen']}, {tt['ten_xe'].lower()} ({tt['che_do']})")
        self.log(f"  tuyến {' – '.join(tt['tram'])}; chỗ-chặng trống {d['trong']}, giữ {d['giu']},"
                 f" bán {d['ban']}; giữ chỗ {tt['ttl_s']:g} s")
        self.kn.bat_dau_doc()

    # ------------------------------------------------------------ nhận
    def cap_nhat(self, tt):
        """Ô trong giỏ không còn mang tên mình: hết hạn (trống) hoặc bị người khác lấy. Chỉ tin cap_nhat
        chụp SAU lần giữ của mình (tt["phien"] ≥ phien của kq)."""
        with self.khoa:
            self.tt = tt
            if not self.gio_hang or tt["phien"] < self.phien_gio:
                return
            ghe, tu, den = self.gio_hang
            o = [f"{g}#{k}" for g in ghe for k in range(tu, den) if f"{g}#{k}" not in self.da_bao]
        bao = {}
        for x in o:
            ai = tt["chu"].get(x)
            if ai != self.ten:
                bao.setdefault(x.split("#")[0], ai)
                with self.khoa:
                    self.da_bao.add(x)
        for g, ai in bao.items():
            self.log(f"  Hết thời gian giữ chỗ: {g} đã được nhả" if ai is None else f"  Chỗ {g} đã bị {ai} lấy mất!")

    def tin_khac(self, tin):
        if tin.get("loai") == "su_kien":
            self.log(f"* {tin['noi_dung']}")

    def mat_ket_noi(self):
        self.log("Mất kết nối máy chủ")

    # ------------------------------------------------------------ gửi
    def giu(self, ghe, tu, den):
        self.log(f"> giữ {', '.join(ghe)} {ten_doan(tu, den)}")

        def kq(tin, rtt):
            if tin.get("ok"):
                with self.khoa:
                    self.gio_hang = (list(tin["ghe"]), tu, den)
                    self.phien_gio, self.da_bao = tin.get("phien", 0), set()
                self.log(f"  ĐÃ GIỮ ({rtt:.1f} ms), {tien(tin['tien'])}, thanh toán trong"
                         f" {tin['het_han_ms'] / 1000:.0f} s")
            else:
                self.log(f"  TỪ CHỐI: {tin.get('ly_do')}")
        self.kn.gui_yc("giu_cho", kq, ghe=ghe, tu=tu, den=den)

    def thanh_toan(self, ghe, tu, den):
        self.log(f"> thanh toán {', '.join(ghe)} {ten_doan(tu, den)}")

        def kq(tin, rtt):
            if tin.get("ok"):
                with self.khoa:
                    self.gio_hang = None
                self.log(f"  ĐẶT VÉ THÀNH CÔNG ({rtt:.1f} ms): {tien(tin['tien'])}")
                self.log(f"  mã vé {', '.join(tin['ma_ve'])}")
            else:
                self.log(f"  TỪ CHỐI: {tin.get('ly_do')}")
        self.kn.gui_yc("thanh_toan", kq, ghe=ghe, tu=tu, den=den)

    def nha(self, ghe, tu, den):
        self.log(f"> nhả {', '.join(ghe)} {ten_doan(tu, den)}")

        def kq(tin, rtt):
            if tin.get("ok"):
                with self.khoa:
                    self.gio_hang = None
                self.log(f"  Đã nhả {', '.join(tin['ghe'])}")
            else:
                self.log(f"  TỪ CHỐI: {tin.get('ly_do')}")
        self.kn.gui_yc("nha", kq, ghe=ghe, tu=tu, den=den)

    def lenh(self, dong):
        p = dong.split()
        try:
            with self.khoa:
                gio = self.gio_hang
            if p[0] == "giu" and len(p) >= 4 and p[1].upper() in MA_TRAM and p[2].upper() in MA_TRAM:
                self.giu([g.upper() for g in p[3:]], MA_TRAM.index(p[1].upper()), MA_TRAM.index(p[2].upper()))
            elif p[0] in ("tt", "nha"):
                if not gio:
                    self.log("chưa giữ chỗ nào")
                elif p[0] == "tt":
                    self.thanh_toan(*gio)
                else:
                    self.nha(*gio)
            elif p[0] == "xem":
                with self.khoa:
                    tt = self.tt
                for d in so_do_chu(tt, self.ten):
                    self.log(d)
            elif p[0] == "thoat":
                self.kn.khi_dong = lambda: None
                self.kn.dong()
            else:
                self.log("lệnh: giu SG DL D01 [D02 …] | tt | nha | xem | thoat   (trạm: " + ", ".join(MA_TRAM) + ")")
        except (ValueError, OSError) as e:
            self.log(f"lỗi: {e}")


def main():
    ap = argparse.ArgumentParser(description="Người đặt vé xe khách theo chặng")
    ap.add_argument("ten")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41780)
    ap.add_argument("--thoi-gian", type=float, default=0.0)
    a = ap.parse_args()
    t0 = time.monotonic()
    ng = NguoiDatVeXe(a.ten)
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
