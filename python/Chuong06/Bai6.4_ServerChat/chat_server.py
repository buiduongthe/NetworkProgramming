"""Bài 6.4 – Server chat nhiều phòng: vòng chấp nhận kết nối và tác vụ phiên.

Cài đặt Mã giả 6.5 (Mục 6.3.3, Giáo trình Lập trình mạng): kế thừa Mã giả 6.3 (server bất đồng bộ),
thay SERVE_ASYNC bằng CHAT_SESSION. Các thủ tục phát, gửi riêng, tác vụ ghi nằm ở chat_broadcast.py
(Mã giả 6.6). Giao thức: Bảng đặc tả giao thức chat (Mục 6.3.1), cổng mặc định 9200.

Chạy:  python3 chat_server.py [host] [port]      (mặc định 0.0.0.0 9200)
"""
import asyncio
import re
import socket
import sys
import time

import chat_broadcast as cb
from async_frame import ConnectionClosed, ProtocolError, parse_endpoint, recv_json_async, send_json_async

NAME_RE = re.compile(r"^[\w-]{1,32}$")                  # tập ký tự cho phép của tên người dùng, tên phòng
RATE_LIMIT = 10                                         # tối đa 10 tin msg/pm mỗi giây (Mục 6.3.4)


class HelloError(Exception):
    def __init__(self, code):
        super().__init__(code)
        self.code = code


def valid(s):
    return isinstance(s, str) and NAME_RE.match(s) is not None


def rate_ok(c):
    now = time.monotonic()
    if now - c.win_start >= 1.0:
        c.win_start, c.win_count = now, 0
    c.win_count += 1
    return c.win_count <= RATE_LIMIT


async def chat_session(reader, writer, addr):
    """CHAT_SESSION (Mã giả 6.5, dòng 4–33)."""
    c = None                                            # chưa đăng ký tên
    try:
        h = await asyncio.wait_for(recv_json_async(reader), 10)        # 10 giây cho hello
        if h.get("type") != "hello" or not valid(h.get("name")):
            raise HelloError(400)
        c = cb.register(h["name"], writer)              # None nếu trùng tên
        if c is None:
            raise HelloError(409)
        cb.log(f"[server] {c.name} đăng ký từ {addr}")
        c.writer_task = asyncio.create_task(cb.writer(c))   # RUN_BACKGROUND(WRITER(c))
        with cb.lock:
            snapshot = {"users": sorted(cb.users), "rooms": sorted(cb.rooms)}
        cb.enqueue(c, {"type": "welcome", **snapshot})
        while True:
            m = await asyncio.wait_for(recv_json_async(reader), 90)    # 3 chu kỳ nhịp tim
            t = m.get("type")
            if t in ("msg", "pm") and not rate_ok(c):
                cb.enqueue(c, cb.error(429))
            elif t == "join" and valid(m.get("room")):
                cb.join_room(c, m["room"])
            elif t == "leave" and isinstance(m.get("room"), str):
                cb.leave_room(c, m["room"])
            elif t == "msg" and isinstance(m.get("room"), str) and isinstance(m.get("text"), str):
                cb.broadcast(m["room"], {"type": "msg", "from": c.name, "room": m["room"],
                                         "text": m["text"], "ts": cb.now_ts()}, c)
            elif t == "pm" and isinstance(m.get("to"), str) and isinstance(m.get("text"), str):
                cb.send_to(m["to"], {"type": "pm", "from": c.name, "text": m["text"],
                                     "ts": cb.now_ts()}, c)
            elif t == "ping":
                cb.enqueue(c, {"type": "pong"})
            else:                                       # OTHERWISE
                cb.enqueue(c, cb.error(400))
    except HelloError as e:                             # lời chào không hợp lệ
        cb.log(f"[server] Từ chối {addr}: lỗi {e.code}")
        try:
            await send_json_async(writer, cb.error(e.code))
        except OSError:
            pass
    except (ConnectionClosed, TimeoutError, OSError, ProtocolError, ValueError):
        pass                                            # kết thúc phiên bình thường hoặc do lỗi
    finally:
        if c is not None:
            cb.unregister(c)
            c.close_outbox()
            cb.log(f"[server] {c.name} rời server")
        writer.close()                                  # CLOSE(conn)


async def chat_server(args):
    host, port = parse_endpoint(args, "0.0.0.0", 9200)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)        # như Mã giả 6.3
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((host, port))
    srv.listen(128)
    srv.setblocking(False)
    loop = asyncio.get_running_loop()
    sessions = set()
    cb.log(f"[server] Server chat đang lắng nghe tại {host}:{port}")
    while True:
        conn, a = await loop.sock_accept(srv)           # AWAIT ACCEPT_ASYNC
        reader, writer = await asyncio.open_connection(sock=conn)
        task = asyncio.create_task(chat_session(reader, writer, f"{a[0]}:{a[1]}"))  # RUN_BACKGROUND
        sessions.add(task)
        task.add_done_callback(sessions.discard)


def main():
    try:
        asyncio.run(chat_server(sys.argv[1:]))
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)


if __name__ == "__main__":
    main()
