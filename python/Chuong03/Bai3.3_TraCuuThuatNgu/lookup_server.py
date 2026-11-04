"""Bài 3.3 – Server tra cứu thuật ngữ dùng khung JSON.

Cài đặt Mã giả 3.7 (Mục 3.5.1, Giáo trình Lập trình mạng) theo đặc tả giao thức dịch vụ tra cứu:
TCP cổng 9100, khung tiền tố độ dài 4 byte, nội dung JSON UTF-8.
Kế thừa: khung sườn ECHO_SERVER (Mã giả 3.1) với HANDLE_ECHO thay bằng HANDLE_LOOKUP;
SEND_JSON/RECV_JSON (Mã giả 3.6, tệp framing.py).

Chạy:  python3 lookup_server.py [host] [port]      (mặc định 0.0.0.0 9100; từ điển glossary.json)
"""
import json
import socket
import sys

from framing import ConnectionClosed, JsonError, ProtocolError, recv_json, send_json
from netutil import parse_endpoint

with open("glossary.json", encoding="utf-8") as f:    # DICT ← LOAD_JSON_FILE("glossary.json")
    DICT = json.load(f)


def process_request(req):
    """Nghiệp vụ, độc lập với socket: nhận đối tượng yêu cầu, trả đối tượng phản hồi."""
    op = req.get("op")                                        # req luôn là đối tượng (dòng 24–27)
    if op == "lookup":
        term = req.get("term")
        if isinstance(term, str) and term in DICT:
            return {"status": 200, "term": term, "text": DICT[term]}
        return {"status": 404, "error": "Không có thuật ngữ"}
    if op == "list":
        return {"status": 200, "terms": list(DICT.keys())}
    return {"status": 400, "error": "Sai thao tác"}           # OTHERWISE


def handle_lookup(conn, addr):
    while True:
        try:
            req = recv_json(conn)
        except ConnectionClosed as e:
            if e.received == 0:
                return                                        # đóng tại ranh giới
            raise                                             # đóng giữa khung: lỗi
        except JsonError:
            send_json(conn, {"status": 400, "error": "JSON sai"})
            print(f"[server] {addr}: (JSON sai) -> 400", flush=True)
            continue
        if not isinstance(req, dict):                         # dòng 24–27: vd. [1, 2] hoặc "abc"
            send_json(conn, {"status": 400, "error": "JSON sai"})
            print(f"[server] {addr}: (không phải đối tượng) -> 400", flush=True)
            continue
        if req.get("op") == "bye":
            return
        resp = process_request(req)
        send_json(conn, resp)
        op = req.get("op")
        op = op if isinstance(op, str) else "?"               # chỉ để ghi nhật ký
        term = f" '{req.get('term')}'" if op == "lookup" else ""
        print(f"[server] {addr}: {op}{term} -> {resp['status']}", flush=True)


def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9100)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)      # SOCKET(INET, STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)    # SETOPT(REUSEADDR)
    srv.bind((host, port))                                        # BIND
    srv.listen(16)                                                # LISTEN, backlog 16
    print(f"[server] Đang lắng nghe tại {host}:{port} ({len(DICT)} thuật ngữ)", flush=True)
    try:
        while True:                                               # vòng chấp nhận kết nối
            conn, (ip, cport) = srv.accept()                      # ACCEPT
            addr = f"{ip}:{cport}"
            print(f"[server] Client kết nối: {addr}", flush=True)
            try:
                handle_lookup(conn, addr)
            except (OSError, ConnectionClosed, ProtocolError) as e:   # NetError, ConnectionClosed, ProtocolError
                print(f"[server] Lỗi với {addr}: {e}", flush=True)    # không dừng server
            finally:
                conn.close()                                      # CLOSE(conn)
                print(f"[server] Client {addr} đã đóng kết nối", flush=True)
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
