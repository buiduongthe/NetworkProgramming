"""Bài 4.2 – Client tra cứu thuật ngữ trên UDP, gửi lại có lùi thời gian chờ.

Cài đặt Mã giả 4.4 UDP_REQUEST (Mục 4.3.1, Giáo trình Lập trình mạng).
Kế thừa: Mã giả 4.2 (client UDP có thời gian chờ), vòng đọc thuật ngữ của client tra cứu TCP.
Mỗi yêu cầu mang id; client gửi lại tối đa max_tries lần, thời gian chờ nhân đôi sau mỗi lần,
bỏ qua datagram không đến từ server hoặc không khớp id.

Chạy:  python3 udp_lookup_client.py [host] [port]      (mặc định 127.0.0.1 9100)
"""
import itertools
import json
import os
import socket
import sys
import time

_ids = itertools.count(1)


class NetError(Exception):
    pass


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1 (xem udp_lookup_server.py)."""
    host = os.environ.get("APP_HOST", default_host)
    port = os.environ.get("APP_PORT", default_port)
    if len(args) >= 1:
        host = args[0]
    if len(args) >= 2:
        port = args[1]
    try:
        port = int(port)
    except ValueError:
        sys.exit(f"Cổng phải là số nguyên: {port}")
    if not 1 <= port <= 65535:
        sys.exit(f"Cổng không hợp lệ: {port}")
    return host, port


def now_ms():
    return time.monotonic() * 1000


def udp_request(s, server, req, timeout_ms, max_tries):
    """Mã giả 4.4 – UDP_REQUEST; các dòng in "[client]   ..." là nhật ký minh họa, không có trong mã giả."""
    req["id"] = next(_ids)                                               # dòng 2: NEXT_ID()
    payload = json.dumps(req, ensure_ascii=False, separators=(",", ":")).encode("utf-8")  # dòng 3
    for attempt in range(1, max_tries + 1):                              # dòng 4
        s.sendto(payload, server)                                        # dòng 5
        print(f"[client]   Lần {attempt}: gửi id={req['id']} ({len(payload)} byte), chờ tối đa {timeout_ms} ms",
              flush=True)
        deadline = now_ms() + timeout_ms                                 # dòng 6
        while now_ms() < deadline:                                       # dòng 7
            s.settimeout(max(1, deadline - now_ms()) / 1000)             # dòng 8: phần thời gian còn lại
            try:
                data, addr = s.recvfrom(65535)                           # dòng 10
                resp = json.loads(data.decode("utf-8"))                  # dòng 11
            except (socket.timeout, ConnectionResetError):               # dòng 12–13: CATCH Timeout, ConnectionReset
                break                                                    # (Windows: ICMP port unreachable) → lần thử kế
            except ValueError:                                           # dòng 14–15: không phải JSON: bỏ
                continue
            if addr[:2] == server[:2] and isinstance(resp, dict) and resp.get("id") == req["id"]:
                return resp                                              # dòng 17
            print(f"[client]   Bỏ datagram lạ hoặc muộn từ {addr[0]}:{addr[1]}", flush=True)
        print(f"[client]   Hết {timeout_ms} ms không có phản hồi", flush=True)
        timeout_ms *= 2                                                  # dòng 19: lùi theo cấp số nhân
    raise NetError(f"Không có phản hồi sau {max_tries} lần")             # dòng 21


def main():
    host, port = parse_endpoint(sys.argv[1:], "127.0.0.1", 9100)
    family, _, _, _, server = socket.getaddrinfo(host, port, type=socket.SOCK_DGRAM)[0]
    s = socket.socket(family, socket.SOCK_DGRAM)
    print(f"[client] Gửi yêu cầu tới {host}:{port} qua UDP (chờ 500 ms, tối đa 4 lần). "
          f"Nhập thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc).", flush=True)
    try:
        for line in sys.stdin:                                           # READ_CONSOLE()
            term = line.strip()
            if not term:
                continue
            print(f"[client] Tra cứu '{term}'", flush=True)
            t0 = now_ms()
            try:
                resp = udp_request(s, server, {"op": "lookup", "term": term}, 500, 4)
            except NetError as e:
                print(f"[client] Lỗi: {e} ({now_ms() - t0:.0f} ms)", flush=True)
                continue
            if resp.get("status") == 200:
                print(f"[client] {resp['term']}: {resp['text']} ({now_ms() - t0:.0f} ms)", flush=True)
            else:
                print(f"[client] Lỗi {resp.get('status')}: {resp.get('error')} ({now_ms() - t0:.0f} ms)", flush=True)
    finally:
        s.close()
        print("[client] Đã đóng socket", flush=True)


if __name__ == "__main__":
    main()
