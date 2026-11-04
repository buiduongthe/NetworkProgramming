"""Bài 3.4 – Client kiểm thử chức năng tải tệp (ca 8, 9 của tập ca kiểm thử, Mục 3.5.3).

Client tự tạo khung tiêu đề và nội dung (không dùng client chuẩn) để thử: tên tệp chứa "../"
hoặc "..\\", tên chỉ gồm thư mục, mã băm khai báo sai, kích thước vượt MAX_FILE và ngắt kết nối
khi đang gửi nội dung. Mỗi ca mở một kết nối riêng tới server.

Chạy:  python3 kiem_thu.py [host] [port]      (mặc định 127.0.0.1 9100)
"""
import hashlib
import sys

from framing import recv_json, send_all, send_json
from netutil import parse_endpoint, safe_connect

HOST, PORT = parse_endpoint(sys.argv[1:], "127.0.0.1", 9100)
DATA = "Nội dung thử\n".encode("utf-8") * 10
results = []


def connect():
    s = safe_connect(HOST, PORT, 3000)
    s.settimeout(5.0)
    return s


def report(no, name, ok, detail):
    results.append(ok)
    print(f"[kiểm thử] Ca {no} – {name}: {'ĐẠT' if ok else 'KHÔNG ĐẠT'} ({detail})", flush=True)


def put_raw(s, name, data, sha256=None, size=None):
    """Gửi tiêu đề put; nếu server sẵn sàng (100) thì gửi nội dung và trả phản hồi cuối."""
    send_json(s, {"op": "put", "name": name, "size": len(data) if size is None else size,
                  "sha256": sha256 or hashlib.sha256(data).hexdigest()})
    resp = recv_json(s)
    if resp.get("status") != 100:
        return resp
    send_all(s, data)
    return recv_json(s)


def main():
    print(f"[kiểm thử] Server {HOST}:{PORT}", flush=True)
    with connect() as s:
        r = put_raw(s, "../thu.txt", DATA)                                   # Ca 8a
        report("8a", "tên '../thu.txt'", r.get("status") == 200 and r.get("saved") == "thu.txt",
               f"status {r.get('status')}, lưu '{r.get('saved')}'")
        r = put_raw(s, "..\\..\\thu2.txt", DATA)                             # Ca 8b
        report("8b", "tên '..\\..\\thu2.txt'", r.get("status") == 200 and r.get("saved") == "thu2.txt",
               f"status {r.get('status')}, lưu '{r.get('saved')}'")
        r = put_raw(s, "../", DATA)                                          # Ca 8c
        report("8c", "tên '../' (không còn tên tệp)", r.get("status") == 400,
               f"bị từ chối: status {r.get('status')}, {r.get('error')}")
        r = put_raw(s, "sai_bam.txt", DATA, sha256="0" * 64)                 # Ca 9
        report("9", "mã băm khai báo sai", r.get("status") == 422, f"status {r.get('status')}, {r.get('error')}")
        r = put_raw(s, "lon.bin", DATA, size=200 * 1048576)                  # Ca bổ sung 1
        report("bổ sung 1", "size 200 MiB > MAX_FILE", r.get("status") == 400,
               f"status {r.get('status')}, {r.get('error')}")
        send_json(s, {"op": "bye"})

    with connect() as s:                                                     # Ca bổ sung 2: ngắt giữa chừng
        send_json(s, {"op": "put", "name": "dang_do.txt", "size": 1000, "sha256": "0" * 64})
        r = recv_json(s)
        send_all(s, DATA[:100])                                              # chỉ gửi 100/1000 byte rồi đóng
    with connect() as s:
        send_json(s, {"op": "list"})
        r2 = recv_json(s)
        report("bổ sung 2", "ngắt sau 100/1000 byte", r.get("status") == 100 and r2.get("status") == 200,
               f"server vẫn phục vụ client mới, list -> {r2.get('status')}")
        send_json(s, {"op": "bye"})

    print(f"[kiểm thử] Kết quả: {sum(results)}/{len(results)} ca đạt", flush=True)


if __name__ == "__main__":
    main()
