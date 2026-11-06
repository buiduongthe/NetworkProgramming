"""Bài mở rộng MR6.2 – Client phòng hội thoại âm thanh.

Mỗi 20 ms client làm hai việc trong cùng một vòng lặp có nhịp cố định:
  1. Thu: lấy 320 mẫu từ nguồn âm (mặc định sóng sin tổng hợp – mỗi client một tần số khác nhau
     để phân tích được ai đang nói; hoặc tệp WAV; hoặc micro thật với --mic nếu có `sounddevice`),
     đóng gói RTP, gửi UDP tới server nếu mic đang mở.
  2. Phát: lấy một khung từ bộ đệm jitter (luồng trộn server gửi về), nhân âm lượng nghe của chính
     client, ghi vào bản ghi (--ghi tệp.wav), phát ra loa nếu có --loa.
Mỗi giây in thanh đo mức (dBFS). Sự kiện từ server được đánh mốc theo vị trí trong bản ghi
(tệp <tên>_moc.json) để phan_tich_wav.py chia đoạn và chứng minh lệnh điều khiển có hiệu lực.

Lệnh (stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ khi client chạy):
  tat_mic   bat_mic   am_luong <hệ số>   thoat

Chạy:  python3 khach_am_thanh.py <tên> <host> <cổng_tcp> [--tan-so 440] [--wav tệp.wav] [--mic]
           [--loa] [--ghi out/ghi.wav] [--thoi-gian 10] [--mat-goi 0.05]
"""
import argparse
import array
import json
import math
import os
import queue
import random
import socket
import sys
import threading
import time

from chung import (KHUNG_MS, MAU_MOI_KHUNG, PT_AM_THANH, PT_GIU_KET_NOI, TAN_SO_MAU, BoDemJitter,
                   byte_sang_pcm, cat_ngon, chu_db, doc_wav, dong_goi_rtp, ghi_wav, gui_json,
                   muc_dbfs, pcm_sang_byte, tach_rtp, thanh_muc)


class NguonSin:
    """Âm tổng hợp: sin tần số f, biên độ tương đối (0..1). Pha liên tục giữa các khung."""

    def __init__(self, f, bien_do):
        self.f, self.a, self.n = f, bien_do * 32767, 0

    def lay_khung(self):
        w = 2 * math.pi * self.f / TAN_SO_MAU
        k = [round(self.a * math.sin(w * (self.n + i))) for i in range(MAU_MOI_KHUNG)]
        self.n += MAU_MOI_KHUNG
        return k


class NguonWav:
    """Phát lặp một tệp WAV 16 bit mono 16 kHz."""

    def __init__(self, duong_dan):
        self.mau, self.vt = doc_wav(duong_dan), 0

    def lay_khung(self):
        k = [self.mau[(self.vt + i) % len(self.mau)] for i in range(MAU_MOI_KHUNG)]
        self.vt += MAU_MOI_KHUNG
        return k


class ThietBiThat:
    """Micro/loa thật qua thư viện ngoài `sounddevice` (tùy chọn, cần PortAudio)."""

    def __init__(self, sd, mic, loa):
        self.vao, self.ra = queue.Queue(maxsize=10), queue.Queue(maxsize=10)
        self.luong = []
        if mic:
            def thu(indata, frames, t, status):
                try:
                    self.vao.put_nowait(bytes(indata))
                except queue.Full:
                    pass
            self.luong.append(sd.RawInputStream(samplerate=TAN_SO_MAU, channels=1, dtype="int16",
                                                blocksize=MAU_MOI_KHUNG, callback=thu))
        if loa:
            def phat(outdata, frames, t, status):
                try:
                    outdata[:] = self.ra.get_nowait()
                except queue.Empty:
                    outdata[:] = bytes(len(outdata))          # thiếu dữ liệu: phát im lặng
            self.luong.append(sd.RawOutputStream(samplerate=TAN_SO_MAU, channels=1, dtype="int16",
                                                 blocksize=MAU_MOI_KHUNG, callback=phat))
        for s in self.luong:
            s.start()

    def lay_khung(self):
        try:
            a = array.array("h")
            a.frombytes(self.vao.get_nowait())
            return list(a)
        except queue.Empty:
            return [0] * MAU_MOI_KHUNG

    def phat(self, mau):
        try:
            self.ra.put_nowait(array.array("h", mau).tobytes())
        except queue.Full:
            pass


def main():
    ap = argparse.ArgumentParser(description="Client phòng hội thoại âm thanh")
    ap.add_argument("ten")
    ap.add_argument("host")
    ap.add_argument("cong", type=int)
    ap.add_argument("--tan-so", type=float, default=440.0, help="tần số âm tổng hợp (Hz)")
    ap.add_argument("--bien-do", type=float, default=0.25, help="biên độ tương đối 0..1")
    ap.add_argument("--wav", help="dùng tệp WAV làm nguồn âm")
    ap.add_argument("--mic", action="store_true", help="dùng micro thật (cần sounddevice)")
    ap.add_argument("--loa", action="store_true", help="phát ra loa thật (cần sounddevice)")
    ap.add_argument("--ghi", help="ghi những gì nghe được vào tệp WAV")
    ap.add_argument("--thoi-gian", type=float, default=10.0, help="số giây chạy")
    ap.add_argument("--mat-goi", type=float, default=0.0, help="giả lập tỉ lệ mất gói khi gửi")
    ap.add_argument("--dem", type=int, default=3, help="độ sâu bộ đệm jitter (khung)")
    a = ap.parse_args()
    tag = f"[khach {a.ten}]"

    def log(text):
        print(f"{tag} {text}", flush=True)

    # ---- Nguồn âm và thiết bị
    nguon, thiet_bi = NguonSin(a.tan_so, a.bien_do), None
    mo_ta = f"sin {a.tan_so:g} Hz, biên độ {a.bien_do:g}"
    if a.wav:
        nguon, mo_ta = NguonWav(a.wav), f"tệp {a.wav}"
    if a.mic or a.loa:
        try:
            import sounddevice as sd                       # thư viện ngoài, tùy chọn
            thiet_bi = ThietBiThat(sd, a.mic, a.loa)
            if a.mic:
                nguon, mo_ta = thiet_bi, "micro thật"
        except Exception as e:                             # không cài hoặc thiếu PortAudio
            log(f"Không dùng được sounddevice ({e}); dùng nguồn tổng hợp/WAV, không phát loa")
            thiet_bi = None

    # ---- Kênh điều khiển TCP: vào phòng
    tcp = socket.create_connection((a.host, a.cong), timeout=5)
    tcp.settimeout(None)
    tcp.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    f = tcp.makefile("r", encoding="utf-8")
    gui_json(tcp, {"loai": "vao", "ten": a.ten})
    chao = json.loads(f.readline())
    ma = chao["ma"]
    log(f"Vào phòng, mã {ma}; nguồn: {mo_ta}")
    log(f"Gửi {MAU_MOI_KHUNG} mẫu/{KHUNG_MS} ms -> 50 gói/s, 256 kbit/s PCM")

    # ---- Kênh dữ liệu UDP: connect() để chỉ nhận gói từ đúng địa chỉ server
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    if hasattr(socket, "SIO_UDP_CONNRESET"):   # Windows: tắt lỗi 10054 ở recvfrom (ICMP cổng đóng)
        udp.ioctl(socket.SIO_UDP_CONNRESET, False)
    udp.connect((a.host, chao["cong_udp"]))

    khoa = threading.Lock()
    tt = {"mic_mo": False, "am_luong": 1.0, "dung": False}
    bo_dem = BoDemJitter(do_sau=a.dem)
    ban_ghi = array.array("h")
    moc = []                                               # (giây trong bản ghi, sự kiện)

    def danh_moc(text):
        with khoa:
            moc.append({"t": round(len(ban_ghi) / TAN_SO_MAU, 3), "su_kien": text})

    def doc_tcp():
        try:
            for dong in f:
                tin = json.loads(dong)
                loai = tin.get("loai")
                if loai == "mic":
                    with khoa:
                        tt["mic_mo"] = tin["mo"]
                    log(f"Mic: {tin['ly_do']}")
                elif loai == "su_kien":
                    log(f"Sự kiện: {tin['noi_dung']}")
                    danh_moc(tin["noi_dung"])
                elif loai == "tu_choi":
                    log(f"Bị từ chối: {tin['ly_do']}")
        except (OSError, ValueError):
            pass
        with khoa:
            tt["dung"] = True              # server đóng kết nối -> client dừng

    def nhan_udp():
        while True:
            try:
                goi = udp.recv(2048)
            except OSError:
                return
            r = tach_rtp(goi)
            if r and r[0] == PT_AM_THANH:
                with khoa:
                    bo_dem.nhan_goi(r[1], r[2], r[4], time.monotonic())

    def lenh(dong):
        p = dong.split()
        if p[0] == "tat_mic":                  # tự tắt: có hiệu lực ngay, không cần server đồng ý
            with khoa:
                tt["mic_mo"] = False
            log("> tat_mic")
            gui_json(tcp, {"loai": "tu_tat_mic"})
        elif p[0] == "bat_mic":                # tự bật: phải chờ server cho phép
            log("> bat_mic (xin server)")
            gui_json(tcp, {"loai": "tu_bat_mic"})
        elif p[0] == "am_luong" and len(p) == 2:
            with khoa:
                tt["am_luong"] = float(p[1])
            log(f"> am_luong {float(p[1]):.2f} (chỉ áp dụng tại client)")
            danh_moc(f"{a.ten} tự đặt âm lượng nghe {float(p[1]):.2f}")
            gui_json(tcp, {"loai": "am_luong_nghe", "gia_tri": float(p[1])})
        elif p[0] == "thoat":
            with khoa:
                tt["dung"] = True
        else:
            log(f"lệnh không hợp lệ: {dong}")

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

    # ---- Vòng lặp 20 ms: thu + gửi, nhận + phát
    rng = random.Random(ma)                    # giả lập mất gói tất định (tái lập được)
    seq = ts = da_gui = bo_gia_lap = 0
    tong_byte = 0
    so_khung = int(a.thoi_gian * 1000 / KHUNG_MS)
    giu = dong_goi_rtp(PT_GIU_KET_NOI, 0, 0, ma)
    udp.send(giu)                              # đăng ký địa chỉ UDP với server ngay từ đầu
    cua_so = []                                # mẫu của 1 giây gần nhất (để đo mức)
    ke_tiep = time.monotonic()
    for n in range(1, so_khung + 1):
        with khoa:
            if tt["dung"]:
                break
            mic_mo, am_luong = tt["mic_mo"], tt["am_luong"]
        # 1. Thu và gửi
        khung = nguon.lay_khung()
        if mic_mo:
            if rng.random() < a.mat_goi:
                bo_gia_lap += 1                # "mạng" làm rơi gói: số thứ tự vẫn tăng
            else:
                goi = dong_goi_rtp(PT_AM_THANH, seq, ts, ma, pcm_sang_byte(khung))
                udp.send(goi)
                da_gui += 1
                tong_byte += len(goi) + 28     # + header UDP (8) và IPv4 (20)
            seq += 1
        elif n % 50 == 0:
            udp.send(giu)                      # mic tắt: mỗi giây một gói giữ kết nối
        ts += MAU_MOI_KHUNG                    # nhãn thời gian tăng theo thời gian thực, kể cả khi tắt mic
        # 2. Nhận và phát
        with khoa:
            p = bo_dem.lay()
        mau = byte_sang_pcm(p) if p else array.array("h", bytes(2 * MAU_MOI_KHUNG))
        if am_luong != 1.0:
            mau = array.array("h", (cat_ngon(round(x * am_luong)) for x in mau))
        with khoa:
            ban_ghi.extend(mau)
        if thiet_bi and a.loa:
            thiet_bi.phat(mau)
        cua_so.extend(mau)
        if n % 50 == 0:
            db = muc_dbfs(cua_so)
            log(f"{n * KHUNG_MS / 1000:4.1f}s nghe {thanh_muc(db)} {chu_db(db)} dBFS"
                f" mic {'mở' if mic_mo else 'tắt'}")
            cua_so = []
        ke_tiep += KHUNG_MS / 1000
        time.sleep(max(0.0, ke_tiep - time.monotonic()))

    # ---- Kết thúc: thống kê, lưu bản ghi
    giay = time.monotonic() - t0
    tcp.close()
    udp.close()
    log(f"Kết thúc sau {giay:.1f} s")
    log(f"Gửi {da_gui} gói ({tong_byte * 8 / giay / 1000:.0f} kbit/s gồm header),"
        f" giả lập mất {bo_gia_lap}")
    b = bo_dem
    log(f"Nhận {b.nhan} gói, mất {b.mat}, trễ {b.tre}, jitter {b.jitter_ms:.2f} ms")
    log(f"Phát: thiếu {b.thieu} khung, cạn {b.can} lần, đệm {a.dem} khung"
        f" = {a.dem * KHUNG_MS} ms")
    if a.ghi:
        os.makedirs(os.path.dirname(a.ghi) or ".", exist_ok=True)
        ghi_wav(a.ghi, ban_ghi)
        duong_moc = os.path.splitext(a.ghi)[0] + "_moc.json"
        with open(duong_moc, "w", encoding="utf-8") as fm:
            json.dump({"ten": a.ten, "moc": moc}, fm, ensure_ascii=False, indent=1)
        log(f"Đã ghi {a.ghi} ({len(ban_ghi) / TAN_SO_MAU:.2f} s), {len(moc)} mốc sự kiện")


if __name__ == "__main__":
    main()
