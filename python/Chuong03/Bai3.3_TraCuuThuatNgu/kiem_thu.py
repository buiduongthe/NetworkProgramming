"""Bài 3.3 – Client kiểm thử server tra cứu theo tập ca kiểm thử tối thiểu (Mục 3.5.3).

Client này tự tạo dãy byte gửi đi (không dùng client chuẩn) để thực hiện các ca 1–7
của bảng ca kiểm thử, cộng một ca bổ sung cho thao tác không có trong đặc tả.
Mỗi ca mở một kết nối riêng tới server.

Chạy:  python3 kiem_thu.py [host] [port]      (mặc định 127.0.0.1 9100)
"""
import socket
import struct
import sys
import time

from framing import json_encode, recv_json, send_all, send_frame, send_json
from netutil import parse_endpoint, safe_connect

HOST, PORT = parse_endpoint(sys.argv[1:], "127.0.0.1", 9100)
results = []


def connect():
    s = safe_connect(HOST, PORT, 3000)
    s.settimeout(5.0)
    return s


def frame_of(obj):
    """Khung hoàn chỉnh (tiêu đề + nội dung) của một đối tượng JSON."""
    data = json_encode(obj).encode("utf-8")
    return struct.pack("!I", len(data)) + data


def report(no, name, ok, detail):
    results.append(ok)
    print(f"[kiểm thử] Ca {no} – {name}: {'ĐẠT' if ok else 'KHÔNG ĐẠT'} ({detail})", flush=True)


def lookup(s, term):
    send_json(s, {"op": "lookup", "term": term})
    return recv_json(s)


def main():
    print(f"[kiểm thử] Server {HOST}:{PORT}", flush=True)
    with connect() as s:                                       # Ca 1, 2
        r = lookup(s, "TCP")
        report(1, "thuật ngữ có trong từ điển", r.get("status") == 200 and r.get("term") == "TCP",
               f"status {r.get('status')}")
        r = lookup(s, "Bắt tay ba bước")
        report(2, "thuật ngữ có dấu", r.get("status") == 200 and r.get("term") == "Bắt tay ba bước",
               f"status {r.get('status')}, term '{r.get('term')}'")
        send_json(s, {"op": "bye"})

    with connect() as s:                                       # Ca 3: gửi từng byte, cách nhau 100 ms
        s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        data = frame_of({"op": "lookup", "term": "UDP"})
        for i in range(len(data)):
            send_all(s, data[i:i + 1])
            time.sleep(0.1)
        r = recv_json(s)
        report(3, f"gửi {len(data)} byte từng byte một", r.get("status") == 200 and r.get("term") == "UDP",
               f"status {r.get('status')}, term '{r.get('term')}'")
        send_json(s, {"op": "bye"})

    with connect() as s:                                       # Ca 4: hai yêu cầu trong một lần SEND
        send_all(s, frame_of({"op": "lookup", "term": "TCP"}) + frame_of({"op": "lookup", "term": "XYZ"}))
        r1, r2 = recv_json(s), recv_json(s)
        report(4, "hai yêu cầu trong một lần SEND", r1.get("status") == 200 and r2.get("status") == 404,
               f"status {r1.get('status')} rồi {r2.get('status')}")
        send_json(s, {"op": "bye"})

    with connect() as s:                                       # Ca 5: độ dài 0xFFFFFFFF > MAX_FRAME
        send_all(s, b"\xff\xff\xff\xff")
        try:
            closed = s.recv(1) == b""
        except ConnectionResetError:
            closed = True
        report(5, "độ dài 4294967295 > MAX_FRAME", closed, "server đóng kết nối" if closed else "không đóng")

    with connect() as s:                                       # Ca 6: nội dung không phải JSON
        send_frame(s, "đây không phải JSON".encode("utf-8"))
        r = recv_json(s)
        send_json(s, {"op": "list"})                           # kết nối vẫn dùng được
        r2 = recv_json(s)
        report(6, "nội dung không phải JSON", r.get("status") == 400 and r2.get("status") == 200,
               f"status {r.get('status')}, sau đó list -> {r2.get('status')}, {len(r2.get('terms', []))} thuật ngữ")
        send_json(s, {"op": "xoa", "term": "TCP"})             # Ca bổ sung: thao tác sai
        r = recv_json(s)
        report("bổ sung", "thao tác \"xoa\"", r.get("status") == 400, f"status {r.get('status')}: {r.get('error')}")
        send_frame(s, b"[1,2]")                                # Ca bổ sung: JSON hợp lệ nhưng không phải đối tượng
        r = recv_json(s)
        report("bổ sung", "JSON là mảng [1,2]", r.get("status") == 400, f"status {r.get('status')}: {r.get('error')}")
        send_json(s, {"op": "bye"})

    with connect() as s:                                       # Ca 7: đóng kết nối giữa khung
        send_all(s, struct.pack("!I", 50) + b"0123456789")
    with connect() as s:
        r = lookup(s, "TCP")
        report(7, "đóng giữa khung (14/54 byte)", r.get("status") == 200,
               f"client mới vẫn được phục vụ, status {r.get('status')}")
        send_json(s, {"op": "bye"})

    print(f"[kiểm thử] Kết quả: {sum(results)}/{len(results)} ca đạt", flush=True)


if __name__ == "__main__":
    main()
