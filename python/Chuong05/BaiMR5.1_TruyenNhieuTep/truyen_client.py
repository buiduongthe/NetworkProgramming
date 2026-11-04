"""Bài mở rộng MR5.1 – Client truyền nhiều tệp đồng thời, có điều khiển start/pause/resume/stop,
giới hạn tốc độ theo phiên và theo tệp (thùng token), tải tiếp bằng offset + kiểm SHA-256.

Vai:
  client <host> <port> [thư_mục_tải]   – giao diện dòng lệnh: nhập lệnh qua bàn phím.
  demo   <host> <port>                 – tự dựng server cục bộ, tạo tệp thử, chạy kịch bản tự động.

Lệnh của giao diện client (mỗi dòng một lệnh):
  start <tên> [download|upload] [tốc_độ]   bắt đầu một lượt (mặc định download); tốc_độ = giới hạn tệp
  pause <tên>        tạm dừng (giữ tệp .part)        resume <tên>   tải tiếp từ offset
  stop  <tên>        dừng hẳn (xóa tệp .part)         speed <tốc_độ> đổi giới hạn tốc độ phiên
  status             xem tiến trình                   quit           thoát
Đơn vị tốc độ: 512K, 1M, 2MiB, hoặc số byte/giây.
"""
import os
import socket
import sys
import threading
import time

from khung import (CHUNK, KHONG_GIOI_HAN, ThungToken, dinh_dang_byte, doc_du, doc_so_byte,
                   gui_khung, nhan_khung, sha256_tep)

CHAY, TAM_DUNG, DUNG, XONG, LOI = "chạy", "tạm dừng", "dừng", "xong", "lỗi"


class LuotTruyen:
    """Một lượt truyền (một tệp). Chạy trong luồng riêng, điều khiển qua thuộc tính trạng_thái."""

    def __init__(self, ten, huong, duong_cuc_bo, rate_tep):
        self.ten = ten
        self.huong = huong                     # "download" | "upload"
        self.cuc_bo = duong_cuc_bo             # đường tệp phía client
        self.trang_thai = CHAY
        self.tong = 0
        self.xong_byte = 0
        self.sha_server = None
        self.sha_ok = None
        self.t0 = time.monotonic()
        self.thoi_gian = 0.0
        self.bucket = ThungToken(rate_tep)     # giới hạn riêng của tệp
        self.luong = None

    def dung_khi(self):
        return self.trang_thai in (TAM_DUNG, DUNG)


class QuanLy:
    def __init__(self, host, port, thu_muc_tai, kho_server):
        self.host, self.port = host, port
        self.thu_muc = thu_muc_tai
        self.kho_server = kho_server           # để suy ra offset khi upload tiếp
        self.session = ThungToken(KHONG_GIOI_HAN)
        self.luot = {}
        os.makedirs(thu_muc_tai, exist_ok=True)

    # ------------- Điều khiển -------------
    def dat_toc_do_phien(self, rate):
        self.session.dat_rate(rate)
        print(f"[client] Đặt tốc độ phiên = {dinh_dang_byte(int(rate))}/s", flush=True)

    def bat_dau(self, ten, huong="download", rate_tep=KHONG_GIOI_HAN):
        if ten in self.luot and self.luot[ten].trang_thai == CHAY:
            print(f"[client] {ten} đang chạy", flush=True)
            return
        cuc_bo = os.path.join(self.thu_muc, ten) if huong == "download" else os.path.join(self.thu_muc, ten)
        t = LuotTruyen(ten, huong, cuc_bo, rate_tep)
        self.luot[ten] = t
        fn = self._tai_xuong if huong == "download" else self._tai_len
        t.luong = threading.Thread(target=fn, args=(t,), daemon=True)
        t.t0 = time.monotonic()
        t.luong.start()
        gh = "" if rate_tep >= KHONG_GIOI_HAN else f", giới hạn tệp {dinh_dang_byte(int(rate_tep))}/s"
        print(f"[client] BẮT ĐẦU {huong} {ten}{gh}", flush=True)

    def tam_dung(self, ten):
        t = self.luot.get(ten)
        if t and t.trang_thai == CHAY:
            t.trang_thai = TAM_DUNG
            print(f"[client] TẠM DỪNG {ten} (giữ .part)", flush=True)

    def tai_tiep(self, ten):
        t = self.luot.get(ten)
        if t and t.trang_thai == TAM_DUNG:
            print(f"[client] TẢI TIẾP {ten}", flush=True)
            self.bat_dau(ten, t.huong, t.bucket.rate)

    def dung_han(self, ten):
        t = self.luot.get(ten)
        if t and t.trang_thai in (CHAY, TAM_DUNG):
            t.trang_thai = DUNG
            print(f"[client] DỪNG {ten} (xóa .part)", flush=True)

    def trang_thai(self):
        print("[client] --- Trạng thái ---", flush=True)
        for ten, t in self.luot.items():
            pct = (100 * t.xong_byte / t.tong) if t.tong else 0
            print(f"[client]   {ten:12} {t.huong:8} {t.trang_thai:8} "
                  f"{thanh_tien_do(pct)} {pct:5.1f}%  {dinh_dang_byte(t.xong_byte)}/"
                  f"{dinh_dang_byte(t.tong)}", flush=True)

    # ------------- Thân luồng truyền -------------
    def _tai_xuong(self, t):
        part = t.cuc_bo + ".part"
        off = os.path.getsize(part) if os.path.exists(part) else 0
        try:
            s = socket.create_connection((self.host, self.port))
            gui_khung(s, {"op": "download", "name": t.ten, "offset": off})
            resp = nhan_khung(s)
            if not resp.get("ok"):
                t.trang_thai = LOI
                print(f"[client] {t.ten}: lỗi {resp.get('loi')}", flush=True)
                return
            t.tong, t.sha_server = resp["size"], resp["sha256"]
            t.xong_byte = off
            if off >= t.tong:                         # đã đủ: chỉ cần kiểm và đổi tên
                s.close()
                self._hoan_tat_tai_xuong(t, part)
                return
            f = open(part, "r+b" if off > 0 else "wb")
            f.seek(off)
            while t.xong_byte < t.tong:
                if t.trang_thai == DUNG:
                    f.close(); s.close()
                    if os.path.exists(part):
                        os.remove(part)
                    return
                if t.trang_thai == TAM_DUNG:
                    f.close(); s.close()
                    return
                b = s.recv(CHUNK)
                if not b:
                    break
                f.write(b)
                t.xong_byte += len(b)
                self.session.tieu_thu(len(b), t.dung_khi)   # điều tiết theo phiên
                t.bucket.tieu_thu(len(b), t.dung_khi)       # và theo tệp
            f.close(); s.close()
            if t.xong_byte >= t.tong:
                self._hoan_tat_tai_xuong(t, part)
        except (ConnectionError, OSError) as e:
            if t.trang_thai not in (DUNG, TAM_DUNG):
                t.trang_thai = LOI
                print(f"[client] {t.ten}: lỗi kết nối {e}", flush=True)

    def _hoan_tat_tai_xuong(self, t, part):
        t.sha_ok = sha256_tep(part) == t.sha_server
        os.replace(part, t.cuc_bo)
        t.thoi_gian = time.monotonic() - t.t0
        t.trang_thai = XONG
        print(f"[client] XONG download {t.ten}: {dinh_dang_byte(t.tong)} trong {t.thoi_gian:.1f}s "
              f"({dinh_dang_byte(int(t.tong / max(t.thoi_gian, 0.001)))}/s), "
              f"SHA-256 {'KHỚP' if t.sha_ok else 'SAI'}", flush=True)

    def _tai_len(self, t):
        try:
            total = os.path.getsize(t.cuc_bo)
            part_server = os.path.join(self.kho_server, t.ten + ".part")
            off = os.path.getsize(part_server) if os.path.exists(part_server) else 0
            sha = sha256_tep(t.cuc_bo)
            s = socket.create_connection((self.host, self.port))
            gui_khung(s, {"op": "upload", "name": t.ten, "offset": off, "size": total, "sha256": sha})
            resp = nhan_khung(s)
            if not resp.get("ok"):
                t.trang_thai = LOI
                return
            t.tong, t.xong_byte = total, off
            with open(t.cuc_bo, "rb") as f:
                f.seek(off)
                while t.xong_byte < total:
                    if t.trang_thai in (DUNG, TAM_DUNG):
                        s.close()
                        return
                    b = f.read(CHUNK)
                    if not b:
                        break
                    self.session.tieu_thu(len(b), t.dung_khi)
                    t.bucket.tieu_thu(len(b), t.dung_khi)
                    s.sendall(b)
                    t.xong_byte += len(b)
            ket = nhan_khung(s)
            s.close()
            t.thoi_gian = time.monotonic() - t.t0
            t.sha_ok = ket.get("sha256_ok")
            t.trang_thai = XONG
            print(f"[client] XONG upload {t.ten}: {dinh_dang_byte(total)} trong {t.thoi_gian:.1f}s "
                  f"({dinh_dang_byte(int(total / max(t.thoi_gian, 0.001)))}/s), "
                  f"server kiểm SHA-256 {'KHỚP' if t.sha_ok else 'SAI'}", flush=True)
        except (ConnectionError, OSError) as e:
            if t.trang_thai not in (DUNG, TAM_DUNG):
                t.trang_thai = LOI
                print(f"[client] {t.ten}: lỗi kết nối {e}", flush=True)


def thanh_tien_do(pct, rong=20):
    day = int(pct / 100 * rong)
    return "[" + "#" * day + "-" * (rong - day) + "]"


# ----------------------------- Vai: giao diện dòng lệnh -----------------------------
def chay_giao_dien(host, port, thu_muc):
    ql = QuanLy(host, port, thu_muc, kho_server="kho_server")
    print("[client] Nhập lệnh (start/pause/resume/stop/speed/status/quit).", flush=True)
    for dong in sys.stdin:
        p = dong.split()
        if not p:
            continue
        lenh = p[0].lower()
        try:
            if lenh == "start":
                huong = p[2] if len(p) > 2 else "download"
                rate = doc_so_byte(p[3]) if len(p) > 3 else KHONG_GIOI_HAN
                ql.bat_dau(p[1], huong, rate)
            elif lenh == "pause":
                ql.tam_dung(p[1])
            elif lenh == "resume":
                ql.tai_tiep(p[1])
            elif lenh == "stop":
                ql.dung_han(p[1])
            elif lenh == "speed":
                ql.dat_toc_do_phien(doc_so_byte(p[1]))
            elif lenh == "status":
                ql.trang_thai()
            elif lenh in ("quit", "exit"):
                break
            else:
                print("[client] Lệnh lạ", flush=True)
        except IndexError:
            print("[client] Thiếu tham số", flush=True)


# ----------------------------- Vai: demo tự động -----------------------------
def cho_den(ql, dieu_kien, han=30):
    het = time.monotonic() + han
    while time.monotonic() < het:
        if dieu_kien():
            return True
        time.sleep(0.1)
    return False


def chay_demo(host, port):
    import shutil
    import tempfile

    from truyen_server import phuc_vu

    goc = tempfile.mkdtemp(prefix="mr51_")
    kho = os.path.join(goc, "kho_server")
    tai = os.path.join(goc, "tai_ve")
    nguon = os.path.join(goc, "nguon")
    for d in (kho, tai, nguon):
        os.makedirs(d, exist_ok=True)

    # Server cục bộ trong một luồng (cùng mã với vai server).
    srv = socket.create_server((host, port))
    srv.listen(16)

    def vong_server():
        while True:
            try:
                conn, _ = srv.accept()
            except OSError:
                return
            threading.Thread(target=phuc_vu, args=(conn, kho), daemon=True).start()
    threading.Thread(target=vong_server, daemon=True).start()
    print(f"[demo] Server cục bộ tại {host}:{port}, thư mục tạm {goc}", flush=True)

    # Tạo 3 tệp thử (3 MiB mỗi tệp) trực tiếp trong kho server để tải xuống.
    cac_tep = ["tep1.bin", "tep2.bin", "tep3.bin"]
    for ten in cac_tep:
        with open(os.path.join(kho, ten), "wb") as f:
            f.write(os.urandom(3 * 1024 * 1024))
    print(f"[demo] Đã tạo {len(cac_tep)} tệp thử, mỗi tệp 3 MiB", flush=True)

    ql = QuanLy(host, port, tai, kho)

    # Luồng in tiến trình định kỳ.
    dang_chay = [True]

    def bao_cao():
        truoc = {}
        while dang_chay[0]:
            time.sleep(0.8)
            for ten, t in list(ql.luot.items()):
                if t.trang_thai == CHAY and t.tong:
                    truoc_byte, truoc_tg = truoc.get(ten, (0, t.t0))
                    now = time.monotonic()
                    if t.xong_byte < truoc_byte:          # tệp bị dừng rồi chạy lại: đặt lại mốc
                        truoc_byte, truoc_tg = 0, now
                    toc = max(0.0, (t.xong_byte - truoc_byte)) / max(now - truoc_tg, 0.001)
                    truoc[ten] = (t.xong_byte, now)
                    pct = 100 * t.xong_byte / t.tong
                    print(f"[tiến trình] {ten} {thanh_tien_do(pct)} {pct:5.1f}%  "
                          f"{dinh_dang_byte(int(toc))}/s", flush=True)
    threading.Thread(target=bao_cao, daemon=True).start()

    print("\n===== KỊCH BẢN: tải xuống 3 tệp đồng thời, giới hạn phiên 2 MiB/s =====", flush=True)
    ql.dat_toc_do_phien(doc_so_byte("2M"))
    for ten in cac_tep:
        ql.bat_dau(ten, "download")

    time.sleep(1.2)
    ql.tam_dung("tep2.bin")                       # tạm dừng giữa chừng
    time.sleep(0.8)
    ql.dung_han("tep3.bin")                        # dừng hẳn, xóa .part
    time.sleep(0.3)
    print("\n===== Nâng tốc độ phiên lên 8 MiB/s, tải tiếp tep2, chạy lại tep3 =====", flush=True)
    ql.dat_toc_do_phien(doc_so_byte("8M"))
    ql.tai_tiep("tep2.bin")                        # tải tiếp từ offset
    ql.bat_dau("tep3.bin", "download")             # chạy lại từ đầu

    cho_den(ql, lambda: all(ql.luot[t].trang_thai in (XONG, LOI) for t in cac_tep), han=30)
    ql.trang_thai()

    print("\n===== KỊCH BẢN upload: đẩy tep1 (đã tải) lên server, giới hạn tệp 1 MiB/s =====", flush=True)
    # Dùng tệp vừa tải về làm nguồn upload.
    nguon_tep = os.path.join(tai, "tep1.bin")
    ql2 = QuanLy(host, port, tai, kho)
    ql2.session = ql.session
    # Đặt tên đích khác để không đè tệp gốc.
    dich = os.path.join(nguon, "tep1_tai_len.bin")
    shutil.copy(nguon_tep, dich)
    t = LuotTruyen("tep1_tai_len.bin", "upload", dich, doc_so_byte("1M"))
    ql2.luot["tep1_tai_len.bin"] = t
    t.luong = threading.Thread(target=ql2._tai_len, args=(t,), daemon=True)
    t.t0 = time.monotonic()
    t.luong.start()
    print("[client] BẮT ĐẦU upload tep1_tai_len.bin, giới hạn tệp 1.00 MiB/s", flush=True)
    cho_den(ql2, lambda: t.trang_thai in (XONG, LOI), han=20)

    dang_chay[0] = False
    time.sleep(0.2)
    print("\n===== TỔNG KẾT =====", flush=True)
    for ten in cac_tep:
        tt = ql.luot[ten]
        print(f"[demo] {ten}: {tt.trang_thai}, {dinh_dang_byte(tt.xong_byte)}, "
              f"SHA-256 {'KHỚP' if tt.sha_ok else '—'}", flush=True)
    print(f"[demo] tep1_tai_len.bin: {t.trang_thai}, server kiểm SHA-256 "
          f"{'KHỚP' if t.sha_ok else '—'}", flush=True)
    print(f"[demo] Server còn giữ: {sorted(os.listdir(kho))}", flush=True)

    srv.close()
    shutil.rmtree(goc, ignore_errors=True)
    print(f"[demo] Đã dọn thư mục tạm {goc}", flush=True)


def main():
    if len(sys.argv) < 4:
        sys.exit("Cách dùng: python3 truyen_client.py client|demo <host> <port> [thư_mục_tải]")
    vai, host, port = sys.argv[1], sys.argv[2], int(sys.argv[3])
    if vai == "client":
        chay_giao_dien(host, port, sys.argv[4] if len(sys.argv) > 4 else "tai_ve")
    elif vai == "demo":
        chay_demo(host, port)
    else:
        sys.exit("Vai phải là client hoặc demo")


if __name__ == "__main__":
    main()
