"""Bài mở rộng MR6.5 – Người xem phát trực tiếp (bản dòng lệnh).

Lõi ở nguoi_xem.py (lớp NguoiXem). Mỗi giây in: số gói tiếng nhận, tổng gói mất, trễ đầu-cuối TB,
số khung hiện và lệch tiếng-hình TB trong giây đó; cuối cùng in TỔNG KẾT. Không phát ra loa (bản
giao diện xem_gui.py mới phát loa nếu có sounddevice) – đồng hồ phát vẫn chạy đúng nhịp 20 ms.

Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy):
  chup <nhãn>   dong_bo bat|tat   dang_ky am_thanh|video|ca_hai   thoat

Chạy:  python3 xem.py <tên> <host> <cổng_tcp> [--thoi-gian 10] [--tre 0] [--dem 5] [--khong-dong-bo]
           [--chi-am-thanh] [--chi-video] [--mat-goi 0.0] [--ghep ket_qua_video.png]
  --thoi-gian  tổng thời gian chạy (giây, tính cả thời gian chờ --tre)
  --tre        chờ bao nhiêu giây rồi mới vào xem (người vào muộn)
  --mat-goi    giả lập tỉ lệ mất gói ở chiều nhận (cả tiếng và hình)
"""
import argparse
import os
import threading
import time

from chung import ghep_anh, ghi_png, doc_lenh_stdin
from nguoi_xem import NguoiXem, anh_chup, chu_ms, dong_tong_ket


def main():
    ap = argparse.ArgumentParser(description="Người xem phát trực tiếp")
    ap.add_argument("ten")
    ap.add_argument("host")
    ap.add_argument("cong", type=int)
    ap.add_argument("--thoi-gian", type=float, default=10.0)
    ap.add_argument("--tre", type=float, default=0.0)
    ap.add_argument("--dem", type=int, default=5, help="độ sâu bộ đệm jitter (khung 20 ms)")
    ap.add_argument("--khong-dong-bo", action="store_true")
    ap.add_argument("--chi-am-thanh", action="store_true")
    ap.add_argument("--chi-video", action="store_true")
    ap.add_argument("--mat-goi", type=float, default=0.0)
    ap.add_argument("--ghep", help="ghép các ảnh đã chụp thành tệp PNG này khi kết thúc")
    a = ap.parse_args()

    khoa_in = threading.Lock()

    def log(text):
        with khoa_in:                               # nhiều luồng cùng in: mỗi dòng in trọn vẹn
            print(f"[xem {a.ten}] {text}", flush=True)

    t0 = time.monotonic()
    nx = NguoiXem(a.ten, bao=log, do_sau=max(1, min(25, a.dem)), dong_bo=not a.khong_dong_bo,
                  am_thanh=not a.chi_video, video=not a.chi_am_thanh, mat_goi=a.mat_goi)
    anh_da_chup = []
    xong = threading.Event()

    def lenh(dong):
        p = dong.split()
        log(f"> {dong}")
        if p[0] == "dong_bo" and len(p) == 2 and p[1] in ("bat", "tat"):
            nx.dat_dong_bo(p[1] == "bat")
        elif p[0] == "dang_ky" and len(p) == 2 and p[1] in ("am_thanh", "video", "ca_hai"):
            nx.dang_ky(p[1] != "video", p[1] != "am_thanh")
        elif p[0] == "chup":
            c = nx.chup()
            if c is None:
                log("chưa có hình để chụp")
                return
            w, h, rgb = anh_chup(c)
            os.makedirs("out", exist_ok=True)
            duong = f"out/{a.ten}_chup_{len(anh_da_chup) + 1}.png"
            ghi_png(duong, rgb, w, h)
            anh_da_chup.append((w, h, rgb, " ".join(p[1:]) or duong))
            log(f"Đã chụp {duong}: hình {c[3] / 1000:.3f} s, tiếng "
                + ("–" if c[4] is None else f"{c[4] / 1000:.3f} s"))
        elif p[0] == "thoat":
            xong.set()
        else:
            log("lệnh không hợp lệ")

    threading.Thread(target=doc_lenh_stdin, args=(t0, lenh), daemon=True).start()
    if a.tre > 0:
        log(f"Chờ {a.tre:.1f} s rồi mới vào xem (vào muộn)")
        time.sleep(a.tre)
    try:
        nx.ket_noi(a.host, a.cong)
    except OSError as e:
        log(f"Không kết nối được: {e}")
        return
    nhan = "tiếng+hình" if nx.am_thanh and nx.video else ("tiếng" if nx.am_thanh else "hình")
    log(f"Nhận {nhan}, đồng bộ {'BẬT' if nx.dong_bo else 'TẮT'}, jitter {nx.do_sau * 20} ms"
        + (f", mất {a.mat_goi * 100:g}%" if a.mat_goi else ""))
    log("Mỗi giây: gói tiếng, mất, trễ ms | khung hiện, lệch ms")
    giay, moc = 0, time.monotonic()
    try:
        while time.monotonic() - t0 < a.thoi_gian and not xong.is_set() and nx.da_ket_noi:
            if time.monotonic() - moc >= 1.0:
                moc += 1.0
                giay += 1
                r = nx.thong_ke_giay()
                log(f"{giay:2d}s tiếng {r['am']:2d} mất {r['mat']} trễ {chu_ms(r['tre']):>3}"
                    f" | hình {r['hien']:2d} lệch {chu_ms(r['lech'], True):>4}")
            time.sleep(0.02)
    except KeyboardInterrupt:
        pass
    tt = nx.trang_thai()
    nx.dong()
    for d in dong_tong_ket(tt, a.tre):
        log(d)
    if a.mat_goi:
        log(f"giả lập mất {tt['bo_gia_lap']} gói nhận")
    if a.ghep and anh_da_chup:
        W, H = ghep_anh(a.ghep, anh_da_chup)
        log(f"Đã ghép {len(anh_da_chup)} ảnh vào {a.ghep} ({W}x{H})")


if __name__ == "__main__":
    main()
