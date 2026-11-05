"""Bài mở rộng MR5.4 – Luồng (thread) và đa luồng trong chương trình mạng (Python).

Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1–5.6) và Chương 6 (Bài 6.1 server đa
luồng, Bài 6.2). Khác MR5.2 (so sánh các mô hình trên một khối việc), bài này đi TỪNG BƯỚC vào chính luồng:
  (a) tạo – chạy – chờ luồng, truyền tham số, lấy kết quả          → vai trò  luong
  (b) server đa luồng: mỗi client một luồng / nhóm luồng + hàng đợi giới hạn (503) → server, tai
  (c) trạng thái dùng chung: tranh chấp, khóa, nguyên tử, bảng đồng thời   → khoa
  (d) nhà sản xuất – người tiêu dùng, hàng đợi chặn có giới hạn, poison pill → hang-doi
  (e) semaphore giới hạn số kết nối ra ngoài cùng lúc                → luong
  (f) bế tắc: phát hiện bằng chờ có hạn, sửa bằng thứ tự lấy khóa     → khoa
  (g) nhật ký theo luồng: tên luồng + ngữ cảnh trong biến cục bộ luồng (nhat_ky.py) – mọi vai trò

Chạy:  python3 luong.py dich-vu <host> <port>      # dịch vụ thử cho luong, hang-doi
       python3 luong.py server  <host> <port>      # hai máy chủ ở cổng port và port+1
       python3 luong.py luong   <host> <port_dịch_vụ>
       python3 luong.py hang-doi <host> <port_dịch_vụ>
       python3 luong.py tai     <host> <port_server>
       python3 luong.py khoa    <host> <port_trống>
"""
import socket
import sys
import threading
import time

import dich_vu
import hang_doi
import khoa
import may_chu
from nhat_ky import dat_ngu_canh, log, say

SO_SUAT, N_GOI_RA, MS_GOI_RA = 3, 8, 150


def goi_dich_vu(host, port, line):
    """Một lời gọi mạng kiểu chặn: kết nối, gửi một dòng, đọc một dòng trả lời."""
    with socket.create_connection((host, port), timeout=10) as s:
        s.sendall((line + "\n").encode())
        return s.makefile(encoding="utf-8").readline().strip()


# ---------------- (a) tạo, chạy, chờ luồng; tham số và kết quả ----------------
def co_ban(host, port):
    say("(a) Tạo – chạy – chờ luồng: 3 luồng, mỗi luồng nhận tham số (mã, thời gian xử lý) và trả kết quả")
    tham_so = [(1, 300), (2, 100), (3, 200)]
    ket_qua = [None] * len(tham_so)      # mỗi luồng ghi vào ô RIÊNG của mình → không cần khóa

    def tai(i, ma, ms):                  # hàm chạy trong luồng; tham số truyền qua args=
        dat_ngu_canh(f"yc={ma}")
        log(f"kết nối dịch vụ, gửi VIEC {ma} {ms}")
        t = time.perf_counter()
        tra_loi = goi_dich_vu(host, port, f"VIEC {ma} {ms}")
        dt = (time.perf_counter() - t) * 1000
        log(f"nhận {tra_loi} sau {dt:.0f} ms")
        ket_qua[i] = (tra_loi, dt)       # Thread không có giá trị trả về → ghi kết quả vào ô dùng riêng

    log("tạo 3 luồng tải-1..tải-3 rồi cho chạy (start)")
    t = time.perf_counter()
    ths = [threading.Thread(target=tai, args=(i, ma, ms), name=f"tải-{ma}")
           for i, (ma, ms) in enumerate(tham_so)]
    for th in ths:
        th.start()                       # luồng bắt đầu chạy song song với luồng chính
    for th in ths:
        th.join()                        # luồng chính chờ từng luồng kết thúc
    tong = time.perf_counter() - t
    tuan_tu = sum(ms for _, ms in tham_so) / 1000
    log(f"chờ (join) xong cả 3 luồng sau {tong:.2f} s (tuần tự sẽ ≈ {tuan_tu:.2f} s)")
    say()
    say(f"  {'Luồng':<8}{'Tham số':<16}{'Kết quả':<10}{'Thời gian':>10}")
    for (ma, ms), (tra_loi, dt) in zip(tham_so, ket_qua):
        say(f"  {'tải-' + str(ma):<8}{f'mã {ma}, {ms} ms':<16}{tra_loi:<10}{dt:>7.0f} ms")
    goi_dich_vu(host, port, "THONGKE")   # đặt lại thống kê của dịch vụ trước phần (e)


# ---------------- (e) semaphore giới hạn kết nối ra ngoài ----------------
def gioi_han(host, port):
    say()
    say(f"(e) Giới hạn kết nối ra ngoài: {N_GOI_RA} luồng gọi dịch vụ, tối đa {SO_SUAT} kết nối cùng lúc "
        "(semaphore)")
    suat = threading.BoundedSemaphore(SO_SUAT)    # bộ đếm SO_SUAT "suất"; hết suất thì acquire() chặn
    dang_dung = [0]
    khoa_dem = threading.Lock()

    def goi(k):
        dat_ngu_canh(f"yc={k}")
        if not suat.acquire(blocking=False):      # thử lấy suất không chờ để biết có phải chờ không
            log("hết suất → chờ")
            suat.acquire()
        with khoa_dem:
            dang_dung[0] += 1
            n = dang_dung[0]
        log(f"được suất (đang dùng {n}/{SO_SUAT}), gửi VIEC {10 + k} {MS_GOI_RA}")
        try:
            goi_dich_vu(host, port, f"VIEC {10 + k} {MS_GOI_RA}")
        finally:
            with khoa_dem:
                dang_dung[0] -= 1
            suat.release()                        # luôn trả suất, kể cả khi lời gọi lỗi
        log("xong, trả suất")

    t = time.perf_counter()
    ths = [threading.Thread(target=goi, args=(k,), name=f"gọi-{k}") for k in range(1, N_GOI_RA + 1)]
    for th in ths:
        th.start()
    for th in ths:
        th.join()
    dt = time.perf_counter() - t
    _, tong, toi_da = goi_dich_vu(host, port, "THONGKE").split()
    dot = -(-N_GOI_RA // SO_SUAT)
    log(f"dịch vụ xác nhận: {tong} yêu cầu, đồng thời tối đa {toi_da}; tổng {dt:.2f} s "
        f"(≈ ⌈{N_GOI_RA}/{SO_SUAT}⌉ × {MS_GOI_RA} ms = {dot * MS_GOI_RA / 1000:.2f} s)")


def run_luong(host, port):
    co_ban(host, int(port))
    gioi_han(host, int(port))
    say()
    say("(g) Nhật ký theo luồng: mỗi dòng ghi tên luồng; ngữ cảnh \"yc=\" lấy từ biến cục bộ luồng,")
    say("    nên hàm ghi nhật ký không cần nhận mã yêu cầu qua tham số.")
    say()
    gil = getattr(sys, "_is_gil_enabled", lambda: True)()
    say(f"Đặc thù Python: GIL đang {'bật' if gil else 'tắt'} – mỗi lúc chỉ một luồng chạy mã Python; lời gọi")
    say("  socket nhả GIL khi chờ, nên luồng hợp với việc chờ mạng; việc CPU thuần Python thì không chạy")
    say("  song song được (xem MR5.2, dùng ProcessPoolExecutor). threading.local là biến cục bộ luồng.")


def main():
    a = sys.argv
    threading.current_thread().name = "chính"
    vai = a[1] if len(a) > 1 else ""
    if len(a) < 4 or vai not in ("dich-vu", "server", "luong", "hang-doi", "tai", "khoa"):
        print("Cách dùng: python3 luong.py dich-vu|server|luong|hang-doi|tai|khoa <host> <port>")
        sys.exit(1)
    host, port = a[2], a[3]
    {"dich-vu": dich_vu.run, "server": may_chu.run_server, "luong": run_luong,
     "hang-doi": hang_doi.run, "tai": may_chu.run_tai, "khoa": khoa.run}[vai](host, port)


if __name__ == "__main__":
    main()
