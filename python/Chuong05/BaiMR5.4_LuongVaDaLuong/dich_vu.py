"""Bài mở rộng MR5.4 – Dịch vụ thử (máy chủ phụ trợ) cho các vai trò luong, hang-doi.

Giao thức dòng văn bản, mỗi kết nối một lệnh, mỗi kết nối một luồng:
  VIEC <mã> <ms>   → chờ <ms> mili giây (giả lập xử lý) rồi trả "XONG <mã>"
  PHAT <n>         → gửi n dòng "GOI <k> <dữ liệu>" (k = 1..n), rồi đọc n dòng "KQ <k> <tổng byte>"
                     (thứ tự bất kỳ), kiểm tra và trả "NHAN <số đúng>/<n>"
  THONGKE          → trả "TK <số yêu cầu VIEC> <đồng thời tối đa>", in một dòng tóm tắt "đợt", đặt lại
"""
import socket
import threading
import time

from nhat_ky import say


def du_lieu(k):
    """Dữ liệu của gói thứ k (giống nhau ở mọi bản ngôn ngữ)."""
    return f"goi-so-{k}-" * 3


def tong_byte(text):
    """Kết quả xử lý một gói: tổng giá trị các byte UTF-8, lấy dư 65536."""
    return sum(text.encode()) % 65536


class ThongKe:
    """Đếm số yêu cầu VIEC và số yêu cầu đang chạy cùng lúc (bảo vệ bằng khóa)."""

    def __init__(self):
        self.khoa = threading.Lock()
        self.tong = self.dang_chay = self.toi_da = 0

    def vao(self):
        with self.khoa:
            self.tong += 1
            self.dang_chay += 1
            self.toi_da = max(self.toi_da, self.dang_chay)

    def ra(self):
        with self.khoa:
            self.dang_chay -= 1

    def lay_va_dat_lai(self):
        with self.khoa:
            kq = (self.tong, self.toi_da)
            self.tong = self.toi_da = 0
            return kq


def phuc_vu(conn, tk):
    with conn:
        f = conn.makefile("rw", encoding="utf-8", newline="\n")
        parts = f.readline().split()
        if not parts:
            return
        if parts[0] == "VIEC":
            tk.vao()
            time.sleep(int(parts[2]) / 1000)
            tk.ra()
            f.write(f"XONG {parts[1]}\n")
        elif parts[0] == "PHAT":
            n = int(parts[1])
            for k in range(1, n + 1):
                f.write(f"GOI {k} {du_lieu(k)}\n")
            f.flush()
            dung = 0
            for _ in range(n):
                kq = f.readline().split()
                if len(kq) == 3 and int(kq[2]) == tong_byte(du_lieu(int(kq[1]))):
                    dung += 1
            f.write(f"NHAN {dung}/{n}\n")
            say(f"[dịch vụ] PHAT {n}: nhận {n} kết quả, đúng {dung}/{n}")
        elif parts[0] == "THONGKE":
            tong, toi_da = tk.lay_va_dat_lai()
            f.write(f"TK {tong} {toi_da}\n")
            say(f"[dịch vụ] đợt: {tong} yêu cầu VIEC, đồng thời tối đa {toi_da}")
        f.flush()


def run(host, port):
    tk = ThongKe()
    srv = socket.create_server((host, int(port)), backlog=256)
    say(f"[dịch vụ] lắng nghe {host}:{port} (mỗi kết nối một luồng)")
    try:
        while True:
            conn, _ = srv.accept()
            threading.Thread(target=phuc_vu, args=(conn, tk), daemon=True).start()
    except KeyboardInterrupt:
        pass
    finally:
        srv.close()
