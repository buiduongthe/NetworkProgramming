"""Bài mở rộng MR6.10 – Máy chủ đăng ký học phần (bản dòng lệnh).

Lõi ở may_chu.py (MayChuDangKy, khung MayChuCoSo trong chung.py, dùng chung với server_gui.py); tệp
này chỉ in nhật ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới) và nhận lệnh bàn phím.

Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chạy):
  che_do khong_khoa|khoa|lac_quan|hang_doi     tre <ms>     dat_lai [khoa=giá_trị …]     thong_ke   thoat
  (tham số đặt lại: so_lop (1–6), si_so (1–500))

Chạy:  python3 server.py [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-lop 3] [--si-so 50]
Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1).
"""
import argparse
import threading
import time

from chung import CHE_DO, doc_lenh_stdin, doc_tham_so, dong_thong_ke
from may_chu import MayChuDangKy, in_dong


def log(text):
    in_dong("server", text)


def main():
    ap = argparse.ArgumentParser(description="Máy chủ đăng ký học phần – tranh chấp tài nguyên")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41740)
    ap.add_argument("--che-do", choices=CHE_DO, default="khoa")
    ap.add_argument("--tre-ms", type=float, default=2.0)
    ap.add_argument("--so-lop", type=int, default=3, help="số lớp học phần (1–6)")
    ap.add_argument("--si-so", type=int, default=50, help="sĩ số mỗi lớp")
    a = ap.parse_args()
    sv = MayChuDangKy(a.che_do, a.tre_ms, bao=log, so_lop=a.so_lop, si_so=a.si_so)
    sv.bat_dau(a.host, a.cong)
    xong = threading.Event()

    def lenh(dong):
        p = dong.split()
        log(f"> {dong}")
        try:
            if p[0] == "che_do" and len(p) == 2:
                sv.dat_che_do(p[1])
            elif p[0] == "tre" and len(p) == 2:
                sv.dat_tre(float(p[1]))
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
