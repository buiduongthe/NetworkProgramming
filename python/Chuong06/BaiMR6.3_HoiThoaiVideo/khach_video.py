"""Bài mở rộng MR6.3 – Client phòng hội thoại video.

Vòng gửi: theo tốc độ khung cho phép (min(fps camera, fps tối đa server đặt)), lấy một khung
từ camera (mặc định tổng hợp; --camera dùng camera thật nếu có `opencv-python`), giảm độ phân
giải theo yêu cầu của server, nén zlib, chia mảnh ≤ 1200 byte, gửi UDP.
Luồng nhận: ghép mảnh thành khung lưới do server gửi, giải nén, giữ khung mới nhất.
Mỗi giây in fps và kbit/s gửi/nhận, số mảnh mất. Lệnh `chup <nhãn>` lưu khung lưới đang xem
thành PNG (out/chup_N.png); khi kết thúc, --ghep tệp.png ghép mọi ảnh đã chụp kèm nhãn.

Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi client chạy):
  tat_camera   bat_camera   nhan_fps <n>   nhan_do_phan_giai <1|2>   chup <nhãn>   thoat

Chạy:  python3 khach_video.py <tên> <host> <cổng_tcp> [--fps 10] [--camera] [--thoi-gian 10]
           [--mat-goi 0.02] [--ghep ket_qua_video.png]
"""
import argparse
import json
import os
import random
import socket
import sys
import threading
import time
import zlib

from chung import (CAO, LOAI_MANH, RONG, CameraTongHop, GhepKhung, chia_manh, ghi_png, giai_nen,
                   goi_giu, gui_json, tach_header, thu_nho, ve_chu)


class CameraThat:
    """Camera thật qua thư viện ngoài OpenCV (tùy chọn)."""

    def __init__(self, cv2):
        self.cv2, self.cap = cv2, cv2.VideoCapture(0)
        if not self.cap.isOpened():
            raise OSError("không mở được camera 0")

    def lay_khung(self, so):
        ok, anh = self.cap.read()
        if not ok:
            return bytearray(RONG * CAO * 3)
        anh = self.cv2.resize(anh, (RONG, CAO))
        return bytearray(self.cv2.cvtColor(anh, self.cv2.COLOR_BGR2RGB).tobytes())


def ghep_anh(duong_dan, ds):
    """Ghép các ảnh chụp (rộng, cao, dữ liệu RGB, nhãn) thành một ảnh: 2 cột, nhãn dưới mỗi ảnh."""
    cot = 2
    o_w = max(w for w, _, _, _ in ds) + 16
    cao_hang = [max(h for _, h, _, _ in ds[i:i + cot]) + 34 for i in range(0, len(ds), cot)]
    W, H = cot * o_w + 16, sum(cao_hang) + 8
    anh = bytearray(b"\xff" * (W * H * 3))
    for n, (w, h, du_lieu, nhan) in enumerate(ds):
        x0 = 16 + (n % cot) * o_w + (o_w - 16 - w) // 2
        y0 = 8 + sum(cao_hang[:n // cot])
        for y in range(h):
            anh[((y0 + y) * W + x0) * 3:((y0 + y) * W + x0 + w) * 3] = du_lieu[y * w * 3:(y + 1) * w * 3]
        ve_chu(anh, W, H, 16 + (n % cot) * o_w, y0 + h + 6, nhan[:52], 1, (0, 0, 0))
    ghi_png(duong_dan, anh, W, H)
    return W, H


def main():
    ap = argparse.ArgumentParser(description="Client phòng hội thoại video")
    ap.add_argument("ten")
    ap.add_argument("host")
    ap.add_argument("cong", type=int)
    ap.add_argument("--fps", type=int, default=10, help="tốc độ khung của camera")
    ap.add_argument("--camera", action="store_true", help="dùng camera thật (cần opencv-python)")
    ap.add_argument("--thoi-gian", type=float, default=10.0)
    ap.add_argument("--mat-goi", type=float, default=0.0, help="giả lập tỉ lệ mất mảnh khi gửi")
    ap.add_argument("--ghep", help="ghép các ảnh đã chụp thành tệp PNG này khi kết thúc")
    a = ap.parse_args()
    tag = f"[khach {a.ten}]"

    def log(text):
        print(f"{tag} {text}", flush=True)

    # ---- Kênh điều khiển TCP
    tcp = socket.create_connection((a.host, a.cong), timeout=5)
    tcp.settimeout(None)
    tcp.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    f = tcp.makefile("r", encoding="utf-8")
    gui_json(tcp, {"loai": "vao", "ten": a.ten})
    chao = json.loads(f.readline())
    ma = chao["ma"]
    camera, mo_ta = CameraTongHop(a.ten, ma), "camera tổng hợp"
    if a.camera:
        try:
            import cv2                                       # thư viện ngoài, tùy chọn
            camera, mo_ta = CameraThat(cv2), "camera thật"
        except Exception as e:
            log(f"Không dùng được camera thật ({e}); dùng camera tổng hợp")
    log(f"Vào phòng, mã {ma}; {mo_ta} {RONG}x{CAO}, {a.fps} fps")

    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    if hasattr(socket, "SIO_UDP_CONNRESET"):   # Windows: tắt lỗi 10054 ở recvfrom (ICMP cổng đóng)
        udp.ioctl(socket.SIO_UDP_CONNRESET, False)
    udp.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1 << 20)
    udp.connect((a.host, chao["cong_udp"]))                  # chỉ nhận datagram từ server

    khoa = threading.Lock()
    tt = {"mo": False, "fps": a.fps, "chia": 1, "dung": False}
    ghep = GhepKhung()
    xem = {"anh": None}                                      # khung lưới mới nhất: (w, h, rgb)
    dem = {"khung_nhan": 0, "byte_nhan": 0}
    anh_chup = []

    def doc_tcp():
        try:
            for dong in f:
                tin = json.loads(dong)
                loai = tin.get("loai")
                if loai == "camera":
                    with khoa:
                        tt["mo"], tt["fps"], tt["chia"] = tin["mo"], tin["fps"], tin["chia"]
                    log(f"Camera: {tin['ly_do']}, gửi tối đa {tin['fps']} fps, 1/{tin['chia']}")
                elif loai == "nhan":
                    log(f"Nhận: {tin['fps']} fps, độ phân giải 1/{tin['chia']}")
                elif loai == "su_kien":
                    log(f"Sự kiện: {tin['noi_dung']}")
                elif loai == "tu_choi":
                    log(f"Bị từ chối: {tin['ly_do']}")
        except (OSError, ValueError):
            pass
        with khoa:
            tt["dung"] = True

    def nhan_udp():
        while True:
            try:
                goi = udp.recv(2048)
            except OSError:
                return
            h = tach_header(goi)
            if h is None or h[0] != LOAI_MANH:
                continue
            _, _, so, i, tong, w, cao, payload = h
            with khoa:
                khung = ghep.them(so, i, tong, w, cao, payload, time.monotonic())
            if khung:
                try:
                    anh = giai_nen(khung[2])
                except zlib.error:
                    continue
                with khoa:
                    xem["anh"] = (w, cao, anh)
                    dem["khung_nhan"] += 1
                    dem["byte_nhan"] += len(khung[2])

    def lenh(dong):
        p = dong.split()
        log(f"> {dong}")
        if p[0] == "tat_camera":                 # tự tắt: có hiệu lực ngay
            with khoa:
                tt["mo"] = False
            gui_json(tcp, {"loai": "tu_tat_camera"})
        elif p[0] == "bat_camera":               # tự bật: chờ server cho phép
            gui_json(tcp, {"loai": "tu_bat_camera"})
        elif p[0] == "nhan_fps" and len(p) == 2:
            gui_json(tcp, {"loai": "nhan", "fps": int(p[1])})
        elif p[0] == "nhan_do_phan_giai" and len(p) == 2:
            gui_json(tcp, {"loai": "nhan", "chia": int(p[1])})
        elif p[0] == "chup":
            with khoa:
                anh = xem["anh"]
            if anh is None:
                log("chưa có hình để chụp")
                return
            os.makedirs("out", exist_ok=True)
            duong = f"out/chup_{len(anh_chup) + 1}.png"
            ghi_png(duong, anh[2], anh[0], anh[1])
            nhan = " ".join(p[1:]) or duong
            anh_chup.append((anh[0], anh[1], anh[2], nhan))
            log(f"Đã chụp {duong} ({anh[0]}x{anh[1]})")
        elif p[0] == "thoat":
            with khoa:
                tt["dung"] = True
        else:
            log("lệnh không hợp lệ")

    def doc_lenh():
        for dong in sys.stdin:
            dong = dong.split("#")[0].strip()
            if dong.startswith("@"):
                moc_t, _, dong = dong.partition(" ")
                time.sleep(max(0.0, float(moc_t[1:]) - (time.monotonic() - t0)))
            if dong:
                lenh(dong)

    t0 = time.monotonic()
    for ham in (doc_tcp, nhan_udp, doc_lenh):
        threading.Thread(target=ham, daemon=True).start()

    # ---- Vòng gửi khung theo fps cho phép; mỗi giây in thống kê
    rng = random.Random(ma)
    udp.send(goi_giu(ma))                       # đăng ký địa chỉ UDP với server
    so_khung = gui_khung = gui_manh = bo_gia_lap = 0
    dem_gui = byte_gui = 0
    han_gui = moc_do = time.monotonic()
    han_giu = moc_do + 1.0
    giay = 0
    log("Mỗi giây: fps và kbit/s gửi | nhận, số mảnh mất")
    while time.monotonic() - t0 < a.thoi_gian:
        bay_gio = time.monotonic()
        with khoa:
            if tt["dung"]:
                break
            mo, fps, chia = tt["mo"], min(a.fps, tt["fps"]), tt["chia"]
        if mo and bay_gio >= han_gui:
            han_gui = max(han_gui + 1 / fps, bay_gio)
            anh = camera.lay_khung(so_khung)
            anh, w, h = thu_nho(anh, RONG, CAO, chia)          # giảm độ phân giải trước khi nén
            nen = zlib.compress(anh, 1)
            for g in chia_manh(ma, so_khung, w, h, nen):
                gui_manh += 1
                if rng.random() < a.mat_goi:
                    bo_gia_lap += 1                             # "mạng" làm rơi mảnh
                else:
                    udp.send(g)
            so_khung += 1
            gui_khung += 1
            dem_gui += 1
            byte_gui += len(nen)
        elif not mo and bay_gio >= han_giu:
            udp.send(goi_giu(ma))                               # giữ kết nối khi tắt camera
            han_giu = bay_gio + 1.0
        if bay_gio - moc_do >= 1.0:
            moc_do += 1.0
            giay += 1
            with khoa:
                kn, bn = dem["khung_nhan"], dem["byte_nhan"]
                dem["khung_nhan"] = dem["byte_nhan"] = 0
                mat = ghep.manh_mat
            log(f"{giay:2d}s gửi {dem_gui:2d} fps {byte_gui * 8 / 1000:4.0f} | nhận {kn:2d} fps"
                f" {bn * 8 / 1000:5.0f}, mất {mat}")
            dem_gui = byte_gui = 0
        time.sleep(0.002)

    # ---- Kết thúc
    tong_giay = time.monotonic() - t0
    tcp.close()
    udp.close()
    log(f"Kết thúc sau {tong_giay:.1f} s")
    log(f"Gửi {gui_khung} khung = {gui_manh} mảnh, giả lập mất {bo_gia_lap} mảnh")
    g = ghep
    log(f"Nhận {g.khung_du} khung đủ, hỏng {g.khung_hong}, mất hẳn {g.khung_mat_han},"
        f" mảnh mất {g.manh_mat}")
    if a.ghep and anh_chup:
        W, H = ghep_anh(a.ghep, anh_chup)
        log(f"Đã ghép {len(anh_chup)} ảnh chụp vào {a.ghep} ({W}x{H})")


if __name__ == "__main__":
    main()
