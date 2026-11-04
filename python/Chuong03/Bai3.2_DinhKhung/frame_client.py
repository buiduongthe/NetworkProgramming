"""Bài 3.2 – Echo Client dùng khung JSON: gửi dòng nhập từ bàn phím và một thông điệp lớn.

Cài đặt Mã giả 3.2 (Mục 3.2.3) với vòng gửi nhận thô được thay bằng SEND_JSON/RECV_JSON
của Mã giả 3.6 (Mục 3.4.3) – lời giải bài tập "Định khung thông điệp" của Chương 3.
Tham số thứ ba (tùy chọn) là số ký tự của thông điệp lớn gửi sau khi hết dữ liệu nhập,
ví dụ 200000: nhờ RECV_EXACT, phản hồi được nhận đủ dù đến thành nhiều phần.

Chạy:  python3 frame_client.py [host] [port] [so_ky_tu]      (mặc định 127.0.0.1 9000 0)
"""
import hashlib
import socket
import sys

from framing import recv_json, send_json
from netutil import parse_endpoint, safe_connect

PATTERN = "Mạng máy tính "                         # 14 ký tự, 18 byte UTF-8


def make_text(n):
    """Tạo chuỗi đúng n ký tự, có chữ tiếng Việt (2–3 byte mỗi ký tự trong UTF-8)."""
    return (PATTERN * (n // len(PATTERN) + 1))[:n]


def sha256_hex(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def main():
    host, port = parse_endpoint(sys.argv[1:3], "127.0.0.1", 9000)
    big = int(sys.argv[3]) if len(sys.argv) > 3 else 0
    s = safe_connect(host, port, 3000)
    s.settimeout(5.0)                                       # RECV_TIMEOUT 5000 ms
    print(f"[client] Đã kết nối {host}:{port}. Nhập dòng cần gửi (Ctrl+D để kết thúc).", flush=True)
    try:
        for line in sys.stdin:                              # READ_CONSOLE()
            line = line.rstrip("\r\n")
            send_json(s, {"text": line})                    # SEND_JSON thay cho SEND_ALL thô
            resp = recv_json(s)                             # RECV_JSON: luôn nhận đủ một khung
            print(f"[client] Gửi: '{line}' -> Nhận: '{resp.get('text')}'", flush=True)
        if big > 0:
            text = make_text(big)
            print(f"[client] Gửi thông điệp lớn: {len(text)} ký tự, "
                  f"{len(text.encode('utf-8'))} byte UTF-8", flush=True)
            send_json(s, {"text": text})
            back = recv_json(s).get("text", "")
            same = back == text
            print(f"[client] Nhận lại: {len(back)} ký tự, {len(back.encode('utf-8'))} byte; "
                  f"trùng khớp: {'có' if same else 'không'}", flush=True)
            print(f"[client] SHA-256 gửi:  {sha256_hex(text)[:32]}…", flush=True)
            print(f"[client] SHA-256 nhận: {sha256_hex(back)[:32]}…", flush=True)
        s.shutdown(socket.SHUT_WR)                          # SHUTDOWN(WRITE)
    finally:
        s.close()
        print("[client] Đã đóng kết nối", flush=True)


if __name__ == "__main__":
    main()
