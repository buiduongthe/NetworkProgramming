"""Bài 4.5 – Server tra cứu UDP xác thực địa chỉ nguồn bằng mã thách thức (cookie) không trạng thái.

Cài đặt Mã giả 4.8 (Mục 4.6.1, Giáo trình Lập trình mạng): COOKIE, HANDLE_DATAGRAM.
Kế thừa: Mã giả 4.3 (server tra cứu UDP) – thay thân vòng lặp bằng HANDLE_DATAGRAM.
Cookie = HMAC-SHA256(KEY, "ip:port:w"), w là khung thời gian 1 phút; server không lưu trạng thái.
Với địa chỉ chưa xác thực, server không gửi phản hồi lớn hơn MAX_RATIO lần yêu cầu mà gửi thách thức 401;
nếu chính thách thức cũng vượt MAX_RATIO lần yêu cầu thì không gửi gì. Datagram quá lớn, không phải JSON
hoặc không phải đối tượng bị bỏ im lặng.

Chạy:  python3 udp_cookie_server.py [host] [port]      (mặc định 0.0.0.0 9100)
"""
import hashlib
import hmac
import json
import os
import socket
import sys
import time

MAX_DGRAM = 1200
KEY, MAX_RATIO = os.urandom(32), 3                  # Mã giả 4.8, dòng 1: KEY ← RANDOM_BYTES(32), MAX_DGRAM ← 1200
HERE = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(HERE, "glossary.json"), encoding="utf-8") as f:
    DICT = json.load(f)


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > giá trị mặc định."""
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


def process_request(req):
    """PROCESS_REQUEST (kế thừa nguyên vẹn từ server tra cứu): nghiệp vụ, độc lập với socket."""
    op = req.get("op")
    if op == "lookup":
        term = req.get("term")
        if isinstance(term, str) and term in DICT:
            return {"status": 200, "term": term, "text": DICT[term]}
        return {"status": 404, "error": "Không có thuật ngữ"}
    if op == "list":
        return {"status": 200, "terms": list(DICT)}
    return {"status": 400, "error": "Sai thao tác"}


def to_bytes(obj):
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def safe_equal(a, b):
    """SAFE_EQUAL: so sánh thời gian hằng, không phụ thuộc vị trí ký tự khác nhau đầu tiên."""
    return hmac.compare_digest(a.encode("utf-8"), b.encode("utf-8"))


def cookie(addr, w):
    """Mã giả 4.8, dòng 2–5: mã thách thức gắn với địa chỉ (ip, cổng) và khung thời gian w."""
    msg = f"{addr[0]}:{addr[1]}:{w}"
    return hmac.new(KEY, msg.encode("utf-8"), hashlib.sha256).hexdigest()


def handle_datagram(s, data, addr):
    """Mã giả 4.8, dòng 6–25; datagram bị bỏ chỉ được ghi nhật ký, không có phản hồi nào."""
    if len(data) > MAX_DGRAM:                                        # dòng 7: quá lớn: bỏ
        print(f"[server] Bỏ datagram {len(data)} byte > MAX_DGRAM từ {addr[0]}:{addr[1]}", flush=True)
        return
    try:                                                             # dòng 8–12
        req = json.loads(data.decode("utf-8"))                       # dòng 9 (gồm lỗi UTF-8)
    except ValueError:                                               # dòng 10–11: không phải JSON: bỏ im lặng
        print(f"[server] Bỏ datagram không phải JSON từ {addr[0]}:{addr[1]}", flush=True)
        return
    if not isinstance(req, dict):                                    # dòng 13
        print(f"[server] Bỏ datagram không phải đối tượng JSON từ {addr[0]}:{addr[1]}", flush=True)
        return
    w = int(time.time() // 60)                                       # dòng 14: FLOOR(NOW_SECONDS() / 60)
    c = req.get("cookie")
    ok = isinstance(c, str) and (safe_equal(c, cookie(addr, w))     # dòng 15–16: so sánh thời gian hằng
                                 or safe_equal(c, cookie(addr, w - 1)))
    state = "không có" if c is None else ("hợp lệ" if ok else "không hợp lệ")
    print(f"[server] Nhận {len(data)} byte từ {addr[0]}:{addr[1]} (id={json.dumps(req.get('id'))}, cookie: {state})",
          flush=True)
    resp = process_request(req)                                      # dòng 17
    resp["id"] = req.get("id")
    out = to_bytes(resp)                                             # dòng 18
    if len(out) > MAX_DGRAM:                                         # giữ kiểm tra của Mã giả 4.3
        resp = {"id": req.get("id"), "status": 413, "error": "Quá lớn"}
        out = to_bytes(resp)
    if not ok and len(out) > MAX_RATIO * len(data):                  # dòng 19
        ch = {"id": req.get("id"), "status": 401, "cookie": cookie(addr, w)}   # dòng 20
        small = to_bytes(ch)                                         # dòng 21: phản hồi nhỏ
        if len(small) > MAX_RATIO * len(data):                       # dòng 22: thách thức cũng quá lớn: không gửi
            print(f"[server]   -> phản hồi {len(out)} byte, thách thức {len(small)} byte đều > {MAX_RATIO} × "
                  f"{len(data)} byte: không gửi gì", flush=True)
            return
        print(f"[server]   -> phản hồi {len(out)} byte > {MAX_RATIO} × {len(data)} byte: "
              f"gửi thách thức 401 ({len(small)} byte)", flush=True)
        out = small
    else:
        print(f"[server]   -> gửi phản hồi status={resp['status']} ({len(out)} byte)", flush=True)
    s.sendto(out, addr)                                              # dòng 24


def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9100)
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind((host, port))
    print(f"[server] Đang lắng nghe UDP tại {host}:{port}, từ điển {len(DICT)} thuật ngữ, MAX_RATIO = {MAX_RATIO}",
          flush=True)
    try:
        while True:
            try:
                data, addr = s.recvfrom(65535)
            except ConnectionResetError:                             # Windows: ICMP port unreachable
                continue
            handle_datagram(s, data, addr)
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        s.close()


if __name__ == "__main__":
    main()
