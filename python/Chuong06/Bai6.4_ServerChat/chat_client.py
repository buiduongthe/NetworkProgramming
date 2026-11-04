"""Bài 6.4 – Client chat dòng lệnh tối giản dùng lớp ChatClient.

Lớp ChatClient chép gọn từ Mã giả 5.5 (Mục 5.3.3): CONNECT gửi hello rồi chạy nền RECEIVE_LOOP,
SEND dùng send_lock để mỗi lúc chỉ gửi một khung, DISCONNECT hủy vòng nhận rồi đóng socket.
Lệnh (Mục 6.3.3): /join phòng, /leave phòng, /pm tên nội dung, /ping, /quit; dòng khác là tin msg
gửi tới phòng hiện tại (phòng của lệnh /join gần nhất). Bộ hẹn giờ gửi ping mỗi 30 giây.
Khi hết dữ liệu nhập (Ctrl+D), client chờ 1 giây để nhận nốt thông điệp rồi ngắt kết nối.

Chạy:  python3 chat_client.py [host] [port] [tên]      (mặc định 127.0.0.1 9200 user)
"""
import asyncio
import sys
import threading

from async_frame import ConnectionClosed, ProtocolError, parse_endpoint, recv_json_async, send_json_async


class ChatClient:
    """STRUCT ChatClient { sock, send_lock, on_message, on_status } (Mã giả 5.5)."""
    def __init__(self, on_message, on_status):
        self.on_message, self.on_status = on_message, on_status
        self.send_lock = asyncio.Lock()                 # ASYNC_LOCK()
        self.reader = self.writer = self.recv_task = None

    async def connect(self, host, port, name):
        self.reader, self.writer = await asyncio.wait_for(          # SAFE_CONNECT_ASYNC, 3000 ms
            asyncio.open_connection(host, port), 3)
        try:
            await send_json_async(self.writer, {"type": "hello", "name": name})
        except OSError:
            self.writer.close()                         # không để rò socket
            raise
        self.on_status("connected")                     # báo trước khi vòng nhận chạy nền
        self.recv_task = asyncio.create_task(self.receive_loop())   # RUN_BACKGROUND

    async def receive_loop(self):
        try:
            while True:
                self.on_message(await recv_json_async(self.reader))
        except asyncio.CancelledError:
            return                                      # ngắt kết nối chủ động
        except (OSError, ConnectionClosed, ProtocolError, ValueError) as e:
            self.writer.close()
            reason = "server đã đóng kết nối" if getattr(e, "received", None) == 0 else str(e)
            self.on_status(f"lost: {reason}")

    async def send(self, obj):
        async with self.send_lock:                      # mỗi lúc gửi một khung
            await send_json_async(self.writer, obj)

    def disconnect(self):
        self.recv_task.cancel()                         # hủy trước, đóng socket sau
        self.writer.close()


def show(m):
    """on_message: hiển thị một thông điệp từ server."""
    t, hhmmss = m.get("type"), str(m.get("ts", ""))[11:19]
    if t == "welcome":
        print(f"[client] Chào mừng! Trực tuyến: {', '.join(m['users'])}; "
              f"phòng: {', '.join(m['rooms']) or '(chưa có)'}", flush=True)
    elif t == "msg":
        print(f"[client] {hhmmss} [{m['room']}] {m['from']}: {m['text']}", flush=True)
    elif t == "pm":
        print(f"[client] {hhmmss} [riêng] {m['from']}: {m['text']}", flush=True)
    elif t == "sys":
        print(f"[client] * {m['text']}", flush=True)
    elif t == "error":
        print(f"[client] Lỗi {m['code']}: {m.get('text', '')}", flush=True)
    elif t == "pong":
        print("[client] pong", flush=True)
    else:
        print(f"[client] {m}", flush=True)


async def main_async(args):
    host, port = parse_endpoint(args, "127.0.0.1", 9200)
    name = args[2] if len(args) > 2 else "user"
    status = {"connected": f"[client] Đã kết nối {host}:{port}, gửi hello với tên {name}"}
    c = ChatClient(show, lambda s: print(status.get(s, f"[client] Mất kết nối: {s[6:]}"), flush=True))
    try:
        await c.connect(host, port, name)
    except (OSError, asyncio.TimeoutError) as e:
        print(f"[client] Không kết nối được {host}:{port}: {e}", flush=True)
        return
    loop = asyncio.get_running_loop()
    lines = asyncio.Queue()

    def read_console():                                 # READ_CONSOLE trong luồng phụ
        for line in sys.stdin:
            loop.call_soon_threadsafe(lines.put_nowait, line.rstrip("\n"))
        loop.call_soon_threadsafe(lines.put_nowait, None)
    threading.Thread(target=read_console, daemon=True).start()

    async def ping_timer():                             # bộ hẹn giờ nhịp tim 30 giây
        while True:
            await asyncio.sleep(30)
            await c.send({"type": "ping"})
    pinger = asyncio.create_task(ping_timer())
    room = None                                         # phòng hiện tại
    while (line := await lines.get()) is not None:
        line = line.strip()
        if not line:
            continue
        print(f"[client] > {line}", flush=True)
        cmd, _, rest = line.partition(" ")
        try:
            if cmd == "/join" and rest:
                room = rest
                await c.send({"type": "join", "room": room})
            elif cmd == "/leave" and rest:
                await c.send({"type": "leave", "room": rest})
            elif cmd == "/pm" and " " in rest:
                to, _, text = rest.partition(" ")
                await c.send({"type": "pm", "to": to, "text": text})
            elif cmd == "/ping":
                await c.send({"type": "ping"})
            elif cmd == "/quit":
                break
            elif line.startswith("/"):
                print("[client] Lệnh không hợp lệ", flush=True)
            elif room is None:
                print("[client] Chưa chọn phòng: dùng /join tên_phòng", flush=True)
            else:
                await c.send({"type": "msg", "room": room, "text": line})
        except OSError as e:
            print(f"[client] Gửi lỗi: {e}", flush=True)
            break
    else:
        await asyncio.sleep(1)                          # hết dữ liệu nhập: chờ nhận nốt
    pinger.cancel()
    c.disconnect()
    print("[client] Đã ngắt kết nối", flush=True)


if __name__ == "__main__":
    try:
        asyncio.run(main_async(sys.argv[1:]))
    except KeyboardInterrupt:
        pass
