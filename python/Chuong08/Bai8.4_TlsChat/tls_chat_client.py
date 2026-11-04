"""Bài 8.4 – Client chat tối giản có TLS và xác thực token.

Cài đặt Mã giả 8.6 phía client (CONNECT: SAFE_CONNECT_ASYNC rồi TLS_WRAP_ASYNC với server_name ← host;
Mục 8.4.5 Giáo trình Lập trình mạng), kế thừa Mã giả 5.5 (ChatClient: RECEIVE_LOOP, SEND_MESSAGE có khóa).
Client gửi hello kèm token, vào phòng "chung", gửi từng dòng đọc từ bàn phím, rồi gửi ping và ngắt khi nhận pong.
Tệp CA: nếu chỉ định, client tin cậy CA nội bộ đó; nếu không, dùng kho tin cậy của hệ điều hành.

Chạy:  python3 tls_chat_client.py <host> <port> <tệp_token> [tệp_CA]
Ví dụ: python3 tls_chat_client.py localhost 9200 certs/an.jwt certs/ca.crt
"""
import asyncio
import ssl
import sys

from khung import ProtocolError, recv_json_async, send_json_async


def log(text):
    print(f"[client] {text}", flush=True)


class ChatClient:
    def __init__(self):
        self.reader = self.writer = None
        self.send_lock = asyncio.Lock()                       # mỗi lúc gửi một khung
        self.welcome = asyncio.Event()                        # đã nhận welcome hoặc lỗi
        self.pong = asyncio.Event()
        self.ok = False

    async def connect(self, host, port, token, ctx):
        """CONNECT của Mã giả 5.5 + 8.6: kết nối TCP, bọc TLS, gửi hello kèm token."""
        self.reader, self.writer = await asyncio.wait_for(asyncio.open_connection(host, port), 3)  # SAFE_CONNECT_ASYNC
        await self.writer.start_tls(ctx, server_hostname=host)   # TLS_WRAP_ASYNC(raw, CLIENT, server_name ← host)
        tls = self.writer.get_extra_info("ssl_object")
        subject = dict(x[0] for x in tls.getpeercert()["subject"])
        log(f"Đã kết nối TLS tới {host}:{port}: {tls.version()}, {tls.cipher()[0]}")
        log(f"Chứng chỉ server CN={subject.get('commonName')} hợp lệ, tên '{host}' khớp SAN")
        await self.send({"type": "hello", "token": token})
        asyncio.create_task(self.receive_loop())              # RUN_BACKGROUND: không AWAIT

    async def receive_loop(self):
        try:
            while True:
                m = await recv_json_async(self.reader)
                t = m.get("type")
                if t == "welcome":
                    self.ok = True
                    log(f"Server chấp nhận: tên = {m['name']} (từ token), trực tuyến: {', '.join(m['users'])}")
                    self.welcome.set()
                elif t == "error":
                    log(f"Lỗi từ server: {m.get('code')} {m.get('text', '')}")
                    self.welcome.set()
                elif t == "sys":
                    log(f"* {m['text']}")
                elif t == "msg":
                    log(f"[{m['room']}] {m['from']}: {m['text']}")
                elif t == "pong":
                    self.pong.set()
        except (asyncio.IncompleteReadError, OSError, ProtocolError, ValueError):
            log("Server đã đóng kết nối")
            self.welcome.set()
            self.pong.set()

    async def send(self, obj):                                # SEND_MESSAGE: khóa gửi
        async with self.send_lock:
            await send_json_async(self.writer, obj)

    def disconnect(self):
        if self.writer is not None:
            self.writer.close()


async def main():
    if len(sys.argv) < 4:
        sys.exit("Cách dùng: python3 tls_chat_client.py <host> <port> <tệp_token> [tệp_CA]")
    host, port, token_file = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    ca_file = sys.argv[4] if len(sys.argv) > 4 else None
    token = open(token_file, encoding="ascii").read().strip()
    ctx = ssl.create_default_context(cafile=ca_file)          # kiểm tra chứng chỉ và tên máy (mặc định)
    ctx.minimum_version = ssl.TLSVersion.TLSv1_2
    log(f"Tin cậy: {'CA trong ' + ca_file if ca_file else 'kho chứng chỉ của hệ điều hành'}; token: {token_file}")
    c = ChatClient()
    try:
        await c.connect(host, port, token, ctx)
    except ssl.SSLCertVerificationError as e:
        log(f"Bắt tay TLS thất bại: {e.verify_message}")
        c.disconnect()
        return 1
    except (OSError, asyncio.TimeoutError) as e:
        log(f"Không kết nối được: {e}")
        return 1
    await asyncio.wait_for(c.welcome.wait(), 5)
    if not c.ok:
        c.disconnect()
        return 1
    await c.send({"type": "join", "room": "chung"})
    lines = await asyncio.to_thread(sys.stdin.read)           # READ_CONSOLE: đọc các dòng cần gửi
    for line in lines.splitlines():
        await c.send({"type": "msg", "room": "chung", "text": line})
    await c.send({"type": "ping"})                            # pong về sau mọi tin trước đó
    await asyncio.wait_for(c.pong.wait(), 5)
    c.disconnect()                                            # DISCONNECT
    log("Đã ngắt kết nối")
    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
