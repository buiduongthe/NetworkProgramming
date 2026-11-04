"""Bài 2.3 – Client tra cứu DNS dùng bộ phân giải của hệ điều hành.

Cài đặt Mã giả 2.3 (Mục 2.5, Giáo trình Lập trình mạng), kế thừa Mã giả 1.1 RESOLVE_HOST.
Mỗi tham số là một tên miền (tra cứu xuôi: tên → địa chỉ IPv4/IPv6)
hoặc một địa chỉ IP (tra cứu ngược: địa chỉ → tên, bản ghi PTR).

Chạy:  python3 dns_client.py <tên miền | địa chỉ IP> ...
Ví dụ: python3 dns_client.py localhost 127.0.0.1 8.8.8.8
"""
import ipaddress
import socket
import sys
import time

FAMILY_NAME = {socket.AF_INET: "INET", socket.AF_INET6: "INET6"}


def resolve_host(name):
    """Mã giả 1.1 – RESOLVE_HOST: in và trả về danh sách địa chỉ; lỗi → danh sách rỗng."""
    try:
        # RESOLVE = getaddrinfo; chỉ lấy SOCK_STREAM để mỗi địa chỉ xuất hiện một lần
        addrs = socket.getaddrinfo(name, None, type=socket.SOCK_STREAM)
        for family, _, _, _, sockaddr in addrs:
            print(f"  {FAMILY_NAME.get(family, family)}  {sockaddr[0]}", flush=True)  # FAMILY, ADDRESS
        return addrs
    except socket.gaierror as e:                                  # ResolveError
        print(f"  Không phân giải được {name} (mã {e.errno}: {e.strerror})", flush=True)
        return []


def is_ip_address(q):
    """IS_IP_ADDRESS: chuỗi là địa chỉ IPv4/IPv6 dạng số hay là tên miền."""
    try:
        ipaddress.ip_address(q)
        return True
    except ValueError:
        return False


def main(args):
    if len(args) == 0:                                            # dòng 1–5
        print("Cách dùng: python3 dns_client.py <tên miền | địa chỉ IP> ...", flush=True)
        return
    for q in args:                                                # dòng 6
        t0 = time.perf_counter()                                  # dòng 7: t0 ← NOW()
        if is_ip_address(q):                                      # dòng 8
            print(f"[dns] Tra cứu ngược: {q}", flush=True)
            try:
                name = socket.gethostbyaddr(q)[0]                 # REVERSE_RESOLVE: bản ghi PTR
                print(f"  {q} → {name}", flush=True)
            except (socket.herror, socket.gaierror):              # ResolveError
                print(f"  {q} → không có bản ghi PTR", flush=True)
        else:
            print(f"[dns] Tra cứu xuôi: {q}", flush=True)
            resolve_host(q)                                       # = Mã giả 1.1
        ms = (time.perf_counter() - t0) * 1000
        print(f"  Thời gian: {ms:.2f} ms", flush=True)            # dòng 18


if __name__ == "__main__":
    main(sys.argv[1:])
