"""Bài mở rộng MR6.2 – Server phòng hội thoại âm thanh nhiều client (trộn tiếng "mix-minus").

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1, 6.4 (server nhiều client, chat),
Bài 4.1 (UDP) và Bài 3.3 (định khung thông điệp). Hai kênh:
  - TCP (kênh điều khiển, tin cậy): mỗi dòng một thông điệp JSON – vào phòng, tự tắt/bật mic,
    báo âm lượng nghe; server gửi trạng thái mic, sự kiện, lời từ chối.
  - UDP (kênh dữ liệu, thời gian thực): gói RTP chứa khung PCM 20 ms. Mất một gói chỉ mất
    20 ms âm thanh; truyền lại như TCP sẽ đến quá muộn, vô ích với âm thanh trực tiếp.

Bộ trộn chạy mỗi 20 ms: lấy một khung từ bộ đệm jitter của từng client, nhân độ lợi đầu vào,
cộng tất cả (tổng T); mỗi người nghe k nhận T - (tiếng của chính k) – gọi là "mix-minus", để
không nghe tiếng vọng của mình – nhân âm lượng chung, cắt ngọn về phạm vi 16 bit rồi gửi đi.

Quy tắc quyền điều khiển:
  1. Mic của client X mở khi và chỉ khi X không tự tắt VÀ server không khóa mic của X.
  2. Server tắt (khóa) thắng client: khi đang bị khóa, client xin bật mic bị từ chối.
  3. Server bật = gỡ khóa, KHÔNG bật hộ mic mà người dùng đã tự tắt (quyền riêng tư: server có
     thể làm người dùng im lặng nhưng không tự ý mở micro của họ).
  4. Độ lợi đầu vào từng client và âm lượng chung do server đặt, áp dụng khi trộn.
  5. Âm lượng nghe do từng client tự đặt và áp dụng tại client; server không có lệnh đổi giá
     trị này (client chỉ báo để server hiển thị).
  6. Server tự thực thi, không tin client: gói âm thanh từ client đang tắt mic bị bỏ, gói UDP
     không khớp địa chỉ đã đăng ký bị bỏ.

Lệnh bảng điều khiển (stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ khi server chạy):
  tat_mic <tên|tat_ca>    bat_mic <tên|tat_ca>    do_loi <tên> <hệ số>
  am_luong_chung <hệ số>  trang_thai               thoat

Chạy:  python3 server_am_thanh.py [host] [cổng_tcp] [cổng_udp]
"""
import array
import itertools
import json
import math
import socket
import sys
import threading
import time

from chung import (BYTE_MOI_KHUNG, KHUNG_MS, MAU_MOI_KHUNG, PT_AM_THANH, SSRC_SERVER, BoDemJitter,
                   byte_sang_pcm, chu_db, dong_goi_rtp, gui_json, muc_dbfs, pcm_sang_byte, tach_rtp,
                   thanh_muc)

khoa = threading.Lock()              # bảo vệ mọi trạng thái dùng chung giữa các luồng
khach = {}                           # mã client (SSRC) -> Khach
da_roi = []                          # client đã rời phòng (giữ lại để in tổng kết)
ma_moi = itertools.count(1)
am_luong_chung = 1.0
t0 = time.monotonic()


def log(text):
    print(f"[server] {text}", flush=True)


class Khach:
    def __init__(self, ma, ten, conn, ip):
        self.ma, self.ten, self.conn, self.ip = ma, ten, conn, ip
        self.khoa_gui = threading.Lock()     # nhiều luồng có thể cùng gửi trên TCP
        self.udp = None                      # địa chỉ UDP học được từ gói đầu tiên
        self.tu_tat = False                  # client tự tắt mic
        self.server_khoa = False             # server khóa mic
        self.do_loi = 1.0                    # độ lợi đầu vào (gain)
        self.am_luong_nghe = 1.0             # client báo (chỉ để hiển thị)
        self.bo_dem = BoDemJitter(do_sau=3)
        self.seq_ra = self.ts_ra = 0         # số thứ tự, nhãn thời gian của luồng trộn gửi cho client
        self.muc_vao = -math.inf
        self.bi_chan = 0                     # gói bị bỏ vì mic đang tắt
        self.sai_nguon = 0                   # gói bị bỏ vì sai địa chỉ nguồn
        self.cat_ngon = 0                    # số mẫu bị cắt ngọn trong luồng gửi cho client này

    @property
    def mic_mo(self):                        # quy tắc 1
        return not self.tu_tat and not self.server_khoa

    def ly_do(self):
        if self.mic_mo:
            return "mở"
        if self.tu_tat and self.server_khoa:
            return "tắt (cả hai)"
        return "tắt (server)" if self.server_khoa else "tắt (tự tắt)"

    def gui(self, obj):
        try:
            with self.khoa_gui:
                gui_json(self.conn, obj)
        except OSError:
            pass

    def gui_mic(self):
        self.gui({"loai": "mic", "mo": self.mic_mo, "ly_do": self.ly_do()})
        if not self.mic_mo:
            self.bo_dem.xoa()                # bỏ phần âm thanh còn trong bộ đệm


def phat_su_kien(text):
    """Báo sự kiện cho mọi client (họ hiển thị và đánh mốc trong bản ghi)."""
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
        k.gui({"loai": "chao", "ma": k.ma, "cong_udp": cong_udp, "tan_so_mau": 16000,
               "khung_ms": KHUNG_MS})
        log(f"#{k.ma} {k.ten} vào phòng từ {addr[0]}:{addr[1]}")
        with khoa:
            k.gui_mic()
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
    if loai == "tu_tat_mic":
        k.tu_tat = True
        k.gui_mic()
        phat_su_kien(f"{k.ten} tự tắt mic")
    elif loai == "tu_bat_mic":
        k.tu_tat = False
        if k.server_khoa:                                        # quy tắc 2
            log(f"{k.ten} xin bật mic -> từ chối (server đang khóa)")
            k.gui({"loai": "tu_choi", "ly_do": "server đang khóa mic của bạn"})
        else:
            k.gui_mic()
            phat_su_kien(f"{k.ten} tự bật mic")
    elif loai == "am_luong_nghe":                                # quy tắc 5: chỉ ghi nhận
        k.am_luong_nghe = float(tin.get("gia_tri", 1.0))
        log(f"{k.ten} tự đặt âm lượng nghe {k.am_luong_nghe:.2f}")


def nhan_ket_noi(srv):
    while True:
        conn, addr = srv.accept()
        conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)   # tin điều khiển đi ngay
        threading.Thread(target=phuc_vu, args=(conn, addr), daemon=True).start()


# ---------------------------------------------------------------- Kênh dữ liệu UDP
def nhan_udp(sock):
    while True:
        goi, addr = sock.recvfrom(2048)
        r = tach_rtp(goi)
        if r is None:
            continue
        pt, seq, ts, ssrc, payload = r
        with khoa:
            k = khach.get(ssrc)
            if k is None:
                continue
            if k.udp is None and addr[0] == k.ip:                 # đăng ký địa chỉ UDP lần đầu
                k.udp = addr
                log(f"#{k.ma} {k.ten} đăng ký UDP {addr[0]}:{addr[1]}")
            if addr != k.udp:                                     # quy tắc 6: sai nguồn
                k.sai_nguon += 1
                continue
            if pt != PT_AM_THANH or len(payload) != BYTE_MOI_KHUNG:
                continue                                          # gói giữ kết nối hoặc sai dạng
            if not k.mic_mo:                                      # quy tắc 6: mic tắt
                k.bi_chan += 1
                continue
            k.bo_dem.nhan_goi(seq, ts, payload, time.monotonic())


def vong_tron(sock):
    """Mỗi 20 ms: trộn và gửi cho từng người nghe. Hẹn giờ tuyệt đối để không trôi nhịp."""
    ke_tiep = time.monotonic()
    while True:
        with khoa:
            ds = list(khach.values())
            # 1. Lấy một khung từ mỗi client đang mở mic, nhân độ lợi đầu vào, cộng vào tổng T
            vao = {}
            tong = [0.0] * MAU_MOI_KHUNG
            for k in ds:
                p = k.bo_dem.lay() if k.mic_mo else None
                if p is None:
                    k.muc_vao = -math.inf
                    continue
                mau = byte_sang_pcm(p)
                k.muc_vao = muc_dbfs(mau)
                g = k.do_loi
                vao[k.ma] = [g * x for x in mau]
                for i, x in enumerate(vao[k.ma]):
                    tong[i] += x
            # 2. Mỗi người nghe: T - tiếng của chính mình, nhân âm lượng chung, cắt ngọn
            for k in ds:
                if k.udp is None:
                    continue
                rieng = vao.get(k.ma)
                ra = array.array("h", bytes(2 * MAU_MOI_KHUNG))
                for i in range(MAU_MOI_KHUNG):
                    v = (tong[i] - (rieng[i] if rieng else 0.0)) * am_luong_chung
                    v = round(v)
                    if v > 32767 or v < -32768:
                        k.cat_ngon += 1
                        v = 32767 if v > 0 else -32768
                    ra[i] = v
                goi = dong_goi_rtp(PT_AM_THANH, k.seq_ra, k.ts_ra, SSRC_SERVER, pcm_sang_byte(ra))
                k.seq_ra += 1
                k.ts_ra += MAU_MOI_KHUNG
                try:
                    sock.sendto(goi, k.udp)
                except OSError:
                    pass
        ke_tiep += KHUNG_MS / 1000
        time.sleep(max(0.0, ke_tiep - time.monotonic()))


# ---------------------------------------------------------------- Bảng điều khiển
def chon(ten):
    """Danh sách client theo tên, hoặc tất cả nếu ten = tat_ca."""
    if ten == "tat_ca":
        return list(khach.values())
    return [k for k in khach.values() if k.ten == ten]


def lenh_server(dong):
    global am_luong_chung
    p = dong.split()
    if not p:
        return True
    log(f"t={time.monotonic() - t0:.1f}s > {dong}")
    with khoa:
        if p[0] in ("tat_mic", "bat_mic") and len(p) == 2:
            ds = chon(p[1])
            if not ds:
                log(f"không có client {p[1]}")
                return True
            for k in ds:
                k.server_khoa = p[0] == "tat_mic"
                k.gui_mic()
            ai = "tất cả" if p[1] == "tat_ca" else p[1]
            if p[0] == "tat_mic":
                phat_su_kien(f"server tắt mic {ai}")
            else:
                van_tat = [k.ten for k in ds if k.tu_tat]          # quy tắc 3
                them = f" ({', '.join(van_tat)} vẫn tự tắt)" if van_tat else ""
                phat_su_kien(f"server bật mic {ai}{them}")
        elif p[0] == "do_loi" and len(p) == 3:
            for k in chon(p[1]):
                k.do_loi = float(p[2])
            phat_su_kien(f"server đặt độ lợi {p[1]} = {float(p[2]):.2f}")
        elif p[0] == "am_luong_chung" and len(p) == 2:
            am_luong_chung = float(p[1])
            phat_su_kien(f"server đặt âm lượng chung {am_luong_chung:.2f}")
        elif p[0] == "trang_thai":
            in_trang_thai()
        elif p[0] == "thoat":
            return False
        else:
            log("lệnh không hợp lệ")
    return True


def in_trang_thai():
    log(f"Trạng thái: {len(khach)} client, âm lượng chung {am_luong_chung:.2f}")
    log(" #  tên mic          lợi   mức vào            mất  jit.")
    for k in khach.values():
        log(f"{k.ma:2d}  {k.ten:<3} {k.ly_do():<12} {k.do_loi:4.2f} {chu_db(k.muc_vao)} "
            f"{thanh_muc(k.muc_vao, 10)} {k.bo_dem.mat:3d} {k.bo_dem.jitter_ms:4.1f}ms")


def in_tong_ket():
    log("Tổng kết (gói vào: từ client tới server):")
    log(" #  tên gói vào mất trễ thiếu cạn bị chặn cắt ngọn")
    for k in sorted(list(khach.values()) + da_roi, key=lambda k: k.ma):
        b = k.bo_dem
        log(f"{k.ma:2d}  {k.ten:<3} {b.nhan:7d} {b.mat:3d} {b.tre:4d} {b.thieu:5d} {b.can:3d} "
            f"{k.bi_chan:7d} {k.cat_ngon:8d}")


def main():
    global cong_udp
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong_tcp = int(sys.argv[2]) if len(sys.argv) > 2 else 41620
    cong_udp = int(sys.argv[3]) if len(sys.argv) > 3 else 41621
    srv = socket.create_server((host, cong_tcp))
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    udp.bind((host, cong_udp))
    log(f"Điều khiển: TCP {host}:{cong_tcp}; âm thanh RTP: UDP {host}:{cong_udp}")
    log(f"PCM 16 bit mono 16 kHz; khung {KHUNG_MS} ms = {MAU_MOI_KHUNG} mẫu = {BYTE_MOI_KHUNG} B")
    for ham, ts in ((nhan_ket_noi, srv), (nhan_udp, udp), (vong_tron, udp)):
        threading.Thread(target=ham, args=(ts,), daemon=True).start()
    try:
        for dong in sys.stdin:                     # bảng điều khiển: đọc lệnh từ bàn phím/kịch bản
            dong = dong.split("#")[0].strip()
            if dong.startswith("@"):               # "@t lệnh": chờ tới giây thứ t
                moc, _, dong = dong.partition(" ")
                time.sleep(max(0.0, float(moc[1:]) - (time.monotonic() - t0)))
            if dong and not lenh_server(dong):
                break
        else:
            threading.Event().wait()               # hết stdin: tiếp tục phục vụ tới khi Ctrl+C
    except KeyboardInterrupt:
        pass
    log("Dừng server (Ctrl+C)")
    with khoa:
        in_tong_ket()


if __name__ == "__main__":
    main()
