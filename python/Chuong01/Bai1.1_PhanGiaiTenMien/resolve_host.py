"""Bài 1.1 – Phân giải tên miền thành danh sách địa chỉ.

Cài đặt Mã giả 1.1 – RESOLVE_HOST (Mục 1.2.3, Giáo trình Lập trình mạng).
Với mỗi tên nhận từ dòng lệnh, chương trình gọi getaddrinfo của hệ điều hành,
in từng địa chỉ kèm họ địa chỉ (INET/INET6); lỗi phân giải được bắt và trả danh sách rỗng.

Chạy:  python3 resolve_host.py [tên ...]      (mặc định: localhost)
Ví dụ: python3 resolve_host.py localhost example.com
"""
import socket
import sys


def family_name(family):
    """FAMILY(a): đổi hằng họ địa chỉ thành tên dùng trong mã giả."""
    return {socket.AF_INET: "INET", socket.AF_INET6: "INET6"}.get(family, str(family))


def resolve_host(name):
    """Mã giả 1.1 – RESOLVE_HOST(name): trả về danh sách (họ, địa chỉ); rỗng nếu lỗi."""
    try:
        # RESOLVE(name): gọi getaddrinfo; type=SOCK_STREAM để mỗi địa chỉ chỉ xuất hiện một lần
        infos = socket.getaddrinfo(name, None, type=socket.SOCK_STREAM)    # dòng 3
        addrs = [(family, sockaddr[0]) for family, _, _, _, sockaddr in infos]
        for family, address in addrs:                                       # FOR EACH a IN addrs
            print(f"  {family_name(family):<6} {address}", flush=True)     # PRINT(FAMILY(a), ADDRESS(a))
        return addrs                                                        # RETURN addrs
    except socket.gaierror as e:                                            # CATCH ResolveError AS e
        print(f"[resolve] Không phân giải được {name} (mã lỗi {e.errno}: {e.strerror})", flush=True)
        return []                                                           # RETURN EMPTY_LIST


def main():
    names = sys.argv[1:] or ["localhost"]
    for name in names:
        print(f"[resolve] Phân giải tên: {name}", flush=True)
        addrs = resolve_host(name)
        print(f"[resolve] {name} -> {len(addrs)} địa chỉ", flush=True)


if __name__ == "__main__":
    main()
