"""Bài 6.6 – Nhận thông báo dịch vụ qua multicast (LISTENER).

Cài đặt Mã giả 6.8, thủ tục LISTENER (Mục 6.4.3, Giáo trình Lập trình mạng): bật REUSEADDR trước
BIND để nhiều chương trình trên cùng máy cùng nghe một cổng, tham gia nhóm (IGMP join), cập nhật
bộ nhớ đệm cache và xóa dịch vụ hết hạn. Tham số run_ms (bổ sung để minh họa): nghe trong run_ms
rồi in bộ nhớ đệm và kết thúc; 0 = nghe mãi.

Chạy:  python3 listener.py [run_ms] [mport]      (mặc định 0 9998)
"""
import json
import socket
import struct
import sys
import time

GROUP, MPORT = "239.255.42.99", 9998                    # CONST GROUP, MPORT


def now_ms():
    return time.monotonic() * 1000


def remove_expired(cache, now):
    """REMOVE_EXPIRED: xóa dịch vụ đã quá hạn ttl_ms."""
    for name in [n for n, (_, _, exp) in cache.items() if exp < now]:
        del cache[name]
        print(f"[listener] Hết hạn, xóa: {name}", flush=True)


def listener(cache, run_ms=0, mport=MPORT):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)                     # SOCKET(INET, DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)                  # nhiều tiến trình cùng nghe
    s.bind(("0.0.0.0", mport))                                               # BIND(s, "0.0.0.0", MPORT)
    mreq = struct.pack("4s4s", socket.inet_aton(GROUP), socket.inet_aton("0.0.0.0"))  # ANY_INTERFACE
    s.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, mreq)          # IGMP join
    print(f"[listener] Tham gia nhóm {GROUP}, nghe cổng {mport}", flush=True)
    end = now_ms() + run_ms
    while run_ms == 0 or now_ms() < end:
        if run_ms:
            s.settimeout(max(1, end - now_ms()) / 1000)
        try:
            d, addr = s.recvfrom(2048)                                       # RECVFROM(s, 2048)
            a = json.loads(d.decode("utf-8"))
        except socket.timeout:
            break
        except ValueError:                                                   # JsonError: sai định dạng, bỏ
            continue
        if isinstance(a, dict) and a.get("type") == "announce":
            if a["name"] not in cache:
                print(f"[listener] Thêm {a['name']} tại {addr[0]}:{a['port']}", flush=True)
            cache[a["name"]] = (addr[0], a["port"], now_ms() + a["ttl_ms"])
        remove_expired(cache, now_ms())                                      # dịch vụ hết hạn: xóa
    s.close()


def main():
    a = sys.argv
    run_ms = int(a[1]) if len(a) > 1 else 0
    cache = {}
    try:
        listener(cache, run_ms, int(a[2]) if len(a) > 2 else MPORT)
    except KeyboardInterrupt:
        pass
    items = ", ".join(f"{n} tại {ip}:{p}" for n, (ip, p, _) in cache.items()) or "(trống)"
    print(f"[listener] Bộ nhớ đệm: {items}", flush=True)


if __name__ == "__main__":
    main()
