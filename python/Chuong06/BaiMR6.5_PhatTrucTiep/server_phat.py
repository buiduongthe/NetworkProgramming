"""Bài mở rộng MR6.5 – Máy chủ trung chuyển phát trực tiếp 1→n (bản dòng lệnh).

Bài mở rộng, không có trong giáo trình; nối tiếp MR6.2 (âm thanh RTP), MR6.3 (video chia mảnh),
Bài 6.1 (server đa luồng), Bài 6.6 (một nguồn – nhiều người nhận). Lõi nằm ở may_chu.py (lớp
MayChuPhat, dùng chung với server_gui.py); tệp này chỉ in sự kiện và nhận lệnh bàn phím.

Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi server chạy):
  trang_thai      bo_dem bat|tat      ngat <tên>      thoat

Chạy:  python3 server_phat.py [host] [cổng_tcp] [cổng_udp]      (mặc định cổng_udp = cổng_tcp + 1)
"""
import sys
import threading
import time

from chung import doc_lenh_stdin
from may_chu import MayChuPhat, bang_tong_ket, bang_trang_thai


_khoa_in = threading.Lock()


def log(text):
    with _khoa_in:                                  # nhiều luồng cùng in: mỗi dòng in trọn vẹn
        print(f"[server] {text}", flush=True)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong_tcp = int(sys.argv[2]) if len(sys.argv) > 2 else 41650
    cong_udp = int(sys.argv[3]) if len(sys.argv) > 3 else cong_tcp + 1
    sv = MayChuPhat(bao=log)
    sv.bat_dau(host, cong_tcp, cong_udp)
    xong = threading.Event()

    def lenh(dong):
        p = dong.split()
        log(f"> {dong}")
        if p[0] == "trang_thai":
            for d in bang_trang_thai(sv.trang_thai()):
                log(d)
        elif p[0] == "bo_dem" and len(p) == 2 and p[1] in ("bat", "tat"):
            sv.dat_bo_dem(p[1] == "bat")
        elif p[0] == "ngat" and len(p) == 2:
            log(f"Đã ngắt {sv.ngat(ten=p[1])} người xem")
        elif p[0] == "thoat":
            xong.set()
        else:
            log("lệnh không hợp lệ")

    t0 = time.monotonic()
    threading.Thread(target=doc_lenh_stdin, args=(t0, lenh), daemon=True).start()
    try:
        while not xong.wait(0.5):
            pass
    except KeyboardInterrupt:                   # Ctrl+C (công cụ demo dừng server bằng SIGINT)
        pass
    for d in bang_tong_ket(sv.trang_thai()):
        log(d)
    sv.dung()


if __name__ == "__main__":
    main()
