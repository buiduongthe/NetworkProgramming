"""Bài 4.2 – Server tra cứu thuật ngữ trên UDP.

Cài đặt Mã giả 4.3 (Mục 4.3.1, Giáo trình Lập trình mạng).
Kế thừa: Mã giả 4.1 (vòng RECVFROM/SENDTO), Mã giả 3.x lookup-server (PROCESS_REQUEST, DICT).
Mỗi yêu cầu và mỗi phản hồi là một datagram chứa đối tượng JSON; server chép trường id
của yêu cầu vào phản hồi và giới hạn phản hồi trong MAX_DGRAM = 1200 byte.

Chạy:  python3 udp_lookup_server.py [host] [port] [so_datagram_bo_qua]   (mặc định 0.0.0.0 9100 0)
Tham số thứ ba (thêm cho bài thực hành): bỏ qua N datagram đầu tiên để mô phỏng mất gói.
"""
import json
import os
import socket
import sys

MAX_DGRAM = 1200                                    # Mã giả 4.3, dòng 1
HERE = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(HERE, "glossary.json"), encoding="utf-8") as f:
    DICT = json.load(f)                             # DICT ← LOAD_JSON_FILE("glossary.json")


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
    """PROCESS_REQUEST (kế thừa nguyên vẹn từ server tra cứu TCP): nghiệp vụ, độc lập với socket."""
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
    """ENCODE(JSON_ENCODE(obj), "UTF-8"), dạng gọn, giữ nguyên chữ có dấu."""
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def main():
    host, port = parse_endpoint(sys.argv[1:3], "0.0.0.0", 9100)        # dòng 3
    drop = int(sys.argv[3]) if len(sys.argv) > 3 else 0                 # mô phỏng mất gói (thêm)
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)                 # dòng 4: SOCKET(INET, DGRAM)
    s.bind((host, port))                                                 # dòng 5: BIND
    print(f"[server] Đang lắng nghe UDP tại {host}:{port}, từ điển {len(DICT)} thuật ngữ", flush=True)
    if drop:
        print(f"[server] Mô phỏng mất gói: bỏ qua {drop} datagram đầu tiên", flush=True)
    count = 0
    try:
        while True:                                                      # dòng 6: LOOP
            try:
                data, addr = s.recvfrom(65535)                           # dòng 7
            except ConnectionResetError:                                 # Windows: ICMP port unreachable
                continue
            count += 1
            if count <= drop:
                print(f"[server] Bỏ qua datagram thứ {count} từ {addr[0]}:{addr[1]} (mô phỏng mất gói)", flush=True)
                continue
            try:                                                         # dòng 8–14
                req = json.loads(data.decode("utf-8"))
                if not isinstance(req, dict):
                    raise ValueError("không phải đối tượng JSON")
                resp = process_request(req)
            except ValueError:                                           # CATCH JsonError (gồm lỗi UTF-8)
                req = {"id": None}
                resp = {"status": 400, "error": "JSON sai"}
            resp["id"] = req.get("id")                                   # dòng 15: ghép phản hồi với yêu cầu
            out = to_bytes(resp)                                         # dòng 16
            if len(out) > MAX_DGRAM:                                     # dòng 17–20
                resp = {"id": req.get("id"), "status": 413, "error": "Quá lớn"}
                out = to_bytes(resp)
            s.sendto(out, addr)                                          # dòng 21
            print(f"[server] Yêu cầu id={json.dumps(req.get('id'))} op={req.get('op')} từ {addr[0]}:{addr[1]}"
                  f" -> status={resp['status']}, {len(out)} byte", flush=True)
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        s.close()


if __name__ == "__main__":
    main()
