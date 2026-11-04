"""Bài 7.3 – Server WebSocket tối giản (asyncio).

Cài đặt Mã giả 7.3 (WS_ACCEPT – bắt tay) và Mã giả 7.4 (WS_READ_FRAME – đọc khung), Mục 7.3.2.
Kế thừa: Mã giả 7.2 (đọc dòng đầu, READ_HEADERS, RESPOND), 3.4 (READ_EXACT), 6.3 (mỗi kết nối một tác vụ).
Server gửi trả khung văn bản "echo: …", trả lời ping bằng pong, đáp khung đóng; khung vi phạm bị đóng
với mã 1009 (quá lớn) hoặc 1002 (khung từ client không che).

Chạy:  python3 ws_server.py [host] [port]      (mặc định 0.0.0.0 8080)
"""
import asyncio
import base64
import hashlib
import json
import struct
import sys

WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
MAX_FRAME = 1048576                                  # 1 MiB (Mã giả 3.6)


class ProtocolError(Exception):
    pass


async def read_line_async(r, max_len=8192):
    """READ_LINE_ASYNC: None khi client đã đóng (Mã giả 3.5)."""
    try:
        line = await r.readuntil(b"\n")
    except asyncio.IncompleteReadError:
        return None
    if len(line.rstrip(b"\n")) > max_len:            # Mã giả 3.5: dòng (không kể LF) dài hơn max_len
        raise ProtocolError("Dòng quá dài")
    return line.rstrip(b"\r\n").decode("utf-8")


async def read_headers_async(r):
    """Mã giả 7.1 – READ_HEADERS (dòng 25–34), phiên bản bất đồng bộ."""
    hdr = {}
    while True:
        line = await read_line_async(r)
        if line is None:                                # dòng 29: đóng giữa phần tiêu đề
            raise ConnectionError("Kết nối đóng khi chưa đọc hết phần tiêu đề")   # ConnectionClosed
        if line == "":                                  # dòng 30: dòng trống – hết
            return hdr
        k, _, v = line.partition(":")
        hdr[k.strip().lower()] = v.strip()


async def respond_400(w, title):
    """RESPOND(conn, 400, PROBLEM(400, title)) của Mã giả 7.2."""
    b = json.dumps({"type": "about:blank", "title": title, "status": 400}, ensure_ascii=False,
                   separators=(",", ":")).encode("utf-8")
    w.write(("HTTP/1.1 400 Bad Request\r\nContent-Type: application/problem+json; charset=utf-8\r\n"
             f"Content-Length: {len(b)}\r\n\r\n").encode("ascii") + b)
    await w.drain()


def base64_decode(text):
    """BASE64_DECODE; chuỗi không phải Base64 hợp lệ cho kết quả rỗng (sẽ bị từ chối)."""
    try:
        return base64.b64decode(text, validate=True)
    except (ValueError, TypeError):                         # binascii.Error là lớp con của ValueError
        return b""


async def ws_accept(w, hdr):
    """Mã giả 7.3: kiểm tra yêu cầu nâng cấp và gửi 101 kèm Sec-WebSocket-Accept."""
    key = hdr.get("sec-websocket-key", "")                  # dòng 3
    if (hdr.get("upgrade", "").lower() != "websocket"       # dòng 4–6
            or hdr.get("sec-websocket-version") != "13"
            or len(base64_decode(key)) != 16):              # khóa: đúng 16 byte
        await respond_400(w, "Sai WebSocket")
        return False
    acc = base64.b64encode(hashlib.sha1((key + WS_GUID).encode("ascii")).digest()).decode("ascii")
    h = "HTTP/1.1 101 Switching Protocols\r\n"
    h += "Upgrade: websocket\r\nConnection: Upgrade\r\n"
    h += f"Sec-WebSocket-Accept: {acc}\r\n\r\n"
    w.write(h.encode("ascii"))
    await w.drain()
    return True                                      # từ đây trao đổi bằng khung WebSocket


async def ws_send(w, opcode, data):
    """Gửi một khung từ server: FIN = 1, không che (server không che dữ liệu)."""
    n = len(data)
    if n < 126:
        head = struct.pack("!BB", 0x80 | opcode, n)
    elif n < 65536:
        head = struct.pack("!BBH", 0x80 | opcode, 126, n)
    else:
        head = struct.pack("!BBQ", 0x80 | opcode, 127, n)
    w.write(head + data)
    await w.drain()


async def ws_close(w, code):
    """WS_CLOSE(s, code): khung opcode 0x8 chứa mã đóng 2 byte."""
    await ws_send(w, 0x8, struct.pack("!H", code))


async def ws_read_frame(r, w, from_client):
    """Mã giả 7.4: đọc một khung WebSocket."""
    b = await r.readexactly(2)
    fin = (b[0] & 0x80) != 0
    opcode = b[0] & 0x0F
    masked = (b[1] & 0x80) != 0
    n = b[1] & 0x7F
    if n == 126:
        n = struct.unpack("!H", await r.readexactly(2))[0]
    if n == 127:
        n = struct.unpack("!Q", await r.readexactly(8))[0]
    if n > MAX_FRAME:                                # kể cả khi bit cao = 1
        await ws_close(w, 1009)                      # 1009: thông điệp quá lớn
        raise ProtocolError("Khung quá lớn")
    if from_client and not masked:
        await ws_close(w, 1002)                      # 1002: vi phạm giao thức
        raise ProtocolError("Khung client phải được che")
    mask = await r.readexactly(4) if masked else None
    data = bytearray(await r.readexactly(n))
    if masked:
        for i in range(n):
            data[i] ^= mask[i % 4]                   # XOR với byte khóa ở vị trí i MOD 4
    return fin, opcode, bytes(data)


async def session(r, w):
    a = w.get_extra_info("peername")
    addr = f"{a[0]}:{a[1]}"
    try:
        line = await read_line_async(r)
        if line is None:                                    # client đóng trước khi gửi yêu cầu
            return
        hdr = await read_headers_async(r)
        if not await ws_accept(w, hdr):
            print(f"[server] {addr} {line} -> 400 (không phải yêu cầu WebSocket)", flush=True)
            return
        print(f"[server] Bắt tay WebSocket thành công với {addr}", flush=True)
        message, msg_type = b"", 0
        while True:
            fin, opcode, data = await ws_read_frame(r, w, from_client=True)
            if opcode == 0x8:                        # đóng: gửi lại mã đóng rồi kết thúc
                code = struct.unpack("!H", data[:2])[0] if len(data) >= 2 else 1005
                print(f"[server] {addr} gửi khung đóng, mã {code}", flush=True)
                await ws_close(w, 1000 if code == 1005 else code)
                return
            if opcode == 0x9:                        # ping -> pong cùng dữ liệu
                print(f"[server] Nhận ping: '{data.decode('utf-8')}' -> gửi pong", flush=True)
                await ws_send(w, 0xA, data)
                continue
            if opcode in (0x1, 0x2):
                message, msg_type = data, opcode
            elif opcode == 0x0:                      # khung tiếp nối
                message += data
            if not fin:
                continue
            if msg_type == 0x1:
                text = message.decode("utf-8")
                print(f"[server] Nhận văn bản ({len(message)} byte): '{text[:40]}'", flush=True)
                await ws_send(w, 0x1, ("echo: " + text).encode("utf-8"))
            else:
                print(f"[server] Nhận nhị phân ({len(message)} byte)", flush=True)
                await ws_send(w, 0x2, message)
    except ProtocolError as e:
        print(f"[server] Lỗi giao thức với {addr}: {e}", flush=True)
    except (asyncio.IncompleteReadError, ConnectionError) as e:
        print(f"[server] Kết nối {addr} đóng đột ngột: {e!r}", flush=True)
    finally:
        w.close()
        print(f"[server] Đóng kết nối {addr}", flush=True)


async def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "0.0.0.0"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 8080
    srv = await asyncio.start_server(session, host, port)
    print(f"[server] Đang lắng nghe tại {host}:{port}", flush=True)
    async with srv:
        await srv.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
