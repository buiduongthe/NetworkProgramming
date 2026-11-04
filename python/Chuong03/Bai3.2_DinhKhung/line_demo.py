"""Bài 3.2 – Minh họa READ_LINE (Mã giả 3.5, Mục 3.4.3): định khung bằng ký tự xuống dòng.

Chương trình tự mở một kết nối loopback: luồng "gửi" chia dữ liệu thành các mảnh cắt ngang
dòng (kể cả cắt giữa hai byte của chữ "à"), luồng chính đọc bằng READ_LINE và in từng dòng
cùng số byte còn dư trong pending. Mảnh cuối là 100 byte không có '\\n' để thấy lỗi "Dòng quá dài".

Chạy:  python3 line_demo.py [host] [port]      (mặc định 127.0.0.1 9000)
"""
import socket
import sys
import threading
import time

from framing import ProtocolError, Reader, read_line, send_all
from netutil import parse_endpoint

MAX_LEN = 64
PIECES = [b"HELLO\nWOR", b"LD\r\nXin ch\xc3", b"\xa0o\n", b"A" * 100]


def show(data):
    """Hiển thị dãy byte: ký tự ASCII in được giữ nguyên, còn lại dạng \\xNN."""
    out = []
    for b in data:
        if b == 0x0A:
            out.append("\\n")
        elif b == 0x0D:
            out.append("\\r")
        elif 0x20 <= b < 0x7F:
            out.append(chr(b))
        else:
            out.append(f"\\x{b:02x}")
    text = "".join(out)
    return text if len(text) <= 24 else text[:21] + "..."


def sender(host, port):
    with socket.create_connection((host, port), timeout=3) as s:
        s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)    # NODELAY: mỗi mảnh đi riêng
        for p in PIECES:
            print(f"[gửi] {len(p)} byte: {show(p)}", flush=True)
            send_all(s, p)
            time.sleep(0.3)


def main():
    host, port = parse_endpoint(sys.argv[1:], "127.0.0.1", 9000)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((host, port))
    srv.listen(1)
    print(f"[đọc] Đang lắng nghe tại {host}:{port}, max_len = {MAX_LEN}", flush=True)
    t = threading.Thread(target=sender, args=(host, port), daemon=True)
    t.start()
    conn, _ = srv.accept()
    r = Reader(conn)                                       # Reader { sock, pending ← EMPTY_BYTES }
    try:
        while True:
            line = read_line(r, MAX_LEN)
            if line is None:
                print("[đọc] Đối tác đã đóng kết nối", flush=True)
                break
            print(f"[đọc] READ_LINE -> '{line}' (pending còn {len(r.pending)} byte)", flush=True)
    except ProtocolError as e:
        print(f"[đọc] ProtocolError: {e} (pending {len(r.pending)} byte > {MAX_LEN})", flush=True)
    finally:
        conn.close()
        srv.close()
    t.join()


if __name__ == "__main__":
    main()
