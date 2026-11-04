"""Bài 7.3 – Kiểm thử server WebSocket bằng các khung tự tạo (kể cả khung vi phạm đặc tả).

Sáu ca kiểm thử, mỗi ca một kết nối:
  1. Yêu cầu HTTP thường (không Upgrade)       -> mong đợi 400 (Mã giả 7.3, dòng 3–7)
  2. Ping                                       -> mong đợi pong cùng dữ liệu
  3. Văn bản 300 byte (độ dài mở rộng 16 bit)   -> mong đợi echo (Mã giả 7.4, dòng 5)
  4. Khung văn bản KHÔNG che                    -> mong đợi khung đóng 1002 (dòng 11–14)
  5. Khung khai báo độ dài 2^40 byte            -> mong đợi khung đóng 1009 (dòng 7–10)
  6. Sec-WebSocket-Key không phải 16 byte       -> mong đợi 400 (Mã giả 7.3, dòng 6)

Chạy:  python3 ws_kiem_thu.py [host] [port]      (mặc định 127.0.0.1 8080)
"""
import socket
import struct
import sys

from ws_client import close_code, ws_connect, ws_read_frame, ws_send


def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 8080

    with socket.create_connection((host, port), timeout=3) as s:          # ca 1
        s.sendall(f"GET / HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n".encode("ascii"))
        status = s.recv(4096).split(b"\r\n")[0].decode("ascii")
        print(f"[kiểm thử] 1. Yêu cầu HTTP thường (không Upgrade) -> {status}", flush=True)

    s, _ = ws_connect(host, port)                                         # ca 2
    with s:
        ws_send(s, 0x9, b"nhip-tim")
        _, opcode, data = ws_read_frame(s)
        print(f"[kiểm thử] 2. Ping 'nhip-tim' -> opcode 0x{opcode:X}, dữ liệu '{data.decode()}'", flush=True)
        ws_send(s, 0x8, struct.pack("!H", 1000))
        ws_read_frame(s)

    s, _ = ws_connect(host, port)                                         # ca 3
    with s:
        ws_send(s, 0x1, b"A" * 300)
        _, opcode, data = ws_read_frame(s)
        print(f"[kiểm thử] 3. Văn bản 300 byte -> nhận lại {len(data)} byte (opcode 0x{opcode:X})", flush=True)
        ws_send(s, 0x8, struct.pack("!H", 1000))
        ws_read_frame(s)

    s, _ = ws_connect(host, port)                                         # ca 4
    with s:
        ws_send(s, 0x1, b"khong che", mask=False)
        _, opcode, data = ws_read_frame(s)
        print(f"[kiểm thử] 4. Khung không che -> khung đóng, mã {close_code(data)}", flush=True)

    s, _ = ws_connect(host, port)                                         # ca 5
    with s:
        s.sendall(struct.pack("!BBQ", 0x81, 0x80 | 127, 1 << 40))        # chỉ gửi tiêu đề khung
        _, opcode, data = ws_read_frame(s)
        print(f"[kiểm thử] 5. Khung khai báo 2^40 byte -> khung đóng, mã {close_code(data)}", flush=True)

    with socket.create_connection((host, port), timeout=3) as s:          # ca 6
        s.sendall(f"GET / HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                  "Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: c2hvcnQ=\r\n\r\n".encode("ascii"))
        status = s.recv(4096).split(b"\r\n")[0].decode("ascii")
        print(f"[kiểm thử] 6. Sec-WebSocket-Key 'c2hvcnQ=' (5 byte) -> {status}", flush=True)


if __name__ == "__main__":
    main()
