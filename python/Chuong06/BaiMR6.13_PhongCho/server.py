"""Bài mở rộng MR6.13 – Máy chủ bán vé có phòng chờ ảo (bản dòng lệnh).

Lõi ở may_chu.py (MayChuPhongCho, khung MayChuCoSo trong chung.py, dùng chung với server_gui.py); tệp
này chỉ in nhật ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới) và nhận lệnh bàn phím.

Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chạy):
  che_do khong_khoa|khoa|lac_quan|hang_doi     tre <ms>     dat_lai [khoa=giá_trị …]     thong_ke   thoat
  toc_do <người/s>  (đổi tốc độ cấp lượt ngay, không đặt lại)
  (tham số đặt lại: tong_ve, ve_toi_da, gia_ve, ten_su_kien, phong_cho (0/1), toc_do, dung_luong,
   luot_s, gioi_han_xu_ly)

Chạy:  python3 server.py [host] [cổng] [--che-do khoa] [--tre-ms 2] [--tong-ve 500] [--toc-do 50]
                         [--dung-luong 50] [--luot-s 20] [--gioi-han 64] [--tat-phong-cho]
Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1).
"""
import argparse
import threading
import time

from chung import CHE_DO, doc_lenh_stdin, doc_tham_so, dong_thong_ke
from may_chu import MayChuPhongCho, in_dong


def log(text):
    in_dong("server", text)


def main():
    ap = argparse.ArgumentParser(description="Máy chủ bán vé có phòng chờ ảo – tranh chấp tài nguyên")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41770)
    ap.add_argument("--che-do", choices=CHE_DO, default="khoa")
    ap.add_argument("--tre-ms", type=float, default=2.0)
    ap.add_argument("--tong-ve", type=int, default=500)
    ap.add_argument("--toc-do", type=float, default=50.0, help="tốc độ cấp lượt (người/s)")
    ap.add_argument("--dung-luong", type=int, default=50, help="dung lượng thùng token")
    ap.add_argument("--luot-s", type=float, default=20.0, help="thời hạn một lượt mua (s)")
    ap.add_argument("--gioi-han", type=int, default=64, help="số yêu cầu mua đang xử lý tối đa")
    ap.add_argument("--tat-phong-cho", action="store_true", help="tắt phòng chờ: mọi người mua trực tiếp")
    a = ap.parse_args()
    sv = MayChuPhongCho(a.che_do, a.tre_ms, bao=log, tong_ve=a.tong_ve, toc_do=a.toc_do,
                        dung_luong=a.dung_luong, luot_s=a.luot_s, gioi_han_xu_ly=a.gioi_han,
                        phong_cho=0 if a.tat_phong_cho else 1)
    sv.bat_dau(a.host, a.cong)
    log(f"Phòng chờ {'TẮT' if a.tat_phong_cho else 'BẬT'}: {a.tong_ve} vé, cấp lượt {a.toc_do:g} người/s")
    xong = threading.Event()

    def lenh(dong):
        p = dong.split()
        log(f"> {dong}")
        try:
            if p[0] == "che_do" and len(p) == 2:
                sv.dat_che_do(p[1])
            elif p[0] == "tre" and len(p) == 2:
                sv.dat_tre(float(p[1]))
            elif p[0] == "toc_do" and len(p) == 2:
                sv.dat_toc_do(float(p[1]))
            elif p[0] == "dat_lai":
                sv.dat_lai(**doc_tham_so(p[1:]))
            elif p[0] == "thong_ke":
                for d in dong_thong_ke(sv.thong_ke_day_du()):
                    log(d)
            elif p[0] == "thoat":
                xong.set()
            else:
                log("lệnh không hợp lệ")
        except ValueError as e:
            log(f"lỗi: {e}")

    t0 = time.monotonic()
    threading.Thread(target=doc_lenh_stdin, args=(t0, lenh), daemon=True).start()
    da_in = 0
    try:
        while not xong.wait(5.0):                   # mỗi 5 s: một dòng số liệu nếu có yêu cầu mới
            tk = sv.thong_ke_day_du()
            if tk["so_yc"] != da_in:
                da_in = tk["so_yc"]
                log(f"{tk['che_do']}: {tk['so_yc']} yc, tc {tk['thanh_cong']},"
                    f" từ chối {tk['tu_choi']}, sai phạm {tk['sai_pham_tong']}")
    except KeyboardInterrupt:                       # Ctrl+C (công cụ demo dừng server bằng SIGINT)
        pass
    log("Tổng kết (từ lần đặt lại cuối):")
    for d in dong_thong_ke(sv.thong_ke_day_du()):
        log("  " + d)
    sv.dung()


if __name__ == "__main__":
    main()
