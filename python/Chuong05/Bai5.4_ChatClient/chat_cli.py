"""Bài 5.4 – Giao diện dòng lệnh dùng lớp ChatClient (Mã giả 5.5).

Tầng "giao diện" chỉ đăng ký hai sự kiện on_message, on_status và gọi send_message, disconnect.
Mỗi dòng nhập được gửi thành một tin; hai lệnh riêng:
  /quit        ngắt kết nối và thoát
  /cho <giây>  tạm dừng đọc bàn phím (dùng trong kịch bản chạy minh họa)
Bàn phím được đọc trên một luồng phụ và chuyển vào asyncio.Queue, nên vòng nhận vẫn chạy
trong lúc chờ người dùng gõ.

Chạy:  python3 chat_cli.py <tên> [host] [port]      (mặc định 127.0.0.1 9200)
"""
import asyncio
import sys
import threading

from chat_client import ChatClient
from khung_json_async import NetError, parse_endpoint

ROOM = "chung"                                     # phòng đang chọn; server tối giản chỉ có phòng "chung"


def start_console_reader(loop, q):
    def reader():
        for line in sys.stdin:
            loop.call_soon_threadsafe(q.put_nowait, line.rstrip("\n"))
        loop.call_soon_threadsafe(q.put_nowait, None)      # hết dữ liệu vào (Ctrl+D)
    threading.Thread(target=reader, daemon=True).start()


def show_message(me, msg):                                 # trình xử lý sự kiện on_message
    t = msg.get("type")
    if t == "msg":
        print(f"[{me}] {msg.get('from')}: {msg.get('text')}", flush=True)
    elif t == "sys":
        print(f"[{me}] (hệ thống) {msg.get('text')}", flush=True)
    elif t == "welcome":
        print(f"[{me}] Server chào mừng; đang trực tuyến: {', '.join(msg.get('users', []))}", flush=True)
    elif t == "error":
        print(f"[{me}] Lỗi {msg.get('code')}: {msg.get('text')}", flush=True)
    else:
        print(f"[{me}] Nhận: {msg}", flush=True)


async def main():
    if len(sys.argv) < 2:
        sys.exit("Cách dùng: python3 chat_cli.py <tên> [host] [port]")
    me = sys.argv[1]
    host, port = parse_endpoint(sys.argv[2:], "127.0.0.1", 9200)
    c = ChatClient()
    c.on_message = lambda msg: show_message(me, msg)       # đăng ký sự kiện
    c.on_status = lambda text: print(f"[{me}] Trạng thái: {text}", flush=True)
    try:
        await c.connect(host, port, me)
    except NetError as e:
        sys.exit(f"[{me}] Không kết nối được: {e}")
    q = asyncio.Queue()
    start_console_reader(asyncio.get_running_loop(), q)
    try:
        while (line := await q.get()) is not None and line != "/quit":
            if line.startswith("/cho "):
                await asyncio.sleep(float(line.split()[1]))
            elif line:
                await c.send_message(ROOM, line)           # AWAIT SEND_MESSAGE(c, room, text)
    finally:
        c.disconnect()
        print(f"[{me}] Đã ngắt kết nối", flush=True)


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
