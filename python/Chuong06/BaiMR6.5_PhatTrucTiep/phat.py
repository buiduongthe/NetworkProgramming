"""Bài mở rộng MR6.5 – Máy phát trực tiếp (bản dòng lệnh): camera + micro -> máy chủ.

Lõi ở may_phat.py (lớp MayPhat). Mặc định camera giả lập "bảng clapper" (đồng hồ, số khung, chớp
trắng mỗi giây tròn) và micro giả lập (440 Hz + bíp 1000 Hz mỗi giây tròn).
  --camera testsrc     ảnh mẫu thử của ffmpeg (cần ffmpeg)
  --camera <thiết bị>  camera thật qua ffmpeg (/dev/video0, "Tên camera" trên Windows, 0 trên macOS)
  --mic <chỉ số>       micro thật qua sounddevice
  --tep <tệp video>    phát lại hình + tiếng của tệp (mp4, mkv…) qua ffmpeg, lặp lại khi hết tệp;
                       thêm --mic gia_lap|<chỉ số> để thay tiếng của tệp bằng micro
  --tao-tep-mau        tạo trước tệp mẫu 10 s (chớp trắng + bíp mỗi giây) tại đường dẫn của --tep
Thiết bị thật không mở được -> tự lùi về nguồn giả lập (có báo).

Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t):  khung_khoa   dung

Chạy:  python3 phat.py <tên> <host> <cổng_tcp> [--camera gia_lap|testsrc|<thiết bị>]
           [--mic gia_lap|<chỉ số>] [--tep <tệp video> [--tao-tep-mau]] [--fps 10] [--gop 20]
           [--thoi-gian 30] [--mat-goi 0.0]
"""
import argparse
import os
import threading
import time

from camera import GIA_LAP, CameraFfmpeg, CameraGiaLap, co_ffmpeg
from chung import FPS_MAC_DINH, GOP_MAC_DINH, doc_lenh_stdin
from may_phat import MayPhat, dong_tong_ket
from tep_video import mo_tep, tao_tep_mau
from thiet_bi_am_thanh import mo_mic, nap_sounddevice


def main():
    ap = argparse.ArgumentParser(description="Máy phát trực tiếp")
    ap.add_argument("ten")
    ap.add_argument("host")
    ap.add_argument("cong", type=int)
    ap.add_argument("--camera", default=GIA_LAP)
    ap.add_argument("--mic", default=None, help="mặc định: gia_lap (hoặc tiếng của tệp khi có --tep)")
    ap.add_argument("--tep", default=None, help="tệp video phát lại (cần ffmpeg)")
    ap.add_argument("--tao-tep-mau", action="store_true", help="tạo tệp mẫu tại đường dẫn --tep trước khi phát")
    ap.add_argument("--fps", type=int, default=FPS_MAC_DINH)
    ap.add_argument("--gop", type=int, default=GOP_MAC_DINH)
    ap.add_argument("--thoi-gian", type=float, default=30.0)
    ap.add_argument("--mat-goi", type=float, default=0.0, help="giả lập tỉ lệ mất gói khi gửi")
    a = ap.parse_args()

    khoa_in = threading.Lock()

    def log(text):
        with khoa_in:                               # nhiều luồng cùng in: mỗi dòng in trọn vẹn
            print(f"[phat {a.ten}] {text}", flush=True)

    mp = MayPhat(a.ten, bao=log, mat_goi=a.mat_goi)
    try:
        ma = mp.ket_noi(a.host, a.cong)
    except OSError as e:
        log(f"Không kết nối được: {e}")
        return
    cam = mic = None
    if a.tep:
        try:
            if a.tao_tep_mau:
                os.makedirs(os.path.dirname(os.path.abspath(a.tep)), exist_ok=True)
                tao_tep_mau(a.tep, 10)
                log(f"Đã tạo tệp mẫu {os.path.basename(a.tep)} (10 s)")
            cam, mic = mo_tep(a.tep, a.fps, lay_tieng=a.mic is None)
        except OSError as e:
            log(f"Không mở được tệp video: {e}")
            cam = mic = None
    if cam is None:
        if a.camera != GIA_LAP and co_ffmpeg():
            cam = CameraFfmpeg(a.camera, a.camera, a.fps)
        else:
            if a.camera != GIA_LAP:
                log("Không có ffmpeg: dùng camera giả lập")
            cam = CameraGiaLap(a.ten, ma, a.fps)
    if mic is None:
        ten_mic = a.mic or "gia_lap"
        sd, _ = nap_sounddevice() if ten_mic != "gia_lap" else (None, "")
        mic, loi = mo_mic(sd, None if ten_mic == "gia_lap" or sd is None else int(ten_mic), ten_mic)
        if loi or (ten_mic != "gia_lap" and sd is None):
            log("Không mở được micro thật: dùng giả lập")
    log(f"Mã {ma}; camera {cam.mo_ta}")
    log(f"Micro {mic.mo_ta}")
    xong = threading.Event()

    def lenh(dong):
        log(f"> {dong}")
        if dong == "khung_khoa":
            mp.khung_khoa_ngay("lệnh khung_khoa")
        elif dong == "dung":
            xong.set()
        else:
            log("lệnh không hợp lệ")

    t0 = time.monotonic()
    threading.Thread(target=doc_lenh_stdin, args=(t0, lenh), daemon=True).start()
    mp.bat_dau_phat(cam, mic, a.fps, a.gop)
    log("Mỗi giây: khung I/P, kbit/s hình, tiếng (cả header)")
    giay, moc = 0, time.monotonic()
    try:
        while time.monotonic() - t0 < a.thoi_gian and not xong.is_set() and mp.da_ket_noi:
            if time.monotonic() - moc >= 1.0:
                moc += 1.0
                giay += 1
                r = mp.thong_ke_giay()
                log(f"{giay:2d}s khung I {r['i']} P {r['p']:2d} | hình {r['kbps_hinh']:4.0f}"
                    f" tiếng {r['kbps_am']:3.0f} kbit/s")
            time.sleep(0.02)
    except KeyboardInterrupt:
        pass
    tong = time.monotonic() - t0
    tt = mp.trang_thai()
    mp.dong()
    for d in dong_tong_ket(tt, tong):
        log(d)
    if a.mat_goi:
        log(f"giả lập mất {mp.bo_gia_lap} gói gửi")


if __name__ == "__main__":
    main()
