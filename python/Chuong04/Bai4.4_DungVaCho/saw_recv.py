"""Bài 4.4 – Bên nhận của giao thức dừng và chờ trên UDP.

Cài đặt Mã giả 4.7 SAW_RECEIVE (Mục 4.4.1). Kế thừa: Mã giả 4.1 (vòng RECVFROM/SENDTO).
Luôn gửi ACK cho datagram nhận được; datagram trùng bị bỏ dữ liệu nhưng vẫn được ACK;
sau FIN chờ thêm 2 × MAX_RTO (như TIME-WAIT của TCP) để trả lời FIN lặp lại khi ACK bị mất.

Chạy:  python3 saw_recv.py [host] [port] [tep_ra] [mat_goi]   (mặc định 0.0.0.0 9000 tep_nhan.bin, không mất gói)
       mat_goi: "0.1" (ngẫu nhiên 10%) hoặc "4,7" (mất ACK gửi đi thứ 4 và 7)
"""
import socket
import struct
import sys

from saw_common import ACK, FIN, MAX_RTO, LossSim, max_rto_note, parse_endpoint, sha256_file


def saw_receive(s, out, loss):
    """Mã giả 4.7 – SAW_RECEIVE; trả về (số byte đã ghi, số datagram trùng)."""
    expected, written, dups = 0, 0, 0                                # dòng 2
    while True:                                                      # dòng 3: LOOP
        try:
            d, peer = s.recvfrom(65535)                              # dòng 5
        except socket.timeout:                                       # dòng 6: chỉ xảy ra sau FIN
            print(f"[nhận] Hết {2 * MAX_RTO} ms không có datagram mới: kết thúc", flush=True)
            return written, dups                                     # dòng 7
        except ConnectionResetError:                                 # Windows: ICMP port unreachable
            continue
        if len(d) < 5:                                               # dòng 9: datagram lỗi: bỏ
            continue
        seq, typ = struct.unpack("!IB", d[:5])                       # dòng 10
        dropped = loss.drop()
        if not dropped:
            s.sendto(struct.pack("!IB", seq, ACK), peer)             # dòng 11: luôn ACK
        note = " – mô phỏng mất ACK" if dropped else ""
        if seq == expected and typ == FIN:                           # dòng 12
            s.settimeout(2 * MAX_RTO / 1000)                         # dòng 13: như TIME-WAIT
            print(f"[nhận] seq={seq} FIN: gửi ACK {seq}{note}; chờ {2 * MAX_RTO} ms (như TIME-WAIT)", flush=True)
        elif seq == expected:                                        # dòng 14
            out.write(d[5:])                                         # dòng 15
            written += len(d) - 5
            expected += 1                                            # dòng 16
            print(f"[nhận] seq={seq} DATA {len(d) - 5} byte: ghi tệp, gửi ACK {seq}{note}", flush=True)
        else:                                                        # seq < expected: trùng
            dups += 1
            print(f"[nhận] seq={seq} trùng: bỏ dữ liệu, vẫn gửi ACK {seq}{note}", flush=True)


def main():
    host, port = parse_endpoint(sys.argv[1:3], "0.0.0.0", 9000)
    path = sys.argv[3] if len(sys.argv) > 3 else "tep_nhan.bin"
    loss = LossSim(sys.argv[4] if len(sys.argv) > 4 else "")
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind((host, port))
    print(f"[nhận] Đang chờ dữ liệu UDP tại {host}:{port}, ghi vào {path}", flush=True)
    print(f"[nhận] Mô phỏng mất ACK: {loss.spec or 'không'}", flush=True)
    print(f"[nhận] {max_rto_note()}", flush=True)
    try:
        with open(path, "wb") as out:
            written, dups = saw_receive(s, out, loss)
    finally:
        s.close()
    print(f"[nhận] Hoàn tất: {written} byte, {dups} datagram trùng; SHA-256:", flush=True)
    print(f"[nhận]   {sha256_file(path)}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
