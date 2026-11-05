"""Bài mở rộng MR5.4 (c), (f) – Trạng thái dùng chung giữa các luồng xử lý kết nối; bế tắc.

(c) Một server đếm nhỏ chạy ngay trong chương trình: 4 luồng "gửi-k" mở 4 kết nối TCP, mỗi kết nối gửi
    2000 dòng "DEM GET" / "DEM PUT" xen kẽ; server tạo mỗi kết nối một luồng "xử-lý-k". Với MỖI thông điệp,
    luồng xử lý cập nhật bốn bộ đếm dùng chung:
      1. không khóa: đọc → nhường CPU (time.sleep(0), giống ghi nhật ký/gọi mạng giữa chừng) → ghi
         → luồng khác chen vào giữa nên MẤT CẬP NHẬT;
      2. có khóa (threading.Lock) quanh đúng đoạn đọc – nhường – ghi đó → đúng;
      3. "nguyên tử": Python không có kiểu số nguyên nguyên tử; ở đây dùng itertools.count – lời gọi
         next() thực hiện trọn trong mã C khi giữ GIL nên không bị chen ngang (chi tiết cài đặt của
         CPython có GIL, không được bảo đảm ở bản free-threaded) – giá trị cuối = next(bộ đếm);
      4. bảng theo loại thông điệp {GET, PUT}: thư viện chuẩn Python không có dict đồng thời → dùng
         dict + một khóa riêng của bảng (ngôn ngữ khác: ConcurrentHashMap, ConcurrentDictionary, sync.Map).
(f) Bế tắc: luồng "chuyển-AB" chuyển tin từ phiên A sang phiên B (khóa A rồi B), "chuyển-BA" làm ngược lại
    (khóa B rồi A). Chờ khóa có hạn (acquire(timeout=0.5)) để PHÁT HIỆN thay vì treo; sửa bằng thứ tự lấy
    khóa cố định (A trước B) cho mọi luồng.
"""
import itertools
import socket
import threading
import time

from nhat_ky import log, say

N_KET_NOI, N_TIN = 4, 2000
GIU_MS, HAN_CHO_MS = 50, 500


class BoDem:
    def __init__(self):
        self.khong_khoa = 0
        self.co_khoa = 0
        self.khoa = threading.Lock()
        self.nguyen_tu = itertools.count()
        self.bang = {}
        self.khoa_bang = threading.Lock()

    def cap_nhat(self, loai):
        v = self.khong_khoa              # 1. đọc
        time.sleep(0)                    #    nhường CPU: luồng khác có thể chen vào đây
        self.khong_khoa = v + 1          #    ghi – có thể đè lên kết quả của luồng khác
        with self.khoa:                  # 2. đoạn găng có khóa
            v = self.co_khoa
            time.sleep(0)
            self.co_khoa = v + 1
        next(self.nguyen_tu)             # 3. tăng "nguyên tử"
        with self.khoa_bang:             # 4. bảng dùng chung
            self.bang[loai] = self.bang.get(loai, 0) + 1


def xu_ly_ket_noi(conn, bd):
    so_tin = 0
    with conn, conn.makefile(encoding="utf-8") as f:
        for line in f:
            parts = line.split()
            if len(parts) == 2 and parts[0] == "DEM":
                bd.cap_nhat(parts[1])
                so_tin += 1
    log(f"kết nối đóng: đã xử lý {so_tin} thông điệp")


def gui(host, port, k):
    data = "".join(f"DEM {'GET' if i % 2 == 0 else 'PUT'}\n" for i in range(N_TIN))
    with socket.create_connection((host, port)) as s:
        s.sendall(data.encode())


def tranh_chap(host, port):
    say(f"(c) Trạng thái dùng chung: {N_KET_NOI} kết nối × {N_TIN} thông điệp DEM, "
        "mỗi kết nối một luồng xử lý")
    bd = BoDem()
    srv = socket.create_server((host, int(port)), backlog=16)
    log(f"server đếm lắng nghe {host}:{port}; {N_KET_NOI} luồng gửi bắt đầu")
    gui_ths = [threading.Thread(target=gui, args=(host, int(port), k), name=f"gửi-{k}")
               for k in range(1, N_KET_NOI + 1)]
    for th in gui_ths:
        th.start()
    xl_ths = []
    for k in range(1, N_KET_NOI + 1):
        conn, _ = srv.accept()
        th = threading.Thread(target=xu_ly_ket_noi, args=(conn, bd), name=f"xử-lý-{k}")
        th.start()
        xl_ths.append(th)
    for th in gui_ths + xl_ths:
        th.join()
    srv.close()
    dung = N_KET_NOI * N_TIN
    nt = next(bd.nguyen_tu)
    get, put = bd.bang.get("GET", 0), bd.bang.get("PUT", 0)
    say()
    say(f"  {'Cách cập nhật biến chung':<32}{'Giá trị':>9}{'Đúng':>6}{'Mất cập nhật':>13}")
    for ten, gt, so in [("không khóa (đọc – nhường – ghi)", str(bd.khong_khoa), bd.khong_khoa),
                        ("khóa (mutex/lock)", str(bd.co_khoa), bd.co_khoa),
                        ("nguyên tử (atomic)", str(nt), nt),
                        ("bảng đồng thời: GET + PUT", f"{get}+{put}", get + put)]:
        say(f"  {ten:<32}{gt:>9}{dung:>6}{dung - so:>13}")
    say("Nhận xét: \"đọc – làm việc khác – ghi\" không khóa làm mất cập nhật; khóa, thao tác nguyên tử")
    say("  hoặc cấu trúc dữ liệu đồng thời đều cho kết quả đúng.")


def be_tac():
    say()
    say("(f) Bế tắc: hai luồng chuyển tin giữa phiên A và phiên B, mỗi luồng cần khóa của cả hai phiên")
    khoa = {"A": threading.Lock(), "B": threading.Lock()}
    bang = []

    def chuyen(nguon, dich, co_thu_tu, kq):
        dau, sau = sorted([nguon, dich]) if co_thu_tu else (nguon, dich)
        with khoa[dau]:
            log(f"giữ khóa phiên {dau}, chờ khóa phiên {sau}")
            time.sleep(GIU_MS / 1000)                   # giả lập ghi vào socket của phiên đang giữ
            if not khoa[sau].acquire(timeout=HAN_CHO_MS / 1000):
                log(f"chờ khóa phiên {sau} quá {HAN_CHO_MS} ms → nghi bế tắc, nhả khóa phiên {dau}, bỏ cuộc")
                kq["bo"] += 1
                return
            try:
                log(f"đã chuyển tin từ phiên {nguon} sang phiên {dich}")
                kq["xong"] += 1
            finally:
                khoa[sau].release()

    for lan, co_thu_tu in [(1, False), (2, True)]:
        if co_thu_tu:
            say(f"Lần {lan} – mọi luồng lấy khóa theo cùng thứ tự (A trước B):")
        else:
            say(f"Lần {lan} – lấy khóa theo thứ tự ngược nhau:")
        kq = {"xong": 0, "bo": 0}
        t1 = threading.Thread(target=chuyen, args=("A", "B", co_thu_tu, kq), name="chuyển-AB")
        t2 = threading.Thread(target=chuyen, args=("B", "A", co_thu_tu, kq), name="chuyển-BA")
        t1.start()
        time.sleep(0.01)
        t2.start()
        t1.join()
        t2.join()
        bang.append(("cùng thứ tự" if co_thu_tu else "ngược thứ tự", kq))
    say()
    say(f"  {'Cách lấy khóa':<20}{'Hoàn tất':>10}{'Bỏ cuộc (hết hạn chờ)':>24}")
    for ten, kq in bang:
        say(f"  {ten:<20}{kq['xong']:>10}{kq['bo']:>24}")
    say("Nhận xét: cùng một thứ tự lấy khóa cho mọi luồng loại bỏ vòng chờ – điều kiện cần của bế tắc.")


def run(host, port):
    tranh_chap(host, port)
    be_tac()
