"""Bài 8.4 – Server chat tối giản có TLS và xác thực token.

Cài đặt Mã giả 8.6 phía server (TLS_SESSION, VERIFY_JWT trong hello; Mục 8.4.5 Giáo trình Lập trình mạng),
kế thừa Mã giả 6.5–6.6 (CHAT_SESSION, BROADCAST, WRITER – bản rút gọn: hello, join, msg, ping).
Cấu hình: TLS_CERT, TLS_KEY (tệp PEM của chứng chỉ và khóa riêng), TLS_KEY_PASS (nếu khóa có mật khẩu),
CHAT_JWT_KEY (khóa ký JWT). Tên người dùng lấy từ claim sub của token đã kiểm tra.

Chạy:  python3 tls_chat_server.py [host] [port]      (mặc định 0.0.0.0 9200)
"""
import asyncio
import os
import ssl
import sys
import time

from jwt_hs256 import ISSUER, JwtError, secret, verify_jwt
from khung import ProtocolError, recv_json_async, send_json_async

users, rooms = {}, {}       # asyncio chạy một luồng: thao tác không có await là nguyên tử, không cần khóa


class HelloError(Exception):
    def __init__(self, code, text):
        super().__init__(text)
        self.code, self.text = code, text


class Client:
    def __init__(self, name, writer):
        self.name, self.writer = name, writer
        self.outbox = asyncio.Queue(256)                       # BOUNDED_QUEUE(256)


def log(text):
    print(f"[server] {text}", flush=True)


def load_cert_and_key():
    """LOAD_CERT_AND_KEY(ENV("TLS_CERT"), SECRET("TLS_KEY_PASS")); TLS_WRAP(..., min ← TLS1_2)."""
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.minimum_version = ssl.TLSVersion.TLSv1_2              # tối thiểu TLS 1.2, thư viện ưu tiên TLS 1.3
    ctx.load_cert_chain(os.environ["TLS_CERT"], os.environ.get("TLS_KEY"),
                        password=os.environ.get("TLS_KEY_PASS"))
    return ctx


def enqueue(c, msg):
    try:
        c.outbox.put_nowait(msg)
    except asyncio.QueueFull:                                  # client chậm: ngắt kết nối
        log(f"Ngắt client chậm {c.name}")
        c.writer.close()


def send_to(name, msg, sender):
    c = users.get(name)
    if c is not None:
        enqueue(c, msg)
    elif sender is not None:
        enqueue(sender, {"type": "error", "code": 404, "text": "Không có người nhận"})


def broadcast(room, msg, sender):
    targets = set(rooms.get(room, ()))                         # sao chép danh sách thành viên
    if sender is not None and sender.name not in targets:
        enqueue(sender, {"type": "error", "code": 403, "text": "Chưa tham gia phòng"})
        return
    for name in targets:
        send_to(name, msg, None)


async def writer_task(c):                                      # tác vụ ghi duy nhất của mỗi client
    try:
        while (msg := await c.outbox.get()) is not None:
            await send_json_async(c.writer, msg)
    except OSError:
        c.writer.close()


async def chat_session(reader, writer, addr):
    """CHAT_SESSION (Mã giả 6.5) chạy trên kết nối đã bọc TLS; hello mang token thay cho tên."""
    c, task = None, None
    try:
        h = await asyncio.wait_for(recv_json_async(reader), 10)
        if h.get("type") != "hello":
            raise HelloError(400, "Thông điệp đầu phải là hello")
        try:
            claims = verify_jwt(h.get("token"), secret("CHAT_JWT_KEY"), ISSUER, "chat")
        except JwtError as e:
            raise HelloError(401, f"Token không hợp lệ: {e}")
        if claims["sub"] in users:                             # REGISTER(claims.sub, conn)
            raise HelloError(409, "Tên đã được sử dụng")
        c = users[claims["sub"]] = Client(claims["sub"], writer)
        log(f"{c.name} đăng nhập từ {addr} (tên lấy từ claim sub)")
        task = asyncio.create_task(writer_task(c))
        enqueue(c, {"type": "welcome", "name": c.name, "users": sorted(users)})
        while True:
            m = await asyncio.wait_for(recv_json_async(reader), 90)
            t = m.get("type")
            if t == "join":
                rooms.setdefault(m.get("room"), set()).add(c.name)
                broadcast(m.get("room"), {"type": "sys", "text": f"{c.name} đã vào phòng {m.get('room')}"}, None)
            elif t == "msg":
                broadcast(m.get("room"), {"type": "msg", "from": c.name, "room": m.get("room"),
                                          "text": m.get("text"), "ts": time.time()}, c)
            elif t == "ping":
                enqueue(c, {"type": "pong"})
            else:
                enqueue(c, {"type": "error", "code": 400, "text": "Loại thông điệp không hỗ trợ"})
    except HelloError as e:                                    # lời chào không hợp lệ
        log(f"Từ chối {addr}: {e.code} {e.text}")
        await send_json_async(writer, {"type": "error", "code": e.code, "text": e.text})
    except (asyncio.IncompleteReadError, asyncio.TimeoutError, OSError, ProtocolError, ValueError) as e:
        if c is None:                                          # kết thúc phiên bình thường hoặc do lỗi
            log(f"Kết nối {addr} đóng trước khi đăng nhập: {type(e).__name__}")
    finally:
        if c is not None:                                      # UNREGISTER, đóng hộp thư đi
            users.pop(c.name, None)
            for members in rooms.values():
                members.discard(c.name)
            if not c.outbox.full():
                c.outbox.put_nowait(None)                          # None: báo tác vụ ghi kết thúc
            if task is not None:
                await asyncio.wait([task], timeout=1)
            log(f"{c.name} rời phiên")
        writer.close()


async def tls_session(reader, writer, ctx):
    """Mã giả 8.6 – TLS_SESSION(conn, addr): thay SERVE_ASYNC; bắt tay TLS trước phiên chat."""
    peer = writer.get_extra_info("peername")
    addr = f"{peer[0]}:{peer[1]}"
    try:
        await writer.start_tls(ctx, ssl_handshake_timeout=10)        # TLS_WRAP_ASYNC(conn, SERVER, cert)
    except (ssl.SSLError, OSError, asyncio.TimeoutError) as e:
        log(f"Bắt tay TLS lỗi {addr}: {getattr(e, 'reason', None) or type(e).__name__}")
        writer.close()
        return
    tls = writer.get_extra_info("ssl_object")
    log(f"Bắt tay TLS thành công {addr}: {tls.version()}, {tls.cipher()[0]}")
    await chat_session(reader, writer, addr)                         # tls thay cho conn


async def main():
    args = sys.argv[1:]
    host = args[0] if len(args) > 0 else os.environ.get("APP_HOST", "0.0.0.0")   # Mã giả 2.1 (rút gọn)
    port = int(args[1]) if len(args) > 1 else int(os.environ.get("APP_PORT", 9200))
    ctx = load_cert_and_key()                                        # nạp một lần khi khởi động
    srv = await asyncio.start_server(lambda r, w: tls_session(r, w, ctx), host, port, backlog=16)
    log(f"Đang lắng nghe (TLS ≥ 1.2) tại {host}:{port}")
    async with srv:
        await srv.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        log("Dừng server")
