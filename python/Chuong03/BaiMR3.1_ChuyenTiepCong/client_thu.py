"""Bài mở rộng MR3.1 – Client thử: gửi dữ liệu, đóng chiều gửi, đọc phản hồi tới EOF.

Chạy:  python3 client_thu.py <host> <port> <chuỗi> [số_lần_lặp]
"""
import socket
import sys


def main():
    host, port, text = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    repeat = int(sys.argv[4]) if len(sys.argv) > 4 else 1
    data = (text * repeat).encode("utf-8")
    chunks = []
    with socket.create_connection((host, port), timeout=5) as s:
        try:
            s.sendall(data)
            s.shutdown(socket.SHUT_WR)               # báo hết dữ liệu: server mới bắt đầu trả lời
            while True:
                part = s.recv(65536)
                if not part:                         # EOF: phía bên kia đã đóng chiều gửi
                    break
                chunks.append(part)
        except ConnectionResetError:                 # bên kia đóng khi còn dữ liệu chưa đọc -> RST
            print("[client] Kết nối bị đặt lại (RST) trước khi có phản hồi", flush=True)
    reply = b"".join(chunks)
    print(f"[client] Gửi {len(data)} byte, nhận {len(reply)} byte", flush=True)
    if not reply:
        print("[client] Kết nối bị đóng mà không có dữ liệu trả về", flush=True)
        sys.exit(1)
    phan_hoi = reply.decode("utf-8", errors="replace")   # cắt theo ký tự, không theo byte
    ok = reply == text.upper().encode("utf-8") * repeat
    print(f"[client] Phản hồi: {phan_hoi[:40]!r}{'...' if len(phan_hoi) > 40 else ''}", flush=True)
    print(f"[client] So khớp với bản chữ hoa: {'KHỚP' if ok else 'SAI'}", flush=True)


if __name__ == "__main__":
    main()
