"""Bài mở rộng MR3.1 – Bộ chuyển tiếp cổng TCP (TCP port forwarder).

Bài mở rộng, không có trong giáo trình; vận dụng Chương 3 (vòng đời socket TCP, SEND_ALL,
SHUTDOWN một chiều) và Chương 6 (mỗi kết nối một luồng).
Mỗi client nối vào cổng nghe được ghép với một kết nối mới tới máy đích; hai luồng
bơm dữ liệu theo hai chiều. Khi một chiều gặp EOF, bộ chuyển tiếp gọi SHUTDOWN(WRITE)
trên socket bên kia để báo "hết dữ liệu" (chuyển tiếp gói FIN), chứ không đóng hẳn,
vì chiều ngược lại có thể vẫn đang truyền.
Đây là nguyên lý của "ssh -L", proxy TCP và cân bằng tải lớp 4.

Chạy:  python3 chuyen_tiep.py <host_nghe> <cổng_nghe> <host_đích> <cổng_đích>
"""
import itertools
import socket
import sys
import threading

counter = itertools.count(1)


def pump(src, dst, label, stats, key):
    """Chép dữ liệu src -> dst tới khi src hết dữ liệu, rồi báo EOF cho dst."""
    total = 0
    try:
        while True:
            data = src.recv(65536)
            if not data:
                break
            dst.sendall(data)                        # SEND_ALL
            total += len(data)
    except OSError as e:
        print(f"[chuyen-tiep] {label}: lỗi {e}", flush=True)
    finally:
        stats[key] = total
        try:
            dst.shutdown(socket.SHUT_WR)             # chuyển tiếp FIN, không đóng hẳn socket
        except OSError:
            pass


def serve(client, addr, target):
    cid = next(counter)
    try:
        upstream = socket.create_connection(target, timeout=3)
        upstream.settimeout(None)
    except OSError as e:
        print(f"[chuyen-tiep] #{cid} {addr[0]}:{addr[1]}: không nối được đích "
              f"{target[0]}:{target[1]} ({e.strerror or e})", flush=True)
        client.close()                               # client nhận EOF ngay
        return
    print(f"[chuyen-tiep] #{cid} {addr[0]}:{addr[1]} <-> đích {target[0]}:{target[1]}", flush=True)
    stats = {}
    t1 = threading.Thread(target=pump, args=(client, upstream, f"#{cid} lên", stats, "len"))
    t2 = threading.Thread(target=pump, args=(upstream, client, f"#{cid} xuống", stats, "xuong"))
    t1.start(); t2.start()
    t1.join(); t2.join()                             # chờ cả hai chiều kết thúc
    client.close(); upstream.close()
    print(f"[chuyen-tiep] #{cid} xong: lên {stats['len']} byte, xuống {stats['xuong']} byte", flush=True)


def main():
    if len(sys.argv) < 5:
        sys.exit("Cách dùng: python3 chuyen_tiep.py <host_nghe> <cổng_nghe> <host_đích> <cổng_đích>")
    lhost, lport, thost, tport = sys.argv[1], int(sys.argv[2]), sys.argv[3], int(sys.argv[4])
    srv = socket.create_server((lhost, lport))
    print(f"[chuyen-tiep] {lhost}:{lport} -> {thost}:{tport}", flush=True)
    try:
        while True:
            client, addr = srv.accept()
            threading.Thread(target=serve, args=(client, addr, (thost, tport)), daemon=True).start()
    except KeyboardInterrupt:
        print("[chuyen-tiep] Dừng", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
