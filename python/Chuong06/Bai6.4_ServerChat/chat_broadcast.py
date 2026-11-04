"""Bài 6.4 – Server chat: cấu trúc dữ liệu dùng chung, phát tới phòng, gửi riêng và tác vụ ghi.

Cài đặt Mã giả 6.6 (Mục 6.3.3, Giáo trình Lập trình mạng) cùng phần cấu trúc dữ liệu của Mã giả 6.5
(STRUCT Client, users, rooms, lock) và các thủ tục mà mã giả chỉ nêu tên: REGISTER, UNREGISTER,
JOIN_ROOM, LEAVE_ROOM. Dùng SEND_JSON_ASYNC của Mã giả 5.1 (async_frame.py).
"""
import asyncio
import threading
from datetime import datetime, timezone

from async_frame import send_json_async

OUTBOX_SIZE = 256                                       # BOUNDED_QUEUE(256)
ERRORS = {400: "Sai cú pháp", 403: "Chưa tham gia phòng", 404: "Không có người nhận",
          409: "Tên đã được sử dụng", 429: "Gửi quá nhanh"}


class Client:
    """STRUCT Client { name, sock, outbox ← BOUNDED_QUEUE(256) }."""
    def __init__(self, name, sock):
        self.name, self.sock = name, sock               # sock: StreamWriter của kết nối
        self.outbox = asyncio.Queue(OUTBOX_SIZE)
        self.win_start, self.win_count = 0.0, 0         # đếm tin trong cửa sổ 1 giây (lỗi 429)

    def close_outbox(self):
        """outbox.close(): đặt dấu kết thúc None để tác vụ ghi dừng."""
        while self.outbox.full():
            self.outbox.get_nowait()                    # bỏ tin còn lại: client đã rời
        self.outbox.put_nowait(None)


users, rooms = {}, {}                                   # tên → Client; tên phòng → tập tên
lock = threading.Lock()                                 # LOCK() bảo vệ users, rooms


def log(msg):
    print(msg, flush=True)


def error(code):
    return {"type": "error", "code": code, "text": ERRORS[code]}


def now_ts():
    """NOW(): thời điểm do server gán, dạng ISO 8601 UTC."""
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z"


def register(name, sock):
    """REGISTER: thêm tên vào users nếu chưa có; trả None nếu trùng tên."""
    with lock:
        if name in users:
            return None
        c = users[name] = Client(name, sock)
        return c


def unregister(c):
    """UNREGISTER: xóa client khỏi users và mọi phòng, báo cho các phòng đó."""
    with lock:
        users.pop(c.name, None)
        left = [r for r, members in rooms.items() if c.name in members]
        for r in left:
            rooms[r].discard(c.name)
            if not rooms[r]:
                del rooms[r]
    for r in left:                                      # phát ngoài khóa
        broadcast(r, {"type": "sys", "text": f"{c.name} đã rời phòng {r}"}, None)


def join_room(c, room):
    with lock:
        rooms.setdefault(room, set()).add(c.name)
    log(f"[server] {c.name} vào phòng {room}")
    broadcast(room, {"type": "sys", "text": f"{c.name} đã vào phòng {room}"}, None)


def leave_room(c, room):
    with lock:
        member = room in rooms and c.name in rooms[room]
        if member:
            rooms[room].discard(c.name)
            if not rooms[room]:
                del rooms[room]
    if not member:
        enqueue(c, error(403))
        return
    log(f"[server] {c.name} rời phòng {room}")
    broadcast(room, {"type": "sys", "text": f"{c.name} đã rời phòng {room}"}, None)


def broadcast(room, msg, sender):
    """BROADCAST (Mã giả 6.6, dòng 1–10); sender None: tin hệ thống."""
    targets = set()                                     # phòng chưa tồn tại
    with lock:                                          # giữ khóa ngắn
        if room in rooms:
            targets = set(rooms[room])                  # COPY
    if sender is not None and sender.name not in targets:
        enqueue(sender, error(403))
        return
    for name in targets:
        send_to(name, msg, None)


def send_to(name, msg, sender):
    """SEND_TO (Mã giả 6.6, dòng 12–18)."""
    with lock:
        c = users.get(name)
    if c is not None:
        enqueue(c, msg)
    elif sender is not None:
        enqueue(sender, error(404))


def enqueue(c, msg):
    """ENQUEUE (Mã giả 6.6, dòng 20–24): try_put không bao giờ chờ."""
    try:
        c.outbox.put_nowait(msg)
    except asyncio.QueueFull:                           # hàng đợi đầy: client chậm
        log(f"[server] Ngắt client chậm {c.name}")
        c.sock.transport.abort()                        # CLOSE(c.sock)


async def writer(c):
    """WRITER (Mã giả 6.6, dòng 26–34): tác vụ ghi duy nhất của mỗi socket."""
    try:
        while (msg := await c.outbox.get()) is not None:
            await send_json_async(c.sock, msg)
    except OSError:                                     # NetError
        c.sock.transport.abort()                        # phiên sẽ kết thúc theo
