"""Bài mở rộng MR6.4 – Tìm server Caro trong LAN bằng UDP broadcast (DISCOVER của Bài 6.5).

Chạy:  python3 tim_server.py [cổng_khám_phá] [thời_gian_chờ_ms] [địa_chỉ_broadcast]
       (mặc định 9999 1000 255.255.255.255)
"""
import sys

from giao_thuc import discover


def main():
    a = sys.argv
    port = int(a[1]) if len(a) > 1 else 9999
    wait_ms = int(a[2]) if len(a) > 2 else 1000
    bcast = a[3] if len(a) > 3 else "255.255.255.255"
    print(f"[tim] Phát discover tới {bcast}:{port}, chờ {wait_ms} ms", flush=True)
    found = discover(port, wait_ms, bcast)
    print(f"[tim] Tìm thấy {len(found)} server:", flush=True)
    for ip, sport, name, rooms, players in found:
        print(f"[tim]   {name} tại {ip}:{sport} – {rooms} phòng, {players} người", flush=True)


if __name__ == "__main__":
    main()
