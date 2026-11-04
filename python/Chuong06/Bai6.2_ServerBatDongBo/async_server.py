"""Bài 6.2 – Server tra cứu bất đồng bộ: mỗi kết nối một tác vụ.

Cài đặt Mã giả 6.3 (Mục 6.2.3, Giáo trình Lập trình mạng). Kế thừa Mã giả 6.1 (SOCKET, SETOPT,
BIND, LISTEN), các hàm khung bất đồng bộ của Mã giả 5.1 (async_frame.py) và PROCESS_REQUEST
của Mã giả 3.7.

Chạy:  python3 async_server.py [host] [port]      (mặc định 0.0.0.0 9000)
"""
import asyncio
import json
import os
import socket
import sys
import threading

from async_frame import ConnectionClosed, ProtocolError, recv_json_async, send_json_async
from netutil import parse_endpoint

with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "glossary.json"), encoding="utf-8") as f:
    DICT = json.load(f)                                 # LOAD_JSON_FILE("glossary.json")


def process_request(req):
    """Mã giả 3.7, dòng 2–12 (giữ nguyên): nghiệp vụ nhanh, không chặn."""
    op = req.get("op")
    if op == "lookup":
        term = req.get("term")
        if term in DICT:
            return {"status": 200, "term": term, "text": DICT[term]}
        return {"status": 404, "error": "Không có thuật ngữ"}
    if op == "list":
        return {"status": 200, "terms": list(DICT)}
    return {"status": 400, "error": "Sai thao tác"}


async def serve_async(reader, writer, addr):
    """SERVE_ASYNC: vòng yêu cầu–phản hồi; khi chờ ở await, tác vụ không chiếm luồng nào."""
    try:
        while True:
            req = await asyncio.wait_for(recv_json_async(reader), 300)   # WITH_TIMEOUT(300000, …)
            await send_json_async(writer, process_request(req))
    except ConnectionClosed as e:
        reason = "client đóng kết nối" if e.received == 0 else str(e)
    except TimeoutError:
        reason = "quá thời gian chờ nhận"
    except (ProtocolError, ValueError) as e:            # ProtocolError, JSON sai
        reason = f"lỗi giao thức: {e}"
    except OSError as e:                                # NetError
        reason = f"lỗi mạng: {e}"
    finally:
        writer.close()                                  # CLOSE(conn)
    print(f"[server] Kết thúc {addr}: {reason}", flush=True)


async def async_server(args):
    host, port = parse_endpoint(args, "0.0.0.0", 9000)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)        # như Mã giả 6.1
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((host, port))
    srv.listen(128)
    srv.setblocking(False)                              # socket dùng với vòng lặp sự kiện
    loop = asyncio.get_running_loop()
    tasks = set()                                       # giữ tham chiếu tới các tác vụ đang chạy
    print(f"[server] Server bất đồng bộ đang lắng nghe tại {host}:{port}", flush=True)
    while True:
        conn, a = await loop.sock_accept(srv)           # AWAIT ACCEPT_ASYNC(srv)
        addr = f"{a[0]}:{a[1]}"
        reader, writer = await asyncio.open_connection(sock=conn)
        task = asyncio.create_task(serve_async(reader, writer, addr))   # RUN_BACKGROUND: không chờ
        tasks.add(task)
        task.add_done_callback(tasks.discard)
        print(f"[server] Nhận {addr} (tác vụ: {len(tasks)}, "
              f"luồng: {threading.active_count()})", flush=True)


def main():
    try:
        asyncio.run(async_server(sys.argv[1:]))
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)


if __name__ == "__main__":
    main()
