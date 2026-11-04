"""Bài 4.5 – Client tra cứu UDP có xử lý mã thách thức (cookie) của server.

Theo đoạn văn sau Mã giả 4.8 (Mục 4.6.1): khi nhận phản hồi 401 kèm cookie, client lưu mã và gửi lại
yêu cầu có trường cookie; các yêu cầu sau dùng lại mã đó. Kế thừa: Mã giả 4.4 UDP_REQUEST.

Chạy:  python3 udp_cookie_client.py [host] [port] [cookie_ban_dau]     (mặc định 127.0.0.1 9100)
       cookie_ban_dau (thêm cho bài thực hành): mô phỏng client dùng cookie giả mạo hoặc của địa chỉ khác.
"""
import itertools
import json
import os
import socket
import sys
import time

_ids = itertools.count(1)
MIN_FIRST = 48      # byte; yêu cầu chưa có cookie được đệm khoảng trắng tới độ dài này để thách thức 401
                    # (khoảng 100 byte) không vượt MAX_RATIO = 3 lần yêu cầu (đoạn văn sau Mã giả 4.8)


class NetError(Exception):
    pass


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1 (xem udp_cookie_server.py)."""
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


def udp_request(s, server, req, timeout_ms, max_tries, pad_to=0):
    """Mã giả 4.4 – UDP_REQUEST (chép từ Bài 4.2); dòng "[client]   ..." là nhật ký minh họa.
    pad_to (bổ sung): đệm khoảng trắng sau JSON tới pad_to byte – JSON hợp lệ cho phép khoảng trắng cuối."""
    req["id"] = next(_ids)
    payload = json.dumps(req, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    payload += b" " * max(0, pad_to - len(payload))
    for attempt in range(1, max_tries + 1):
        s.sendto(payload, server)
        print(f"[client]   Lần {attempt}: gửi id={req['id']} ({len(payload)} byte), chờ tối đa {timeout_ms} ms",
              flush=True)
        deadline = now_ms() + timeout_ms
        while now_ms() < deadline:
            s.settimeout(max(1, deadline - now_ms()) / 1000)
            try:
                data, addr = s.recvfrom(65535)
                resp = json.loads(data.decode("utf-8"))
            except (socket.timeout, ConnectionResetError):             # dòng 12–13: Timeout, ConnectionReset
                break
            except ValueError:                                         # dòng 14–15: không phải JSON: bỏ
                continue
            if addr[:2] == server[:2] and isinstance(resp, dict) and resp.get("id") == req["id"]:
                return resp
            print(f"[client]   Bỏ datagram lạ hoặc muộn từ {addr[0]}:{addr[1]}", flush=True)
        print(f"[client]   Hết {timeout_ms} ms không có phản hồi", flush=True)
        timeout_ms *= 2
    raise NetError(f"Không có phản hồi sau {max_tries} lần")


def lookup(s, server, term, cookie):
    req = {"op": "lookup", "term": term}
    if cookie:
        req["cookie"] = cookie                                       # gửi kèm cookie đã lưu
        return udp_request(s, server, req, 500, 4)
    return udp_request(s, server, req, 500, 4, MIN_FIRST)           # chưa có cookie: đệm yêu cầu


def main():
    host, port = parse_endpoint(sys.argv[1:3], "127.0.0.1", 9100)
    cookie = sys.argv[3] if len(sys.argv) > 3 else None
    family, _, _, _, server = socket.getaddrinfo(host, port, type=socket.SOCK_DGRAM)[0]
    s = socket.socket(family, socket.SOCK_DGRAM)
    print(f"[client] Gửi yêu cầu tới {host}:{port} qua UDP. Nhập thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc).", flush=True)
    try:
        for line in sys.stdin:
            term = line.strip()
            if not term:
                continue
            print(f"[client] Tra cứu '{term}' ({'kèm cookie' if cookie else 'không kèm cookie'})", flush=True)
            try:
                resp = lookup(s, server, term, cookie)
                if resp.get("status") == 401 and isinstance(resp.get("cookie"), str):
                    cookie = resp["cookie"]                          # lưu mã thách thức
                    print(f"[client] Nhận thách thức 401, lưu cookie {cookie[:16]}…; gửi lại kèm cookie", flush=True)
                    resp = lookup(s, server, term, cookie)           # gửi lại đúng một lần
            except NetError as e:
                print(f"[client] Lỗi: {e}", flush=True)
                continue
            if resp.get("status") == 200:
                print(f"[client] {resp['term']}: {resp['text']}", flush=True)
            else:
                print(f"[client] Lỗi {resp.get('status')}: {resp.get('error', 'không được phục vụ')}", flush=True)
    finally:
        s.close()
        print("[client] Đã đóng socket", flush=True)


if __name__ == "__main__":
    main()
