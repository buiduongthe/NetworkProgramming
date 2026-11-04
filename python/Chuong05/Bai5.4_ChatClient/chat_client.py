"""Bài 5.4 – Lớp dịch vụ ChatClient bất đồng bộ (asyncio).

Cài đặt Mã giả 5.5 (Mục 5.3.3, Giáo trình Lập trình mạng).
Kế thừa: Mã giả 5.1 (khung JSON bất đồng bộ); SAFE_CONNECT_ASYNC = Mã giả 2.2 + AWAIT.
Lớp không tham chiếu tới điều khiển giao diện nào: tầng giao diện (ở đây là chat_cli.py)
chỉ đăng ký hai sự kiện on_message, on_status.
Ánh xạ: cts ↔ tác vụ vòng nhận (CANCEL = task.cancel()); send_lock ↔ asyncio.Lock;
RUN_BACKGROUND ↔ asyncio.create_task.
"""
import asyncio

from khung_json_async import (ConnectionClosed, NetError, ProtocolError,
                              recv_json_async, safe_connect_async, send_json_async)


class ChatClient:
    def __init__(self):                                     # STRUCT ChatClient (dòng 1–2)
        self.reader = self.writer = None                    # sock
        self.recv_task = None                               # vai trò của cts: hủy vòng nhận
        self.send_lock = asyncio.Lock()                     # ASYNC_LOCK(): mỗi lúc gửi một khung
        self.on_message = lambda msg: None                  # sự kiện cho UI
        self.on_status = lambda text: None

    async def connect(self, host, port, name):              # Mã giả 5.5, dòng 3–14
        self.reader, self.writer = await safe_connect_async(host, port, 3000)
        hello = {"type": "hello", "name": name}
        try:
            await send_json_async(self.writer, hello)
        except (NetError, OSError):
            self.writer.close()                             # không để rò socket
            raise
        self.recv_task = asyncio.create_task(self.receive_loop())   # RUN_BACKGROUND: không await
        self.on_status("connected")

    async def receive_loop(self):                           # Mã giả 5.5, dòng 15–27
        try:
            while True:
                msg = await recv_json_async(self.reader)
                self.on_message(msg)                        # RAISE_EVENT(on_message, msg)
        except asyncio.CancelledError:
            return                                          # ngắt kết nối chủ động
        except (NetError, ConnectionClosed, ProtocolError, OSError, ValueError) as e:   # ValueError = JsonError
            self.writer.close()                             # giải phóng socket
            self.on_status(f"lost: {e}")

    async def send_message(self, room, text):               # Mã giả 5.5, dòng 28–36
        async with self.send_lock:                          # ACQUIRE_ASYNC ... FINALLY RELEASE
            m = {"type": "msg", "room": room, "text": text}
            await send_json_async(self.writer, m)

    def disconnect(self):                                   # Mã giả 5.5, dòng 37–39
        if self.recv_task:
            self.recv_task.cancel()                         # CANCEL(c.cts) trước
        if self.writer:
            self.writer.close()                             # CLOSE(c.sock) sau
