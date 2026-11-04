"""Bài 1.2 – Kiểm tra trạng thái một cổng TCP.

Cài đặt Mã giả 1.2 – PORT_CHECK (Mục 1.2.5, Giáo trình Lập trình mạng),
kế thừa Mã giả 1.1 – RESOLVE_HOST (Mục 1.2.3).
Kết quả là một trong bốn trạng thái: OPEN, CLOSED, FILTERED, UNRESOLVED.
Chỉ kiểm tra trên máy và mạng thuộc quyền quản lý của người thực hiện.

Chạy:  python3 port_check.py <host> <port> [timeout_ms]      (mặc định timeout_ms = 2000)
Ví dụ: python3 port_check.py localhost 9000
"""
import errno
import socket
import sys

UNREACHABLE = (errno.ENETUNREACH, errno.EHOSTUNREACH)     # NetworkUnreachable, HostUnreachable


def family_name(family):
    """FAMILY(a): đổi hằng họ địa chỉ thành tên dùng trong mã giả."""
    return {socket.AF_INET: "INET", socket.AF_INET6: "INET6"}.get(family, str(family))


def resolve_host(name):
    """Mã giả 1.1 – RESOLVE_HOST(name) (chép từ Bài 1.1): danh sách (họ, địa chỉ); rỗng nếu lỗi."""
    try:
        infos = socket.getaddrinfo(name, None, type=socket.SOCK_STREAM)   # RESOLVE(name)
        addrs = [(family, sockaddr[0]) for family, _, _, _, sockaddr in infos]
        for family, address in addrs:
            print(f"  {family_name(family):<6} {address}", flush=True)    # PRINT(FAMILY(a), ADDRESS(a))
        return addrs
    except socket.gaierror as e:                                           # CATCH ResolveError AS e
        print(f"[port-check] Không phân giải được {name} (mã lỗi {e.errno}: {e.strerror})", flush=True)
        return []                                                          # RETURN EMPTY_LIST


def port_check(host, port, timeout_ms):
    """Mã giả 1.2 – PORT_CHECK(host, port, timeout_ms)."""
    addrs = resolve_host(host)                                  # dòng 2: lỗi phân giải → rỗng
    if len(addrs) == 0:                                         # dòng 3
        return "UNRESOLVED"
    state = "FILTERED"                                          # dòng 4: chưa địa chỉ nào phản hồi
    for family, address in addrs:                               # dòng 5: FOR EACH a IN addrs
        s = socket.socket(family, socket.SOCK_STREAM)           # dòng 6: SOCKET(FAMILY(a), STREAM)
        s.settimeout(timeout_ms / 1000)                         # dòng 7: SETOPT(CONNECT_TIMEOUT)
        try:
            s.connect((address, port))                          # dòng 9: bắt tay ba bước
            print(f"[port-check] {address} cổng {port}: kết nối thành công", flush=True)
            return "OPEN"                                       # dòng 10
        except ConnectionRefusedError:                          # dòng 11: CATCH ConnectionRefused
            print(f"[port-check] {address} cổng {port}: bị từ chối (RST)", flush=True)
            state = "CLOSED"                                    # dòng 12
        except TimeoutError:                                    # dòng 13: CATCH Timeout
            print(f"[port-check] {address} cổng {port}: hết thời gian chờ", flush=True)
            continue                                            # dòng 14
        except OSError as e:                                    # dòng 13: NetworkUnreachable, HostUnreachable
            if e.errno not in UNREACHABLE:
                raise                                           # lỗi khác: để thoát ra như mã giả
            print(f"[port-check] {address} cổng {port}: không có đường đi ({e.strerror})", flush=True)
            continue
        finally:
            s.close()                                           # dòng 16: FINALLY CLOSE(s)
    return state                                                # dòng 19: CLOSED hoặc FILTERED


def main():
    if len(sys.argv) < 3:
        sys.exit("Cách dùng: python3 port_check.py <host> <port> [timeout_ms]")
    host, port = sys.argv[1], int(sys.argv[2])
    timeout_ms = int(sys.argv[3]) if len(sys.argv) > 3 else 2000
    print(f"[port-check] Kiểm tra {host}:{port} (thời gian chờ {timeout_ms} ms)", flush=True)
    state = port_check(host, port, timeout_ms)
    print(f"[port-check] Kết quả: {host}:{port} -> {state}", flush=True)


if __name__ == "__main__":
    main()
