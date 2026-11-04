"""Bài 4.3 – DNS server giả lập tối giản, chỉ dùng để chạy thử client DNS tự tạo (Mã giả 4.5).

Không phải mã giả của giáo trình. Server trả lời cho đúng một tên miền:
  demo.ltm.local  A     192.0.2.10      (địa chỉ dành cho tài liệu, RFC 5737)
  demo.ltm.local  AAAA  2001:db8::10    (địa chỉ dành cho tài liệu, RFC 3849)
Tên khác: RCODE = 3 (NXDOMAIN). Datagram sai định dạng: bỏ qua, không trả lời.
Tên trong phần trả lời được nén bằng con trỏ C0 0C (trỏ tới QNAME ở vị trí 12).
Dùng chung cho demo của cả ba bản C#, Java, Python.

Chạy:  python3 fake_dns_server.py [host] [port]      (mặc định 127.0.0.1 5353)
"""
import socket
import struct
import sys

NAME = "demo.ltm.local"
RECORDS = {1: socket.inet_pton(socket.AF_INET, "192.0.2.10"),        # TYPE 1 = A
           28: socket.inet_pton(socket.AF_INET6, "2001:db8::10")}    # TYPE 28 = AAAA
TYPE_NAMES = {1: "A", 28: "AAAA"}


def parse_question(m):
    """Trả về (id, cờ, tên, qtype, vị trí kết thúc câu hỏi); ném ValueError nếu sai định dạng."""
    if len(m) < 12:
        raise ValueError("ngắn hơn tiêu đề")
    qid, flags, qdcount = struct.unpack("!HHH", m[:6])
    if qdcount != 1:
        raise ValueError("QDCOUNT khác 1")
    labels, pos = [], 12
    while True:
        if pos >= len(m):
            raise ValueError("tên không kết thúc")
        n = m[pos]
        if n == 0:
            pos += 1
            break
        if n > 63 or pos + 1 + n > len(m):
            raise ValueError("nhãn sai")
        labels.append(m[pos + 1:pos + 1 + n].decode("ascii"))
        pos += 1 + n
    if pos + 4 > len(m):
        raise ValueError("thiếu QTYPE/QCLASS")
    qtype, _ = struct.unpack("!HH", m[pos:pos + 4])
    return qid, flags, ".".join(labels).lower(), qtype, pos + 4


def build_response(m):
    qid, flags, name, qtype, qend = parse_question(m)
    answers = []
    if name == NAME and qtype in RECORDS:
        rdata = RECORDS[qtype]
        answers.append(b"\xc0\x0c" + struct.pack("!HHIH", qtype, 1, 300, len(rdata)) + rdata)
    rcode = 0 if name == NAME else 3                                   # 3 = NXDOMAIN
    rflags = 0x8000 | 0x0400 | (flags & 0x0100) | 0x0080 | rcode       # QR, AA, RD (chép lại), RA, RCODE
    header = struct.pack("!HHHHHH", qid, rflags, 1, len(answers), 0, 0)
    return header + m[12:qend] + b"".join(answers), qid, name, qtype, rcode, len(answers)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 5353
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind((host, port))
    print(f"[dns-giả] Đang lắng nghe UDP tại {host}:{port}; {NAME} -> 192.0.2.10, 2001:db8::10", flush=True)
    try:
        while True:
            m, addr = s.recvfrom(4096)
            try:
                out, qid, name, qtype, rcode, an = build_response(m)
            except (ValueError, UnicodeDecodeError) as e:
                print(f"[dns-giả] Bỏ datagram sai định dạng từ {addr[0]}:{addr[1]}: {e}", flush=True)
                continue
            s.sendto(out, addr)
            print(f"[dns-giả] Truy vấn id=0x{qid:04X} {name} {TYPE_NAMES.get(qtype, qtype)} từ {addr[0]}:{addr[1]}"
                  f" -> RCODE={rcode}, {an} bản ghi", flush=True)
    except KeyboardInterrupt:
        print("[dns-giả] Dừng server", flush=True)
    finally:
        s.close()


if __name__ == "__main__":
    main()
