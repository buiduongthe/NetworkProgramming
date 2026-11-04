"""Bài 3.2 – Echo Server dùng khung JSON (tiền tố độ dài 4 byte).

Khung sườn ECHO_SERVER của Mã giả 3.1 (Mục 3.2.2), thay HANDLE_ECHO bằng HANDLE_FRAMES
dùng RECV_FRAME/SEND_JSON của Mã giả 3.6 (Mục 3.4.3). Mỗi yêu cầu {"text": ...} được
trả lời bằng {"status": 200, "text": ...}; yêu cầu sai nhận {"status": 400, "error": ...}.

Chạy:  python3 frame_server.py [host] [port]      (mặc định 0.0.0.0 9000)
"""
import socket
import sys

from framing import ConnectionClosed, JsonError, ProtocolError, json_decode, recv_frame, send_json
from netutil import parse_endpoint


def handle_frames(conn, addr):
    """Vòng phục vụ một client: mỗi khung nhận được là một thông điệp hoàn chỉnh."""
    while True:
        try:
            payload = recv_frame(conn)                 # RECV_JSON tách hai bước để ghi kích thước khung
        except ConnectionClosed as e:
            if e.received == 0:
                return                                 # đóng tại ranh giới khung: kết thúc bình thường
            raise                                      # đóng giữa khung: lỗi
        print(f"[server] Nhận khung {len(payload)} byte từ {addr}", flush=True)
        try:
            req = json_decode(payload)
        except JsonError:
            send_json(conn, {"status": 400, "error": "JSON sai"})
            continue
        if isinstance(req, dict) and isinstance(req.get("text"), str):
            send_json(conn, {"status": 200, "text": req["text"]})
        else:
            send_json(conn, {"status": 400, "error": "Thiếu trường text"})


def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9000)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)      # SOCKET(INET, STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)    # SETOPT(REUSEADDR)
    srv.bind((host, port))                                        # BIND
    srv.listen(16)                                                # LISTEN, backlog 16
    print(f"[server] Đang lắng nghe tại {host}:{port}", flush=True)
    try:
        while True:                                               # vòng chấp nhận kết nối
            conn, (ip, cport) = srv.accept()                      # ACCEPT
            addr = f"{ip}:{cport}"
            print(f"[server] Client kết nối: {addr}", flush=True)
            try:
                handle_frames(conn, addr)
            except (OSError, ConnectionClosed, ProtocolError) as e:   # NetError, ConnectionClosed, ProtocolError
                print(f"[server] Lỗi với {addr}: {e}", flush=True)    # không dừng server
            finally:
                conn.close()                                      # CLOSE(conn)
                print(f"[server] Client {addr} đã đóng kết nối", flush=True)
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
