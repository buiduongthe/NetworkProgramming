"""Bài 6.5 – Khám phá server chat trong LAN bằng broadcast: phía client (DISCOVER).

Cài đặt Mã giả 6.7, hàm DISCOVER (Mục 6.4.2, Giáo trình Lập trình mạng). Kế thừa Mã giả 4.4
(UDP_REQUEST): gửi một yêu cầu nhưng thu thập NHIỀU phản hồi đến hết thời gian chờ.

Chạy:  python3 discover.py [wait_ms] [disc_port] [địa_chỉ_broadcast]
       (mặc định 2000 9999 255.255.255.255)
"""
import json
import random
import socket
import sys
import time

DISC_PORT = 9999                                        # CONST DISC_PORT ← 9999


def now_ms():
    return time.monotonic() * 1000


def discover(wait_ms, disc_port=DISC_PORT, bcast="255.255.255.255"):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)             # SOCKET(INET, DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)         # SETOPT(BROADCAST): bắt buộc
    q = {"type": "discover", "service": "chat", "id": random.randint(1, 999999)}   # NEXT_ID()
    s.sendto(json.dumps(q).encode("utf-8"), (bcast, disc_port))
    print(f"[client] Phát discover id={q['id']} tới {bcast}:{disc_port}", flush=True)
    print(f"[client] Thu thập phản hồi trong {wait_ms} ms", flush=True)
    found, deadline = [], now_ms() + wait_ms
    while now_ms() < deadline:
        s.settimeout(max(1, deadline - now_ms()) / 1000)            # MAX(1, …): giá trị 0 nghĩa là chờ vô hạn
        try:
            d, addr = s.recvfrom(2048)
            r = json.loads(d.decode("utf-8"))
        except socket.timeout:
            break
        except ValueError:                                          # JsonError: sai định dạng, bỏ
            continue
        if isinstance(r, dict) and r.get("id") == q["id"]:
            found.append((addr[0], r.get("port"), r.get("name")))
    s.close()
    return found


def main():
    a = sys.argv
    wait_ms = int(a[1]) if len(a) > 1 else 2000
    disc_port = int(a[2]) if len(a) > 2 else DISC_PORT
    bcast = a[3] if len(a) > 3 else "255.255.255.255"
    found = discover(wait_ms, disc_port, bcast)
    print(f"[client] Tìm thấy {len(found)} server:", flush=True)
    for ip, port, name in found:
        print(f"[client]   {name} tại {ip}:{port}", flush=True)


if __name__ == "__main__":
    main()
