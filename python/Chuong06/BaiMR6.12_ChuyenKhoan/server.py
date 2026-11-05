"""Bài mở rộng MR6.12 – Máy chủ ngân hàng (bản dòng lệnh).

Lõi ở may_chu.py (MayChuNganHang, khung MayChuCoSo trong chung.py, dùng chung với server_gui.py); tệp
này chỉ in nhật ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới) và nhận lệnh bàn phím.

Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chạy):
  che_do khong_khoa|khoa|lac_quan|hang_doi     tre <ms>     dat_lai [khoa=giá_trị …]     thong_ke   thoat
  (tham số đặt lại: so_tk (5–20), so_du_dau, khoa_ngay_tho (0 = khóa theo thứ tự mã, 1 = nguồn rồi đích))

Chạy:  python3 server.py [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-tk 10] [--so-du 5000000]
                         [--khoa-ngay-tho]
Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1).
"""
import argparse
import threading
import time

from chung import CHE_DO, doc_lenh_stdin, doc_tham_so, dong_thong_ke
from may_chu import MayChuNganHang, in_dong


def log(text):
    in_dong("server", text)


def main():
    ap = argparse.ArgumentParser(description="Máy chủ ngân hàng – tranh chấp tài nguyên")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41760)
    ap.add_argument("--che-do", choices=CHE_DO, default="khoa")
    ap.add_argument("--tre-ms", type=float, default=2.0)
    ap.add_argument("--so-tk", type=int, default=10, help="số tài khoản (5–20)")
    ap.add_argument("--so-du", type=int, default=5_000_000, help="số dư ban đầu mỗi tài khoản")
    ap.add_argument("--khoa-ngay-tho", action="store_true",
                    help="chế độ khoa: khóa nguồn rồi đích (có thể khóa chết)")
    a = ap.parse_args()
    sv = MayChuNganHang(a.che_do, a.tre_ms, bao=log, so_tk=a.so_tk, so_du_dau=a.so_du,
                        khoa_ngay_tho=int(a.khoa_ngay_tho))
    sv.bat_dau(a.host, a.cong)
    if a.khoa_ngay_tho:
        log("Khóa ngây thơ: nguồn rồi đích (có thể khóa chết)")
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
