"""Bài mở rộng MR6.3 – Server phòng hội thoại video nhiều client (ghép lưới kiểu MCU).

Bài mở rộng, không có trong giáo trình; nối tiếp MR6.2 (hội thoại âm thanh), Bài 6.1, 6.4
(server nhiều client) và Bài 4.1 (UDP). Hai kênh giống MR6.2:
  - TCP: thông điệp JSON điều khiển (vào phòng, bật/tắt camera, tham số gửi/nhận, sự kiện);
  - UDP: khung hình nén zlib, chia mảnh ≤ 1200 byte mỗi datagram.
Kiến trúc MCU (Multipoint Control Unit): server giải nén khung của mọi client, ghép thành một
lưới (mỗi ô 160×120), nén lại và gửi cho từng người nghe theo tốc độ khung/độ phân giải mà
người đó yêu cầu. (Cách khác là SFU – Selective Forwarding Unit: server chỉ chuyển tiếp luồng
của từng người, client tự ghép; xem README.)

Quy tắc quyền điều khiển (giống mic ở MR6.2):
  1. Camera của X mở khi và chỉ khi X không tự tắt VÀ server không tắt camera của X.
  2. Server tắt thắng client: đang bị server tắt thì client xin bật bị từ chối.
  3. Server bật = gỡ lệnh tắt của server, KHÔNG bật hộ camera mà người dùng tự tắt.
  4. Giới hạn gửi của từng client (fps tối đa, độ phân giải 1/1, 1/2, 1/4) và giới hạn fps chung
     do server đặt ("tương tự độ lợi và âm lượng chung" ở MR6.2).
  5. Tham số nhận (fps, độ phân giải lưới) do từng client tự chọn; fps nhận thực tế không vượt
     fps chung của server.
  6. Server tự thực thi: bỏ khung của camera đang tắt, bỏ khung đến nhanh hơn fps cho phép,
     bỏ datagram sai địa chỉ nguồn.

Lệnh bảng điều khiển (stdin; "@t lệnh" = chờ tới giây thứ t):
  tat_camera <tên|tat_ca>   bat_camera <tên|tat_ca>   fps_toi_da <tên|tat_ca> <n>
  do_phan_giai <tên|tat_ca> <1|2|4>   fps_chung <n>   trang_thai   thoat

Chạy:  python3 server_video.py [host] [cổng_tcp] [cổng_udp]
"""
import itertools
import json
import math
import socket
import sys
import threading
import time
import zlib

from chung import (CAO, LOAI_MANH, MA_SERVER, RONG, GhepKhung, chia_manh, giai_nen, gui_json,
                   phong_to, tach_header, thu_nho, to_mau, ve_chu)

FPS_MAC_DINH = 10
khoa = threading.Lock()
khach = {}                           # mã -> Khach
da_roi = []
ma_moi = itertools.count(1)
fps_chung = FPS_MAC_DINH             # giới hạn fps đầu ra chung cho mọi người nhận
t0 = time.monotonic()


def log(text):
    print(f"[server] {text}", flush=True)


class Khach:
    def __init__(self, ma, ten, conn, ip):
        self.ma, self.ten, self.conn, self.ip = ma, ten, conn, ip
        self.khoa_gui = threading.Lock()
        self.udp = None
        self.tu_tat = False              # client tự tắt camera
        self.server_tat = False          # server tắt camera
        self.fps_toi_da = FPS_MAC_DINH   # giới hạn gửi do server đặt
        self.chia = 1                    # độ phân giải gửi = 1/chia mỗi chiều
        self.nhan_fps = FPS_MAC_DINH     # tham số nhận do client chọn
        self.nhan_chia = 1
        self.ghep = GhepKhung()
        self.o = None                    # ô 160×120 đã phóng to, có nhãn – dùng khi ghép lưới
        self.luc_nhan_cuoi = 0.0
        self.bo_vuot_fps = self.bo_tat = self.sai_nguon = 0
        self.dem_khung = self.dem_byte = 0           # cửa sổ 1 giây để đo fps, kbit/s vào
        self.fps_vao = self.kbps_vao = 0.0
        self.so_khung_ra = 0                         # số khung lưới đã gửi cho client này
        self.han_gui_ra = 0.0
        self.dem_ra = self.byte_ra = 0
        self.fps_ra = self.kbps_ra = 0.0

    @property
    def cam_mo(self):                                # quy tắc 1
        return not self.tu_tat and not self.server_tat

    def ly_do(self):
        if self.cam_mo:
            return "mở"
        if self.tu_tat and self.server_tat:
            return "tắt (cả hai)"
        return "tắt (server)" if self.server_tat else "tắt (tự tắt)"

    def gui(self, obj):
        try:
            with self.khoa_gui:
                gui_json(self.conn, obj)
        except OSError:
            pass

    def gui_camera(self):
        """Báo cho client tham số gửi hiện hành."""
        self.gui({"loai": "camera", "mo": self.cam_mo, "ly_do": self.ly_do(),
                  "fps": self.fps_toi_da, "chia": self.chia})
        self.ve_o()

    def gui_nhan(self):
        self.gui({"loai": "nhan", "fps": min(self.nhan_fps, fps_chung), "chia": self.nhan_chia})

    def ve_o(self, khung=None):
        """Vẽ ô của client trong lưới: hình đã phóng về 160×120 + dải nhãn trạng thái."""
        if khung is not None:
            w, h, anh = khung
            o = bytearray(phong_to(anh, w, h, RONG // w))
        elif self.cam_mo and self.o is not None:
            o = self.o
        else:
            o = bytearray(bytes((40, 40, 40)) * (RONG * CAO))
            if not self.cam_mo:
                ve_chu(o, RONG, CAO, 20, 44, "TAT CAMERA", 2)
                ve_chu(o, RONG, CAO, 44, 66, "(SERVER)" if self.server_tat else "(TU TAT)", 2)
        to_mau(o, RONG, 0, CAO - 11, RONG, 11, (0, 0, 0))
        nhan = f"{self.ten} {self.fps_toi_da}FPS" + (f" 1/{self.chia}" if self.chia > 1 else "")
        ve_chu(o, RONG, CAO, 2, CAO - 9, nhan, 1, (255, 255, 0))
        self.o = o


def phat_su_kien(text):
    log(f"  => {text}")
    for k in list(khach.values()):
        k.gui({"loai": "su_kien", "noi_dung": text})


# ---------------------------------------------------------------- Kênh điều khiển TCP
def phuc_vu(conn, addr):
    f = conn.makefile("r", encoding="utf-8")
    k = None
    try:
        tin = json.loads(f.readline() or "{}")
        if tin.get("loai") != "vao" or not tin.get("ten"):
            return
        with khoa:
            k = Khach(next(ma_moi), str(tin["ten"])[:8], conn, addr[0])
            khach[k.ma] = k
        k.gui({"loai": "chao", "ma": k.ma, "cong_udp": cong_udp, "rong": RONG, "cao": CAO})
        log(f"#{k.ma} {k.ten} vào phòng từ {addr[0]}:{addr[1]}")
        with khoa:
            k.gui_camera()
            k.gui_nhan()
            phat_su_kien(f"{k.ten} vào phòng")
        for dong in f:
            tin = json.loads(dong)
            with khoa:
                xu_ly_tin_client(k, tin)
    except (OSError, ValueError):
        pass
    finally:
        if k:
            with khoa:
                khach.pop(k.ma, None)
                da_roi.append(k)
                log(f"#{k.ma} {k.ten} rời phòng")
                phat_su_kien(f"{k.ten} rời phòng")
        conn.close()


def xu_ly_tin_client(k, tin):
    loai = tin.get("loai")
    if loai == "tu_tat_camera":
        k.tu_tat = True
        k.gui_camera()
        phat_su_kien(f"{k.ten} tự tắt camera")
    elif loai == "tu_bat_camera":
        k.tu_tat = False
        if k.server_tat:                                          # quy tắc 2
            log(f"{k.ten} xin bật camera -> từ chối (server đang tắt)")
            k.gui({"loai": "tu_choi", "ly_do": "server đang tắt camera của bạn"})
        else:
            k.gui_camera()
            phat_su_kien(f"{k.ten} tự bật camera")
    elif loai == "nhan":                                          # quy tắc 5
        k.nhan_fps = max(1, min(30, int(tin.get("fps", k.nhan_fps))))
        k.nhan_chia = 2 if int(tin.get("chia", k.nhan_chia)) == 2 else 1
        log(f"{k.ten} chọn nhận {k.nhan_fps} fps, độ phân giải 1/{k.nhan_chia}")
        k.gui_nhan()


def nhan_ket_noi(srv):
    while True:
        conn, addr = srv.accept()
        conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        threading.Thread(target=phuc_vu, args=(conn, addr), daemon=True).start()


# ---------------------------------------------------------------- Kênh dữ liệu UDP
def nhan_udp(sock):
    while True:
        goi, addr = sock.recvfrom(2048)
        h = tach_header(goi)
        if h is None:
            continue
        loai, ma, so, i, tong, w, cao, payload = h
        bay_gio = time.monotonic()
        with khoa:
            k = khach.get(ma)
            if k is None:
                continue
            if k.udp is None and addr[0] == k.ip:
                k.udp = addr
                log(f"#{k.ma} {k.ten} đăng ký UDP {addr[0]}:{addr[1]}")
            if addr != k.udp:                                     # quy tắc 6
                k.sai_nguon += 1
                continue
            if loai != LOAI_MANH:
                continue
            khung = k.ghep.them(so, i, tong, w, cao, payload, bay_gio)
            if khung is None:
                continue
            if not k.cam_mo:                                      # quy tắc 6: camera tắt
                k.bo_tat += 1
                continue
            if bay_gio - k.luc_nhan_cuoi < 0.8 / k.fps_toi_da:    # quy tắc 6: vượt fps (dung sai 20%)
                k.bo_vuot_fps += 1
                continue
            try:
                anh = giai_nen(khung[2])
            except zlib.error:
                continue
            if w * cao * 3 != len(anh) or RONG % w or w * CAO != cao * RONG:
                continue                                          # kích thước không hợp lệ
            k.luc_nhan_cuoi = bay_gio
            k.dem_khung += 1
            k.dem_byte += len(khung[2])
            k.ve_o((w, cao, anh))


# ---------------------------------------------------------------- Ghép lưới và gửi
def ghep_luoi(ds):
    """Ghép ô của mọi client thành lưới c cột × r hàng (c = ⌈√n⌉)."""
    n = max(1, len(ds))
    cot = math.ceil(math.sqrt(n))
    hang = math.ceil(n / cot)
    trong = bytes((15, 15, 15)) * RONG
    ra = []
    for r in range(hang):
        o = [ds[r * cot + c].o if r * cot + c < len(ds) else None for c in range(cot)]
        for y in range(CAO):
            ra.append(b"".join(x[y * RONG * 3:(y + 1) * RONG * 3] if x is not None else trong
                               for x in o))
    return b"".join(ra), cot * RONG, hang * CAO


def vong_gui(sock):
    """Mỗi 5 ms kiểm tra: người nhận nào tới hạn (theo fps nhận) thì ghép lưới, thu nhỏ theo độ
    phân giải nhận, nén, chia mảnh và gửi. Mỗi giây tính lại fps, kbit/s."""
    moc_do = time.monotonic()
    while True:
        bay_gio = time.monotonic()
        with khoa:
            ds = sorted((k for k in khach.values() if k.o is not None), key=lambda k: k.ma)
            for k in khach.values():
                fps = min(k.nhan_fps, fps_chung)
                if k.udp is None or not ds or bay_gio < k.han_gui_ra:
                    continue
                k.han_gui_ra = max(k.han_gui_ra + 1 / fps, bay_gio)
                anh, w, h = ghep_luoi(ds)
                anh, w, h = thu_nho(anh, w, h, k.nhan_chia)
                nen = zlib.compress(anh, 1)
                for g in chia_manh(MA_SERVER, k.so_khung_ra, w, h, nen):
                    try:
                        sock.sendto(g, k.udp)
                    except OSError:
                        pass
                k.so_khung_ra += 1
                k.dem_ra += 1
                k.byte_ra += len(nen)
            if bay_gio - moc_do >= 1.0:
                for k in khach.values():
                    k.fps_vao, k.kbps_vao = k.dem_khung, k.dem_byte * 8 / 1000
                    k.fps_ra, k.kbps_ra = k.dem_ra, k.byte_ra * 8 / 1000
                    k.dem_khung = k.dem_byte = k.dem_ra = k.byte_ra = 0
                moc_do += 1.0
        time.sleep(0.005)


# ---------------------------------------------------------------- Bảng điều khiển
def chon(ten):
    return [k for k in khach.values() if ten == "tat_ca" or k.ten == ten]


def lenh_server(dong):
    global fps_chung
    p = dong.split()
    if not p:
        return True
    log(f"t={time.monotonic() - t0:.1f}s > {dong}")
    with khoa:
        ai = "tất cả" if len(p) > 1 and p[1] == "tat_ca" else (p[1] if len(p) > 1 else "")
        if p[0] in ("tat_camera", "bat_camera") and len(p) == 2:
            ds = chon(p[1])
            for k in ds:
                k.server_tat = p[0] == "tat_camera"
                k.gui_camera()
            if p[0] == "tat_camera":
                phat_su_kien(f"server tắt camera {ai}")
            else:
                van_tat = [k.ten for k in ds if k.tu_tat]          # quy tắc 3
                them = f" ({', '.join(van_tat)} vẫn tự tắt)" if van_tat else ""
                phat_su_kien(f"server bật camera {ai}{them}")
        elif p[0] == "fps_toi_da" and len(p) == 3:
            for k in chon(p[1]):
                k.fps_toi_da = max(1, min(30, int(p[2])))
                k.gui_camera()
            phat_su_kien(f"server giới hạn {ai} gửi tối đa {p[2]} fps")
        elif p[0] == "do_phan_giai" and len(p) == 3 and p[2] in ("1", "2", "4"):
            for k in chon(p[1]):
                k.chia = int(p[2])
                k.gui_camera()
            phat_su_kien(f"server đặt độ phân giải {ai} = 1/{p[2]}")
        elif p[0] == "fps_chung" and len(p) == 2:
            fps_chung = max(1, min(30, int(p[1])))
            for k in khach.values():
                k.gui_nhan()
            phat_su_kien(f"server đặt fps chung tối đa {fps_chung}")
        elif p[0] == "trang_thai":
            in_trang_thai()
        elif p[0] == "thoat":
            return False
        else:
            log("lệnh không hợp lệ")
    return True


def in_trang_thai():
    log(f"Trạng thái: {len(khach)} client, fps chung {fps_chung}")
    log("                     giới hạn vào        ra (lưới)")
    log(" #  tên camera       fps 1/n  fps  kbit  fps  kbit")
    for k in sorted(khach.values(), key=lambda k: k.ma):
        log(f"{k.ma:2d}  {k.ten:<3} {k.ly_do():<12} {k.fps_toi_da:3d} {k.chia:3d} {k.fps_vao:4.0f}"
            f" {k.kbps_vao:5.0f} {k.fps_ra:4.0f} {k.kbps_ra:5.0f}")


def in_tong_ket():
    log("Tổng kết luồng vào (từ client tới server):")
    log(" #  tên mảnh khung hỏng mảnh mất  bỏ:tắt vượt fps")
    log("        nhận    đủ        mất hẳn")
    for k in sorted(list(khach.values()) + da_roi, key=lambda k: k.ma):
        g = k.ghep
        log(f"{k.ma:2d}  {k.ten:<3} {g.manh_nhan:4d} {g.khung_du:5d} {g.khung_hong:4d} {g.manh_mat:4d}"
            f" {g.khung_mat_han:4d} {k.bo_tat:6d} {k.bo_vuot_fps:6d}")


def main():
    global cong_udp
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong_tcp = int(sys.argv[2]) if len(sys.argv) > 2 else 41630
    cong_udp = int(sys.argv[3]) if len(sys.argv) > 3 else 41631
    srv = socket.create_server((host, cong_tcp))
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    udp.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1 << 20)   # đệm nhận lớn: mảnh đến dồn dập
    udp.bind((host, cong_udp))
    log(f"TCP {host}:{cong_tcp} điều khiển; UDP :{cong_udp} video")
    log(f"Khung {RONG}x{CAO} RGB {RONG * CAO * 3} B, nén zlib, mảnh ≤ 1200 B")
    for ham, s in ((nhan_ket_noi, srv), (nhan_udp, udp), (vong_gui, udp)):
        threading.Thread(target=ham, args=(s,), daemon=True).start()
    try:
        for dong in sys.stdin:
            dong = dong.split("#")[0].strip()
            if dong.startswith("@"):
                moc, _, dong = dong.partition(" ")
                time.sleep(max(0.0, float(moc[1:]) - (time.monotonic() - t0)))
            if dong and not lenh_server(dong):
                break
        else:
            threading.Event().wait()
    except KeyboardInterrupt:
        pass
    log("Dừng server (Ctrl+C)")
    with khoa:
        in_tong_ket()


if __name__ == "__main__":
    main()
