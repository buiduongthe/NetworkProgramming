"""Bài 7.7 – Gửi ICMP Echo Request và đo thời gian khứ hồi (socket RAW).

Cài đặt Mã giả 7.8 (Mục 7.5.3, Giáo trình Lập trình mạng): INET_CHECKSUM (RFC 1071) và PING.
Kế thừa: Mã giả 4.4 – cùng mẫu gửi rồi chờ phản hồi có hạn giờ, bỏ qua gói không khớp.
Socket RAW cần quyền quản trị: Linux/macOS chạy bằng sudo, Windows mở cửa sổ lệnh "Run as administrator".

Chạy:  sudo python3 ping.py [host] [số_lần] [thời_gian_chờ_ms]   (mặc định 127.0.0.1 4 1000)
"""
import os
import socket
import struct
import sys
import time


def inet_checksum(b):
    """Mã giả 7.8, dòng 1–11: tổng kiểm tra Internet (RFC 1071)."""
    if len(b) % 2 == 1:
        b += b"\0"
    s = 0
    for i in range(0, len(b), 2):
        s += (b[i] << 8) | b[i + 1]                       # FROM_UINT16_BE(b[i..i+1])
    while s > 0xFFFF:
        s = (s & 0xFFFF) + (s >> 16)                      # cộng phần tràn
    return ~s & 0xFFFF


def ping(host, seq, timeout_ms):
    """Mã giả 7.8, dòng 13–35: trả về (địa chỉ, RTT giây, TTL) hoặc ném TimeoutError."""
    dst = next(a[4][0] for a in socket.getaddrinfo(host, None, socket.AF_INET))   # FIRST_IPV4(RESOLVE(host))
    s = socket.socket(socket.AF_INET, socket.SOCK_RAW, socket.IPPROTO_ICMP)       # cần quyền quản trị
    ident = os.getpid() & 0xFFFF
    p = struct.pack("!BBHHH", 8, 0, 0, ident, seq) + os.urandom(32)            # kiểu 8, mã 0, checksum 0
    p = p[:2] + struct.pack("!H", inet_checksum(p)) + p[4:]                     # điền tổng kiểm tra
    if seq == 1:
        print(f"[ping] Tiêu đề ICMP gửi đi: {p[:8].hex(' ').upper()} (kiểu, mã, checksum, id, seq)", flush=True)
    try:
        t0 = time.perf_counter()
        s.sendto(p, (dst, 0))
        deadline = time.monotonic() + timeout_ms / 1000   # dòng 21: hạn chót chung
        while True:                                       # bỏ qua gói ICMP khác
            s.settimeout(max(0.001, deadline - time.monotonic()))   # dòng 23: MAX(1 ms, phần còn lại)
            d, addr = s.recvfrom(1500)                    # RAW: có cả tiêu đề IP
            ihl = (d[0] & 0x0F) * 4                       # độ dài tiêu đề IPv4
            m = d[ihl:]
            if m[0] == 0 and struct.unpack("!H", m[4:6])[0] == ident and struct.unpack("!H", m[6:8])[0] == seq:
                return addr[0], time.perf_counter() - t0, d[8]                  # d[8]: TTL
            print(f"[ping]   bỏ qua gói ICMP kiểu {m[0]} (không phải Echo Reply của tiến trình này)", flush=True)
    finally:
        s.close()


def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "127.0.0.1"
    count = int(sys.argv[2]) if len(sys.argv) >= 3 else 4
    timeout_ms = int(sys.argv[3]) if len(sys.argv) >= 4 else 1000
    print(f"[ping] PING {host}: {count} gói Echo Request, dữ liệu 32 byte, chờ tối đa {timeout_ms} ms", flush=True)
    rtts = []
    for seq in range(1, count + 1):
        try:
            addr, rtt, ttl = ping(host, seq, timeout_ms)
            rtts.append(rtt * 1000)
            print(f"[ping] Trả lời từ {addr}: seq={seq} ttl={ttl} thời gian={rtt * 1000:.3f} ms", flush=True)
        except (TimeoutError, socket.timeout):
            print(f"[ping] seq={seq}: hết thời gian chờ", flush=True)
        except PermissionError:
            sys.exit("[ping] Socket RAW cần quyền quản trị (sudo hoặc Run as administrator)")
        if seq < count:
            time.sleep(1)
    lost = 100 * (count - len(rtts)) // count
    avg = f"{sum(rtts) / len(rtts):.3f} ms" if rtts else "không có"
    print(f"[ping] Đã gửi {count}, nhận {len(rtts)}, mất {lost}%; RTT trung bình {avg}", flush=True)


if __name__ == "__main__":
    main()
