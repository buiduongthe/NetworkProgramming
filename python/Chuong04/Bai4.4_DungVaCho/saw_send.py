"""Bài 4.4 – Bên gửi của giao thức dừng và chờ (stop-and-wait ARQ) trên UDP.

Cài đặt Mã giả 4.6 SAW_SEND (Mục 4.4.1) với RTO ước lượng theo RFC 6298 (Mục 4.4.2):
SRTT, RTTVAR, RTO = SRTT + max(G, K·RTTVAR); nhân đôi RTO khi hết thời gian chờ; thuật toán Karn.
Kế thừa: Mã giả 4.4 (vòng chờ phản hồi đúng địa chỉ, đúng mã định danh trong phần thời gian còn lại).

Chạy:  python3 saw_send.py <tep> [host] [port] [mat_goi]      (mặc định 127.0.0.1 9000, không mất gói)
       mat_goi: "0.1" (ngẫu nhiên 10%) hoặc "3,7" (mất datagram DATA/FIN gửi đi thứ 3 và 7)
"""
import os
import socket
import struct
import sys
import time

from saw_common import ACK, CHUNK, DATA, FIN, MAX_TRIES, RTO_INIT, MAX_RTO, RTO_MIN, LossSim, max_rto_note, parse_endpoint, sha256_file

FIN_MARK = None


class NetError(Exception):
    pass


def now_ms():
    return time.monotonic() * 1000


class RttEstimator:
    """RTO() / UPDATE_RTT() / BACKOFF_RTO() theo RFC 6298 (Mục 4.4.2)."""
    ALPHA, BETA, K, G = 1 / 8, 1 / 4, 4, 1.0                         # G: độ phân giải đồng hồ 1 ms

    def __init__(self):
        self.srtt, self.rttvar, self.rto = None, None, RTO_INIT

    def update(self, r):                                             # UPDATE_RTT(R')
        if self.srtt is None:
            self.srtt, self.rttvar = r, r / 2                        # mẫu đầu tiên
        else:
            self.rttvar = (1 - self.BETA) * self.rttvar + self.BETA * abs(self.srtt - r)  # tính trước, với SRTT cũ
            self.srtt = (1 - self.ALPHA) * self.srtt + self.ALPHA * r
        self.rto = min(MAX_RTO, max(RTO_MIN, self.srtt + max(self.G, self.K * self.rttvar)))

    def backoff(self):                                               # BACKOFF_RTO(): RTO ← MIN(2 × RTO, MAX_RTO)
        self.rto = min(MAX_RTO, 2 * self.rto)


def wait_ack(s, peer, seq, rto_ms):
    """WAIT_ACK: chờ ACK mang đúng seq từ đúng peer trong rto_ms; bỏ ACK cũ, ACK lạ; hết giờ → socket.timeout."""
    deadline = now_ms() + rto_ms
    while True:
        remaining = deadline - now_ms()
        if remaining <= 0:
            raise socket.timeout()
        s.settimeout(max(1, remaining) / 1000)
        try:
            d, addr = s.recvfrom(65535)
        except ConnectionResetError:                                 # Windows: ICMP port unreachable – xử lý
            raise socket.timeout()                                   # như hết giờ (Mã giả 4.4, dòng 12–13)
        if addr[:2] == peer[:2] and len(d) >= 5 and d[4] == ACK and struct.unpack("!I", d[:4])[0] == seq:
            return


def saw_send(s, peer, chunks, est, loss):
    """Mã giả 4.6 – SAW_SEND; trả về (số datagram đã gửi, số lần gửi lại)."""
    seq, sent, retrans = 0, 0, 0                                     # dòng 3
    for c in chunks + [FIN_MARK]:                                    # dòng 4: APPEND(chunks, FIN_MARK)
        typ = FIN if c is FIN_MARK else DATA                         # dòng 5
        body = b"" if c is FIN_MARK else c
        tries = 0                                                    # dòng 6
        while True:                                                  # dòng 7: LOOP
            dgram = struct.pack("!IB", seq, typ) + body              # UINT32_BE(seq) + BYTE(type) + c
            dropped = loss.drop()
            if not dropped:
                s.sendto(dgram, peer)                                # dòng 8
            sent += 1
            print(f"[gửi] seq={seq} {'FIN' if typ == FIN else 'DATA'} {len(dgram)} byte, RTO {est.rto:.0f} ms"
                  + (" – mô phỏng mất gói" if dropped else ""), flush=True)
            t0 = now_ms()                                            # dòng 9
            try:
                wait_ack(s, peer, seq, est.rto)                      # dòng 11
                if tries == 0:
                    est.update(now_ms() - t0)                        # dòng 12: Karn – chỉ đo khi chưa gửi lại
                break                                                # dòng 13
            except socket.timeout:                                   # dòng 14
                tries += 1
                retrans += 1
                print(f"[gửi] Hết {est.rto:.0f} ms chờ ACK {seq}, gửi lại (lần {tries})", flush=True)
                est.backoff()                                        # dòng 16
                if tries > MAX_TRIES:                                # dòng 17
                    raise NetError(f"Không nhận được ACK {seq} sau {MAX_TRIES} lần gửi lại")
        seq += 1                                                     # dòng 20
    return sent, retrans


def main():
    if len(sys.argv) < 2:
        print("Cách dùng: python3 saw_send.py <tep> [host] [port] [mat_goi]")
        return 1
    path = sys.argv[1]
    host, port = parse_endpoint(sys.argv[2:4], "127.0.0.1", 9000)
    loss = LossSim(sys.argv[4] if len(sys.argv) > 4 else "")
    with open(path, "rb") as f:
        data = f.read()
    chunks = [data[i:i + CHUNK] for i in range(0, len(data), CHUNK)]     # các khối ≤ 1195 byte
    print(f"[gửi] Tệp {os.path.basename(path)}: {len(data)} byte, {len(chunks)} khối; SHA-256:", flush=True)
    print(f"[gửi]   {sha256_file(path)}", flush=True)
    print(f"[gửi] Gửi tới {host}:{port}; mô phỏng mất DATA/FIN: {loss.spec or 'không'}", flush=True)
    print(f"[gửi] {max_rto_note()}", flush=True)
    family, _, _, _, peer = socket.getaddrinfo(host, port, type=socket.SOCK_DGRAM)[0]
    s = socket.socket(family, socket.SOCK_DGRAM)
    t0 = now_ms()
    try:
        sent, retrans = saw_send(s, peer, chunks, RttEstimator(), loss)
    except NetError as e:
        print(f"[gửi] Lỗi: {e}", flush=True)
        return 1
    finally:
        s.close()
    print(f"[gửi] Hoàn tất: {sent} datagram đã gửi ({retrans} lần gửi lại), {now_ms() - t0:.0f} ms", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
