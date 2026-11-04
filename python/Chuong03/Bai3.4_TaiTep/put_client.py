"""Bài 3.4 – Client tra cứu và tải tệp lên server.

Cài đặt Mã giả 3.10 (PUT_FILE, Mục 3.5.2) trên nền client tra cứu của Mã giả 3.8:
cùng một kết nối dùng cho cả tra cứu và tải tệp. Mỗi dòng nhập là một lệnh:
    put <đường dẫn tệp>     – tải tệp lên (đọc và gửi từng khối 64 KiB)
    <thuật ngữ>             – tra cứu thuật ngữ

Chạy:  python3 put_client.py [host] [port]      (mặc định 127.0.0.1 9100)
"""
import hashlib
import os
import sys

from framing import ProtocolError, json_encode, recv_json, send_all, send_json
from netutil import parse_endpoint, safe_connect


def sha256_of_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


def put_file(s, path):
    """Mã giả 3.10: khung tiêu đề -> chờ 100 -> gửi đúng size byte -> nhận kết quả."""
    digest = sha256_of_file(path)                             # HEX(SHA256_OF_FILE(path))
    name, size = os.path.basename(path), os.path.getsize(path)
    print(f"[client] Tải lên {name}: {size} byte, SHA-256 {digest[:16]}…", flush=True)
    send_json(s, {"op": "put", "name": name, "size": size, "sha256": digest})
    resp = recv_json(s)
    if resp.get("status") != 100:
        raise ProtocolError(resp.get("error"))
    with open(path, "rb") as f:                               # OPEN_FILE(path, READ)
        while chunk := f.read(65536):                         # READ(f, 65536)
            send_all(s, chunk)
    resp = recv_json(s)
    print(f"[client] Kết quả: {resp.get('status')} {json_encode(resp)}", flush=True)


def main():
    host, port = parse_endpoint(sys.argv[1:], "127.0.0.1", 9100)
    s = safe_connect(host, port, 3000)
    s.settimeout(5.0)                                          # RECV_TIMEOUT 5000 ms
    print(f"[client] Đã kết nối {host}:{port}. Nhập 'put <tệp>' hoặc thuật ngữ (Ctrl+D để kết thúc).", flush=True)
    try:
        for line in sys.stdin:                                 # READ_CONSOLE()
            line = line.rstrip("\r\n")
            if line.startswith("put "):
                path = line[4:].strip()
                try:
                    put_file(s, path)
                except FileNotFoundError:
                    print(f"[client] Lỗi: không tìm thấy tệp '{path}'", flush=True)
                except ProtocolError as e:                     # server không trả 100
                    print(f"[client] Lỗi: {e}", flush=True)
                continue
            send_json(s, {"op": "lookup", "term": line})
            resp = recv_json(s)
            if resp.get("status") == 200:
                print(f"[client] {resp['term']}: {resp['text']}", flush=True)
            else:
                print(f"[client] Lỗi {resp.get('status')}: {resp.get('error')} ('{line}')", flush=True)
        send_json(s, {"op": "bye"})
    finally:
        s.close()
        print("[client] Đã đóng kết nối", flush=True)


if __name__ == "__main__":
    main()
