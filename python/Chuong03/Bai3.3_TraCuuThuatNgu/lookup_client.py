"""Bài 3.3 – Client tra cứu thuật ngữ.

Cài đặt Mã giả 3.8 (Mục 3.5.1, Giáo trình Lập trình mạng): giữ phần kết nối của ECHO_CLIENT
(Mã giả 3.2), thay vòng gửi nhận thô bằng SEND_JSON/RECV_JSON (Mã giả 3.6).
Mỗi dòng nhập là một thuật ngữ cần tra; hết dữ liệu nhập thì gửi {"op":"bye"}.

Chạy:  python3 lookup_client.py [host] [port]      (mặc định 127.0.0.1 9100)
"""
import sys

from framing import recv_json, send_json
from netutil import parse_endpoint, safe_connect


def main():
    host, port = parse_endpoint(sys.argv[1:], "127.0.0.1", 9100)
    s = safe_connect(host, port, 3000)
    s.settimeout(5.0)                                          # RECV_TIMEOUT 5000 ms
    print(f"[client] Đã kết nối {host}:{port}. Nhập thuật ngữ cần tra (Ctrl+D để kết thúc).", flush=True)
    try:
        for term in sys.stdin:                                 # READ_CONSOLE()
            term = term.rstrip("\r\n")
            send_json(s, {"op": "lookup", "term": term})
            resp = recv_json(s)
            if resp.get("status") == 200:
                print(f"[client] {resp['term']}: {resp['text']}", flush=True)
            else:
                print(f"[client] Lỗi {resp.get('status')}: {resp.get('error')} ('{term}')", flush=True)
        send_json(s, {"op": "bye"})
    finally:
        s.close()
        print("[client] Đã đóng kết nối", flush=True)


if __name__ == "__main__":
    main()
