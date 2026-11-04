"""Bài mở rộng MR4.1 – Bộ thu thập NetFlow v5 (collector) trên UDP.

Bài mở rộng, không có trong giáo trình; vận dụng Chương 4: server UDP nhận datagram,
mỗi datagram là trọn một gói xuất (UDP giữ ranh giới thông điệp), giải mã nhị phân
bằng struct như Bài 4.3 (truy vấn DNS). Bộ thu:
  - kiểm tra tính hợp lệ (phiên bản, độ dài = 24 + 48 x count) và loại gói hỏng;
  - phát hiện mất gói nhờ trường flow_sequence (giống số thứ tự ở Bài 4.4);
  - tổng hợp: nguồn dùng nhiều băng thông nhất, dịch vụ (cổng/giao thức) phổ biến.
Dừng khi nhàn rỗi quá --nhan-roi giây sau gói đầu tiên (hoặc Ctrl+C) và in báo cáo.

Chạy:  python3 bo_thu_thap.py [host] [port] [--nhan-roi S]
"""
import argparse
import socket
from collections import defaultdict

from netflow_v5 import HEADER, MAX_RECORDS, PROTO_NAME, RECORD


def parse_packet(data):
    """Trả về (tiêu_đề, danh_sách_bản_ghi) hoặc ném ValueError nếu gói không hợp lệ."""
    if len(data) < HEADER.size:
        raise ValueError(f"quá ngắn ({len(data)} byte)")
    version, count, _uptime, _secs, _nsecs, seq, _etype, eid, _samp = HEADER.unpack_from(data)
    if version != 5:
        raise ValueError(f"phiên bản {version}, không phải 5")
    if not 1 <= count <= MAX_RECORDS or len(data) != HEADER.size + count * RECORD.size:
        raise ValueError(f"độ dài {len(data)} không khớp count={count}")
    records = []
    for i in range(count):
        f = RECORD.unpack_from(data, HEADER.size + i * RECORD.size)
        records.append({"src": socket.inet_ntoa(f[0]), "dst": socket.inet_ntoa(f[1]),
                        "pkts": f[5], "bytes": f[6], "sport": f[9], "dport": f[10], "proto": f[13]})
    return {"count": count, "seq": seq, "engine": eid}, records


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("port", nargs="?", type=int, default=2055)
    ap.add_argument("--nhan-roi", type=float, default=1.5, help="số giây nhàn rỗi thì kết thúc")
    a = ap.parse_args()
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind((a.host, a.port))
    print(f"[thu-thap] Nghe NetFlow v5 tại UDP {a.host}:{a.port}", flush=True)
    by_src = defaultdict(int)
    by_service = defaultdict(lambda: [0, 0])       # (cổng, giao thức) -> [số luồng, byte]
    expected = {}                                  # (địa chỉ máy xuất, engine) -> seq mong đợi
    ok = bad = flows = lost = total_bytes = 0
    try:
        while True:
            try:
                data, addr = sock.recvfrom(2048)   # gói v5 lớn nhất 24 + 30*48 = 1464 byte
            except socket.timeout:
                print(f"[thu-thap] Nhàn rỗi {a.nhan_roi} s, kết thúc", flush=True)
                break
            sock.settimeout(a.nhan_roi)            # sau gói đầu tiên mới bắt đầu đếm nhàn rỗi
            try:
                head, records = parse_packet(data)
            except ValueError as e:
                bad += 1
                print(f"[thu-thap] Loại gói từ {addr[0]}:{addr[1]}: {e}", flush=True)
                continue
            key = (addr[0], head["engine"])
            if key in expected and head["seq"] != expected[key]:
                gap = head["seq"] - expected[key]
                lost += max(gap, 0)
                print(f"[thu-thap] MẤT GÓI: chờ seq={expected[key]}, nhận seq={head['seq']}, "
                      f"thiếu {gap} luồng", flush=True)
            expected[key] = head["seq"] + head["count"]
            ok += 1
            flows += head["count"]
            print(f"[thu-thap] Gói seq={head['seq']}: {head['count']} luồng", flush=True)
            for r in records:
                by_src[r["src"]] += r["bytes"]
                svc = by_service[(r["dport"], r["proto"])]
                svc[0] += 1
                svc[1] += r["bytes"]
                total_bytes += r["bytes"]
    except KeyboardInterrupt:
        pass
    finally:
        sock.close()
    print(f"[thu-thap] Hợp lệ {ok} gói, loại {bad} gói, mất {lost} luồng", flush=True)
    print(f"[thu-thap] Đã thu {flows} luồng, tổng {total_bytes} byte", flush=True)
    if not total_bytes:
        return
    print("[thu-thap] Nguồn dùng nhiều byte nhất:", flush=True)
    for src, b in sorted(by_src.items(), key=lambda kv: (-kv[1], kv[0]))[:3]:
        print(f"  {src:<12} {b:>9} byte ({b * 100 / total_bytes:.1f}%)", flush=True)
    print("[thu-thap] Dịch vụ theo số luồng:", flush=True)
    for (port, proto), (n, b) in sorted(by_service.items(), key=lambda kv: (-kv[1][0], kv[0])):
        name = f"{port}/{PROTO_NAME.get(proto, proto)}"
        print(f"  {name:<9} {n:>3} luồng {b:>9} byte", flush=True)


if __name__ == "__main__":
    main()
