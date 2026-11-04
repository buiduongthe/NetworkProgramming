"""Bài 3.4 – Server tra cứu và nhận tệp có kiểm tra mã băm SHA-256.

Cài đặt Mã giả 3.9 (Mục 3.5.2, Giáo trình Lập trình mạng): HANDLE_PUT được gắn vào HANDLE_LOOKUP
của Mã giả 3.7 bằng nhánh  IF req.op = "put" THEN HANDLE_PUT(conn, req); CONTINUE.
Ba biện pháp an toàn: BASENAME chặn "../"; giới hạn MAX_FILE; ghi tệp tạm .part, chỉ đổi tên khi mã băm khớp.

Chạy:  python3 put_server.py [host] [port]      (mặc định 0.0.0.0 9100; tệp lưu trong thư mục uploads/)
"""
import hashlib
import json
import os
import socket
import sys

from framing import ConnectionClosed, JsonError, ProtocolError, recv_json, send_json
from netutil import parse_endpoint

MAX_FILE = 100 * 1048576
UPLOAD_DIR = "uploads"
with open("glossary.json", encoding="utf-8") as f:    # DICT ← LOAD_JSON_FILE("glossary.json")
    DICT = json.load(f)


def process_request(req):
    """Mã giả 3.7 – nghiệp vụ tra cứu, độc lập với socket."""
    op = req.get("op")
    if op == "lookup":
        term = req.get("term")
        if isinstance(term, str) and term in DICT:
            return {"status": 200, "term": term, "text": DICT[term]}
        return {"status": 404, "error": "Không có thuật ngữ"}
    if op == "list":
        return {"status": 200, "terms": list(DICT.keys())}
    return {"status": 400, "error": "Sai thao tác"}


def basename(name):
    """BASENAME: bỏ mọi thành phần thư mục, tách theo cả '/' và '\\' để an toàn trên mọi hệ điều hành."""
    return name.replace("\\", "/").split("/")[-1] if isinstance(name, str) else ""


def is_hex(v, n):
    """IS_HEX(v, n): v là chuỗi đúng n ký tự thập lục phân."""
    return isinstance(v, str) and len(v) == n and all(c in "0123456789abcdefABCDEF" for c in v)


def handle_put(conn, req):
    """Mã giả 3.9 – req = {op, name, size, sha256}; trả mã trạng thái để ghi nhật ký."""
    name = basename(req.get("name"))                          # dòng 3: BASENAME(REPLACE(name, "\\", "/"))
    size = req.get("size")
    if (name in ("", ".", "..") or not isinstance(size, int) or isinstance(size, bool)   # dòng 4–6
            or size < 0 or size > MAX_FILE or not is_hex(req.get("sha256"), 64)):
        send_json(conn, {"status": 400, "error": "Sai tham số"})
        return "400 (Sai tham số)"
    send_json(conn, {"status": 100})                          # sẵn sàng nhận nội dung
    tmp = os.path.join(UPLOAD_DIR, name + ".part")
    f = open(tmp, "wb")                                       # OPEN_FILE(tmp, WRITE)
    h = hashlib.sha256()                                      # SHA256_INIT()
    remaining = size
    try:
        while remaining > 0:
            chunk = conn.recv(min(65536, remaining))
            if len(chunk) == 0:
                raise ConnectionClosed(size - remaining, f"Tệp chưa nhận đủ ({size - remaining}/{size} byte)")
            f.write(chunk)
            h.update(chunk)                                   # SHA256_UPDATE
            remaining -= len(chunk)
    except Exception:                                         # dòng 23–25 CATCH ANY: xóa tệp dở dang rồi báo tiếp
        f.close()
        os.remove(tmp)
        raise
    f.close()
    if h.hexdigest() == req["sha256"]:                        # HEX(SHA256_FINAL(h))
        os.replace(tmp, os.path.join(UPLOAD_DIR, name))       # RENAME (ghi đè nếu đã có)
        send_json(conn, {"status": 200, "saved": name})
        return f"200, lưu {UPLOAD_DIR}/{name} ({size} byte)"
    os.remove(tmp)
    send_json(conn, {"status": 422, "error": "Mã băm không khớp"})
    return "422 (Mã băm không khớp)"


def handle_lookup(conn, addr):
    """Mã giả 3.7 – HANDLE_LOOKUP, bổ sung nhánh "put" theo Mã giả 3.9."""
    while True:
        try:
            req = recv_json(conn)
        except ConnectionClosed as e:
            if e.received == 0:
                return                                        # đóng tại ranh giới
            raise                                             # đóng giữa khung: lỗi
        except JsonError:
            send_json(conn, {"status": 400, "error": "JSON sai"})
            continue
        if not isinstance(req, dict):                         # Mã giả 3.7 dòng 24–27
            send_json(conn, {"status": 400, "error": "JSON sai"})
            continue
        op = req.get("op")
        if op == "bye":
            return
        if op == "put":
            print(f"[server] {addr}: put '{req.get('name')}' -> {handle_put(conn, req)}", flush=True)
            continue
        resp = process_request(req)
        send_json(conn, resp)
        op = op if isinstance(op, str) else "?"               # chỉ để ghi nhật ký
        term = f" '{req.get('term')}'" if op == "lookup" else ""
        print(f"[server] {addr}: {op}{term} -> {resp['status']}", flush=True)


def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9100)
    os.makedirs(UPLOAD_DIR, exist_ok=True)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)      # SOCKET(INET, STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)    # SETOPT(REUSEADDR)
    srv.bind((host, port))                                        # BIND
    srv.listen(16)                                                # LISTEN, backlog 16
    print(f"[server] Đang lắng nghe tại {host}:{port}, thư mục tải lên: {UPLOAD_DIR}/", flush=True)
    try:
        while True:
            conn, (ip, cport) = srv.accept()                      # ACCEPT
            addr = f"{ip}:{cport}"
            print(f"[server] Client kết nối: {addr}", flush=True)
            try:
                handle_lookup(conn, addr)
            except (OSError, ConnectionClosed, ProtocolError) as e:
                print(f"[server] Lỗi với {addr}: {e}", flush=True)    # không dừng server
            finally:
                conn.close()
                print(f"[server] Client {addr} đã đóng kết nối", flush=True)
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
