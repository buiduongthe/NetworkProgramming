"""Bài mở rộng MR5.5 – Phối hợp đa luồng và bất đồng bộ trong server mạng (Python).

Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1–5.6 bất đồng bộ) và Chương 6
(Bài 6.1 server đa luồng, Bài 6.2 server bất đồng bộ), MR5.4 (luồng). Server asyncio phải:
  (a) đẩy việc CPU (băm SHA-256 dữ liệu nhận được) sang nhóm luồng mà không chặn vòng lặp sự kiện;
  (b) gọi an toàn một hàm thư viện cũ kiểu CHẶN từ mã async (asyncio.to_thread);
  (c) cho một luồng hệ điều hành chạy nền (cảm biến) giao kết quả VÀO vòng lặp (call_soon_threadsafe);
  (d) truyền hủy và hạn chót qua ranh giới (wait_for → CancelledError → threading.Event → luồng dừng);
  (e) áp lực ngược giữa phần async và nhóm luồng (asyncio.Semaphore);
  (f) cho thấy cách làm SAI – chạy việc CPU ngay trên vòng lặp – bằng nhịp tim 10 ms.

Chạy:  python3 phoi_hop.py server <host> <port>
       python3 phoi_hop.py tai    <host> <port>
"""
import sys

import may_chu
import tai


def main():
    a = sys.argv
    if len(a) < 4 or a[1] not in ("server", "tai"):
        print("Cách dùng: python3 phoi_hop.py server|tai <host> <port>")
        sys.exit(1)
    (may_chu.run if a[1] == "server" else tai.run)(a[2], a[3])


if __name__ == "__main__":
    main()
