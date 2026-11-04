"""Bài mở rộng MR4.1 – Máy xuất NetFlow v5 giả lập (exporter) gửi qua UDP.

Sinh các bản ghi luồng (flow) giả lập theo một bộ sinh số giả ngẫu nhiên đơn giản (LCG)
có hạt giống cố định, nên bản Python và bản Go sinh ra đúng cùng một dữ liệu.
Mỗi gói UDP chứa tối đa 30 bản ghi. Tùy chọn --bo-goi K cố ý KHÔNG gửi gói thứ K
(mô phỏng mất gói trên đường truyền); cuối cùng gửi thêm một datagram rác để thử bộ thu.

Chạy:  python3 may_xuat.py <host> <port> [--so-luong N] [--bo-goi K]
"""
import argparse
import socket
import struct
import time

from netflow_v5 import HEADER, MAX_RECORDS, RECORD

HOSTS = ["10.20.0.11", "10.20.0.12", "10.20.0.13", "10.20.0.14", "10.20.1.5", "10.20.1.6"]
SERVICES = [("10.20.1.194", 443, 6), ("10.20.1.194", 80, 6), ("10.20.1.195", 5432, 6),
            ("10.20.1.196", 53, 17), ("203.0.113.10", 443, 6)]


class Lcg:
    """x = (x * 1103515245 + 12345) mod 2^31, lấy 15 bit cao (các bit thấp của LCG kém ngẫu nhiên).
    Đủ để sinh dữ liệu thử, KHÔNG dùng cho bảo mật."""
    def __init__(self, seed):
        self.x = seed

    def next(self):
        self.x = (self.x * 1103515245 + 12345) & 0x7FFFFFFF
        return self.x >> 16


def make_flows(n, seed=2026):
    rng = Lcg(seed)
    flows = []
    for _ in range(n):
        src = HOSTS[rng.next() % len(HOSTS)]
        dst, dport, proto = SERVICES[rng.next() % len(SERVICES)]
        pkts = 1 + rng.next() % 200
        octets = pkts * (60 + rng.next() % 1400)
        sport = 32768 + rng.next() % 28000
        first = rng.next() % 60000
        flows.append((src, dst, sport, dport, proto, pkts, octets, first, first + rng.next() % 5000))
    return flows


def pack_packet(flows, seq, uptime_ms):
    now = time.time()
    head = HEADER.pack(5, len(flows), uptime_ms, int(now), int(now % 1 * 1e9), seq, 0, 1, 0)
    body = b"".join(
        RECORD.pack(socket.inet_aton(src), socket.inet_aton(dst), socket.inet_aton("0.0.0.0"),
                    1, 2, pkts, octets, first, last, sport, dport, 0,
                    0x1B if proto == 6 else 0, proto, 0, 0, 0, 24, 27, 0)
        for src, dst, sport, dport, proto, pkts, octets, first, last in flows)
    return head + body


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("host")
    ap.add_argument("port", type=int)
    ap.add_argument("--so-luong", type=int, default=100, help="tổng số luồng sinh ra")
    ap.add_argument("--bo-goi", type=int, default=0, help="thứ tự gói cố ý không gửi")
    a = ap.parse_args()
    flows = make_flows(a.so_luong)
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    seq = 0
    for k, start in enumerate(range(0, len(flows), MAX_RECORDS), 1):
        chunk = flows[start:start + MAX_RECORDS]
        data = pack_packet(chunk, seq, 120000 + k * 1000)
        if k == a.bo_goi:
            print(f"[xuat] Gói #{k}: {len(chunk)} bản ghi, seq={seq} -> BỎ (giả lập mất gói)", flush=True)
        else:
            sock.sendto(data, (a.host, a.port))
            print(f"[xuat] Gói #{k}: {len(chunk)} bản ghi, seq={seq}, {len(data)} byte", flush=True)
        seq += len(chunk)                     # flow_sequence đếm tổng số luồng đã xuất
        time.sleep(0.05)
    sock.sendto(b"xin chao, day khong phai NetFlow", (a.host, a.port))
    print("[xuat] Gửi thêm 1 datagram rác để thử bộ thu", flush=True)
    sock.close()


if __name__ == "__main__":
    main()
