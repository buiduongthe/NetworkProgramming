"""Bài 5.2 – Mở các cổng minh họa trên 127.0.0.1 để chạy thử chương trình quét.

- Cổng "mở": socket lắng nghe bình thường, chấp nhận và đóng ngay mọi kết nối.
- Cổng "bị lọc" (mô phỏng): socket lắng nghe với backlog 1 không bao giờ ACCEPT; chương trình
  tự lấp đầy hàng đợi kết nối, sau đó Linux bỏ qua gói SYN mới nên phía quét hết thời gian chờ,
  giống cổng bị tường lửa lọc. (Trên Windows hàng đợi đầy thường trả RST, tức cổng đóng.)
Các cổng còn lại trong dải không có chương trình lắng nghe nên ở trạng thái đóng.

Chạy:  python3 demo_ports.py <danh_sách_cổng_mở> <danh_sách_cổng_lọc>
       ví dụ: python3 demo_ports.py 9003,9011,9017 9005,9008,9013,9019
"""
import socket
import sys
import threading


def open_port(port):
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", port))
    srv.listen(16)

    def accept_loop():
        while True:
            conn, _ = srv.accept()
            conn.close()
    threading.Thread(target=accept_loop, daemon=True).start()
    return srv


def filtered_port(port):
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", port))
    srv.listen(1)                                          # hàng đợi rất ngắn, không bao giờ ACCEPT
    fillers = []
    for _ in range(8):                                     # lấp đầy hàng đợi
        c = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        c.settimeout(0.3)
        try:
            c.connect(("127.0.0.1", port))
            fillers.append(c)
        except OSError:                                    # SYN bị bỏ qua: hàng đợi đã đầy
            c.close()
            break
    return [srv] + fillers


def parse_list(text):
    return [int(x) for x in text.split(",") if x]


def main():
    opened = parse_list(sys.argv[1]) if len(sys.argv) > 1 else [9003, 9011, 9017]
    filtered = parse_list(sys.argv[2]) if len(sys.argv) > 2 else [9005, 9008, 9013, 9019]
    keep = [open_port(p) for p in opened]
    for p in filtered:
        keep += filtered_port(p)
    print(f"[demo] Cổng mở: {', '.join(map(str, opened))}", flush=True)
    print(f"[demo] Cổng bị lọc (mô phỏng): {', '.join(map(str, filtered))}", flush=True)
    print("[demo] Nhấn Ctrl+C để dừng", flush=True)
    try:
        threading.Event().wait()
    except KeyboardInterrupt:
        print("[demo] Dừng", flush=True)
    finally:
        for s in keep:
            s.close()


if __name__ == "__main__":
    main()
