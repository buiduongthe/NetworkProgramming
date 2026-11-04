"""Bài 6.6 – Phát thông báo dịch vụ qua multicast (ANNOUNCER).

Cài đặt Mã giả 6.8, thủ tục ANNOUNCER (Mục 6.4.3, Giáo trình Lập trình mạng). Kế thừa Mã giả 6.7:
thay địa chỉ broadcast bằng địa chỉ nhóm 239.255.42.99 (dải quản trị cục bộ 239.0.0.0/8).
Tham số count (bổ sung để minh họa): dừng sau count lần phát; 0 = phát mãi.

Chạy:  python3 announcer.py [service_port] [tên] [period_ms] [count] [mport]
       (mặc định 9200 server-chat 5000 0 9998)
"""
import json
import random
import socket
import sys
import time

GROUP, MPORT = "239.255.42.99", 9998                    # CONST GROUP, MPORT


def announcer(service_port, name, period_ms, count=0, mport=MPORT):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)                     # SOCKET(INET, DGRAM)
    s.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, 1)              # chỉ trong mạng con
    s.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_LOOP, 1)             # nhận cả trên máy gửi
    print(f"[announcer] Phát tới nhóm {GROUP}:{mport}, chu kỳ {period_ms} ms", flush=True)
    n = 0
    while count == 0 or n < count:
        a = {"type": "announce", "name": name, "port": service_port, "ttl_ms": 3 * period_ms}
        s.sendto(json.dumps(a, ensure_ascii=False).encode("utf-8"), (GROUP, mport))
        n += 1
        print(f"[announcer] Lần {n}: announce {name}, cổng {service_port}, ttl_ms={a['ttl_ms']}", flush=True)
        time.sleep((period_ms + random.randint(0, period_ms // 10)) / 1000)  # có nhiễu
    print(f"[announcer] Dừng phát sau {n} lần", flush=True)
    s.close()


def main():
    a = sys.argv
    try:
        announcer(int(a[1]) if len(a) > 1 else 9200, a[2] if len(a) > 2 else "server-chat",
                  int(a[3]) if len(a) > 3 else 5000, int(a[4]) if len(a) > 4 else 0,
                  int(a[5]) if len(a) > 5 else MPORT)
    except KeyboardInterrupt:
        print("[announcer] Dừng", flush=True)


if __name__ == "__main__":
    main()
