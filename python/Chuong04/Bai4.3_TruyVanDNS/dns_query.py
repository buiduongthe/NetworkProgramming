"""Bài 4.3 – Tự tạo và phân tích truy vấn DNS qua UDP (RFC 1035).

Cài đặt Mã giả 4.5 (Mục 4.3.2, Giáo trình Lập trình mạng): BUILD_QUERY, SKIP_NAME, DNS_QUERY.
Kế thừa: client tra cứu DNS của Mục 2.5 – thay RESOLVE của hệ điều hành bằng truy vấn tự tạo.
Bổ sung so với mã giả (theo đoạn văn sau Mã giả 4.5): kiểm tra mọi chỉ số pos không vượt độ dài
thông điệp và kiểm tra cờ TC trước khi đọc phần trả lời.

Chạy:  python3 dns_query.py <ip_dns_server> <ten_mien> [A|AAAA] [cong]     (mặc định A, cổng 53)
Ví dụ: python3 dns_query.py 8.8.8.8 example.com AAAA
       python3 dns_query.py 2001:4860:4860::8888 example.com A      (máy chủ DNS IPv6)
"""
import ipaddress
import secrets
import socket
import struct
import sys

QTYPES = {"A": 1, "AAAA": 28}
TYPE_NAMES = {1: "A", 28: "AAAA"}
RCODES = {0: "NOERROR", 1: "FORMERR", 2: "SERVFAIL", 3: "NXDOMAIN", 4: "NOTIMP", 5: "REFUSED"}


class ProtocolError(Exception):
    pass


def hexdump(b):
    return " ".join(f"{x:02X}" for x in b)


def build_query(qid, name, qtype):
    """Mã giả 4.5, dòng 1–9."""
    q = struct.pack("!HH", qid, 0x0100)                       # dòng 2: ID, cờ RD = 1
    q += struct.pack("!H", 1) + bytes(6)                      # dòng 3: QD=1, AN=NS=AR=0
    for label in name.rstrip(".").split("."):                 # dòng 4
        b = label.encode("idna")                              # dòng 5: ASCII; tên quốc tế → Punycode
        if not 1 <= len(b) <= 63:
            raise ValueError(f"Nhãn không hợp lệ: '{label}'")
        q += bytes([len(b)]) + b                              # dòng 6
    return q + b"\x00" + struct.pack("!HH", qtype, 1)          # dòng 8: kết thúc tên, QTYPE, QCLASS = IN


def need(m, end):
    """Kiểm tra chỉ số (bổ sung): phản hồi sai định dạng không được làm đọc ngoài thông điệp."""
    if end > len(m):
        raise ProtocolError("Phản hồi sai định dạng: vượt quá độ dài thông điệp")


def skip_name(m, pos):
    """Mã giả 4.5, dòng 10–17: nhảy qua một tên (không giải mã)."""
    while True:
        need(m, pos + 1)
        n = m[pos]
        if n == 0:
            return pos + 1                                    # hết tên
        if n & 0xC0 == 0xC0:
            need(m, pos + 2)
            return pos + 2                                    # con trỏ nén 2 byte
        if n & 0xC0:
            raise ProtocolError("Phản hồi sai định dạng: nhãn kiểu lạ")
        pos += 1 + n


def dns_query(server, name, qtype):
    """Mã giả 4.5, dòng 18–38; server = (ip, cổng); trả về (rcode, answers)."""
    ip = ipaddress.ip_address(server[0])                      # địa chỉ IP dạng số (IPv4 hoặc IPv6)
    family = socket.AF_INET6 if ip.version == 6 else socket.AF_INET
    s = socket.socket(family, socket.SOCK_DGRAM)              # dòng 19: SOCKET(FAMILY(server.ip), DGRAM)
    s.settimeout(2.0)                                         # dòng 20: RECV_TIMEOUT 2000 ms
    qid = secrets.randbits(16)                                # dòng 20: RANDOM_UINT16()
    q = build_query(qid, name, qtype)
    print(f"[client] Truy vấn {TYPE_NAMES[qtype]} cho {name} tới {server[0]}:{server[1]}"
          f" (id=0x{qid:04X}, {len(q)} byte)", flush=True)
    print(f"[client] Truy vấn (hex): {hexdump(q)}", flush=True)
    try:
        s.sendto(q, (str(ip), server[1]))                     # dòng 21
        m, addr = s.recvfrom(4096)                            # dòng 22
    finally:
        s.close()
    if ipaddress.ip_address(addr[0].split("%")[0]) != ip or addr[1] != server[1]:   # dòng 23: addr ≠ server
        raise ProtocolError(f"Phản hồi từ địa chỉ lạ {addr[0]}:{addr[1]}")
    need(m, 12)
    if struct.unpack("!H", m[0:2])[0] != qid:                 # dòng 24
        raise ProtocolError("ID của phản hồi không khớp truy vấn")
    rcode = m[3] & 0x0F                                       # dòng 25: 3 = NXDOMAIN
    ancount = struct.unpack("!H", m[6:8])[0]                  # dòng 26
    print(f"[client] Phản hồi {len(m)} byte từ {addr[0]}:{addr[1]}, RCODE={rcode} ({RCODES.get(rcode, '?')}),"
          f" ANCOUNT={ancount}", flush=True)
    print(f"[client] Phản hồi (hex): {hexdump(m)}", flush=True)
    if m[2] & 0x02:                                           # bổ sung: cờ TC
        raise ProtocolError("Phản hồi bị cắt (TC = 1): cần hỏi lại qua TCP cổng 53")
    pos = skip_name(m, 12) + 4                                # dòng 27: bỏ QTYPE, QCLASS
    answers = []
    for _ in range(ancount):                                  # dòng 29
        pos = skip_name(m, pos)                               # dòng 30
        need(m, pos + 10)
        rtype = struct.unpack("!H", m[pos:pos + 2])[0]        # dòng 31
        rdlen = struct.unpack("!H", m[pos + 8:pos + 10])[0]   # dòng 32
        need(m, pos + 10 + rdlen)
        rdata = m[pos + 10:pos + 10 + rdlen]                  # dòng 33
        if rtype == 1 and rdlen == 4:                         # dòng 34
            answers.append(socket.inet_ntop(socket.AF_INET, rdata))
        if rtype == 28 and rdlen == 16:                       # dòng 35
            answers.append(socket.inet_ntop(socket.AF_INET6, rdata))
        pos += 10 + rdlen                                     # dòng 36
    return rcode, answers


def main():
    if len(sys.argv) < 3:
        print("Cách dùng: python3 dns_query.py <ip_dns_server> <ten_mien> [A|AAAA] [cong]")
        return 1
    server_ip, name = sys.argv[1], sys.argv[2]
    tname = sys.argv[3].upper() if len(sys.argv) > 3 else "A"
    port = int(sys.argv[4]) if len(sys.argv) > 4 else 53
    if tname not in QTYPES:
        print("Kiểu bản ghi chỉ hỗ trợ A hoặc AAAA")
        return 1
    try:
        rcode, answers = dns_query((server_ip, port), name, QTYPES[tname])
    except socket.timeout:
        print("[client] Lỗi: hết thời gian chờ phản hồi (2000 ms)", flush=True)
        return 1
    except ProtocolError as e:
        print(f"[client] Lỗi giao thức: {e}", flush=True)
        return 1
    for a in answers:
        print(f"[client] Địa chỉ: {a}", flush=True)
    if not answers:
        print(f"[client] Không có bản ghi {tname}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
