"""Bài 5.4 – Server chat tối giản để chạy thử lớp ChatClient.

Server chỉ cài đặt phần giao thức chat (Mục 6.3.1) mà Mã giả 5.5 dùng tới: hello → welcome
(lỗi 409 nếu trùng tên), msg → chuyển tiếp tới mọi người trong một phòng chung, ping → pong,
thông báo sys khi có người vào, ra. Bản đầy đủ (nhiều phòng, tin riêng, hộp thư đi cho mỗi
client) là Mã giả 6.x ở Chương 6. Mỗi client có một khóa gửi để các khung không xen kẽ.

Chạy:  python3 chat_server.py [host] [port]      (mặc định 0.0.0.0 9200)
"""
import asyncio
import sys
import time

from khung_json_async import (ConnectionClosed, ProtocolError, parse_endpoint,
                              recv_json_async, send_json_async)

users = {}                                                 # tên → (writer, khóa gửi)


async def send_to(name, obj):
    writer, lock = users[name]
    async with lock:                                       # mỗi socket mỗi lúc chỉ ghi một khung
        await send_json_async(writer, obj)


async def broadcast(obj):
    for name in list(users):                               # sao chép danh sách rồi mới gửi
        try:
            await send_to(name, obj)
        except (OSError, KeyError):
            pass                                           # client vừa rời: bỏ qua


async def chat_session(reader, writer):
    name = None
    try:
        h = await asyncio.wait_for(recv_json_async(reader), 10)   # hello trong 10 giây
        if h.get("type") != "hello" or not h.get("name"):
            await send_json_async(writer, {"type": "error", "code": 400, "text": "Cần hello"})
            return
        if h["name"] in users:
            await send_json_async(writer, {"type": "error", "code": 409, "text": "Tên đã được dùng"})
            return
        name = h["name"]
        users[name] = (writer, asyncio.Lock())
        print(f"[server] {name} đã kết nối", flush=True)
        await send_to(name, {"type": "welcome", "users": list(users)})
        await broadcast({"type": "sys", "text": f"{name} đã vào phòng"})
        while True:
            m = await asyncio.wait_for(recv_json_async(reader), 90)
            if m.get("type") == "msg":
                print(f"[server] {name}: {m.get('text')}", flush=True)
                await broadcast({"type": "msg", "from": name, "room": m.get("room", "chung"),
                                 "text": m.get("text"), "ts": time.strftime("%H:%M:%S")})
            elif m.get("type") == "ping":
                await send_to(name, {"type": "pong"})
            else:
                await send_to(name, {"type": "error", "code": 400, "text": "Sai loại thông điệp"})
    except (ConnectionClosed, ProtocolError, TimeoutError, OSError, ValueError):
        pass                                               # kết thúc phiên bình thường hoặc do lỗi
    finally:
        writer.close()
        if name is not None:
            del users[name]
            print(f"[server] {name} đã rời phòng", flush=True)
            await broadcast({"type": "sys", "text": f"{name} đã rời phòng"})


async def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9200)
    srv = await asyncio.start_server(chat_session, host, port)
    print(f"[server] Đang lắng nghe tại {host}:{port}", flush=True)
    async with srv:
        await srv.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
