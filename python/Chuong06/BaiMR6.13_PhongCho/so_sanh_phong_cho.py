"""Bài mở rộng MR6.13 – So sánh BẬT / TẮT phòng chờ ảo (dòng lệnh).

Chạy hai lượt mô phỏng ở cùng một chế độ (mặc định khoa, đúng): phòng chờ TẮT (mọi người dồn vào "mua",
máy chủ giới hạn số yêu cầu đang xử lý -> từ chối "qua_tai", người dùng bấm lại) rồi BẬT (xếp hàng, cấp
lượt theo thùng token). Bảng: số đơn mua được, số lần bị từ chối vì quá tải, số người hết vé, thời gian
mua p50/p99 (từ lần bấm "mua" đầu tiên tới kết quả cuối, gồm cả các lần bấm lại) và thời gian cả lượt.

Chạy:  python3 so_sanh_phong_cho.py [host] [cổng] [--n 1000] [--k 200] [--che-do khoa]
"""
import argparse

import mo_phong
from chung import CHE_DO


def main():
    ap = argparse.ArgumentParser(description="So sánh bật/tắt phòng chờ ảo")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41770)
    ap.add_argument("--n", type=int, default=1000, help="số người dùng ảo")
    ap.add_argument("--k", type=int, default=None, help="số kết nối (mặc định min(n, 200))")
    ap.add_argument("--che-do", choices=CHE_DO, default="khoa")
    a = ap.parse_args()
    k = a.k if a.k else min(a.n, 200)

    def bao(text):
        print(f"[mo_phong] {text}", flush=True)
    hang = mo_phong.so_sanh_phong_cho(a.host, a.cong, a.n, k, bao, a.che_do)
    bao(f"Bật/tắt phòng chờ ({a.n} người, {a.che_do}, {min(k, a.n)} kết nối):")
    for d in mo_phong.bang_phong_cho(hang):
        print(d, flush=True)


if __name__ == "__main__":
    main()
