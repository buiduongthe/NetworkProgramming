"""Bài mở rộng MR6.6 – Người tham gia đấu giá (bản dòng lệnh).

Kết nối máy chủ, in diễn biến phiên (giá mới, bị trả giá cao hơn, gia hạn, người thắng) và nhận lệnh:
  tra <giá>       trả đúng giá này                tang [n]    trả giá hiện tại + n bước giá (mặc định 1)
  thoat
Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).

Chạy:  python3 dau_gia.py <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
"""
import argparse
import threading
import time

from chung import doc_lenh_stdin, tien
from khach import KetNoiKhach
from may_chu import in_dong


class NguoiThamGia:
    def __init__(self, ten):
        self.ten = ten
        self.tt = None                  # cap_nhat mới nhất
        self.khoa = threading.Lock()
        self.kn = KetNoiKhach(khi_cap_nhat=self.cap_nhat, khi_tin=self.tin_khac, khi_dong=self.mat_ket_noi)

    def log(self, text):
        in_dong(self.ten, text)

    def ket_noi(self, host, cong):
        self.kn.ket_noi(host, cong, self.ten)
        tt = self.tt = self.kn.trang_thai
        con = "không giới hạn" if tt["con_lai_ms"] is None else f"còn {tt['con_lai_ms'] / 1000:.1f} s"
        self.log(f"Vào phòng: {tt['mon']['ten']} ({tt['che_do']})")
        self.log(f"  giá {tien(tt['gia'])}, bước {tien(tt['mon']['buoc_gia'])}, {con}")
        self.kn.bat_dau_doc()

    # ------------------------------------------------------------ nhận
    def cap_nhat(self, tt):
        with self.khoa:
            cu, self.tt = self.tt, tt
        if (tt["gia"], tt["nguoi_dan_dau"]) != (cu["gia"], cu["nguoi_dan_dau"]):
            self.log(f"Giá mới {tien(tt['gia'])} – {tt['nguoi_dan_dau']} dẫn đầu")
            if cu["nguoi_dan_dau"] == self.ten and tt["nguoi_dan_dau"] != self.ten:
                self.log("  Bạn bị trả giá cao hơn!")
        if tt["ket_thuc"] and not cu["ket_thuc"]:
            self.log("  BẠN THẮNG!" if tt["nguoi_dan_dau"] == self.ten else "  Bạn không thắng phiên này")

    def tin_khac(self, tin):
        if tin.get("loai") == "su_kien":
            self.log(f"* {tin['noi_dung']}")

    def mat_ket_noi(self):
        self.log("Mất kết nối máy chủ")

    # ------------------------------------------------------------ gửi
    def tra(self, gia, ghi_chu=""):
        self.log(f"> trả {tien(gia)}{ghi_chu}")

        def kq(tin, rtt):
            if tin.get("ok"):
                them = ", phiên được GIA HẠN" if tin.get("gia_han") else ""
                self.log(f"  CHẤP NHẬN ({rtt:.1f} ms){them}")
            else:
                self.log(f"  TỪ CHỐI: {tin.get('ly_do')}")
        self.kn.gui_yc("tra_gia", kq, gia=gia)

    def lenh(self, dong):
        p = dong.split()
        try:
            if p[0] == "tra" and len(p) == 2:
                self.tra(int(p[1]))
            elif p[0] == "tang" and len(p) <= 2:
                n = int(p[1]) if len(p) == 2 else 1
                with self.khoa:
                    gia = self.tt["gia"] + n * self.tt["mon"]["buoc_gia"]
                self.tra(gia, f" (+{n} bước)")
            elif p[0] == "thoat":
                self.kn.khi_dong = lambda: None
                self.kn.dong()
            else:
                self.log("lệnh: tra <giá> | tang [n] | thoat")
        except (ValueError, OSError) as e:
            self.log(f"lỗi: {e}")


def main():
    ap = argparse.ArgumentParser(description="Người tham gia đấu giá")
    ap.add_argument("ten")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41700)
    ap.add_argument("--thoi-gian", type=float, default=0.0)
    a = ap.parse_args()
    t0 = time.monotonic()
    ng = NguoiThamGia(a.ten)
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
