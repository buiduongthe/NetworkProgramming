"""Bài 5.5 – Server nhận tệp có kiểm tra mã băm (đi kèm để chạy thử PUT_FILE_ASYNC).

Cài đặt Mã giả 3.9 (HANDLE_PUT, Mục 3.5.2) trên nền bất đồng bộ của Mã giả 5.1: ghi vào tệp
tạm .part, chỉ đổi tên khi mã băm SHA-256 khớp; kết nối đóng giữa chừng thì xóa tệp .part.
Tham số rate_kib giới hạn tốc độ đọc (KiB/s) để mô phỏng đường truyền chậm; 0 = không giới hạn.

Chạy:  python3 put_server.py [host] [port] [rate_kib]      (mặc định 0.0.0.0 9100 0)
"""
import asyncio
import hashlib
import os
import socket
import sys

from khung_json_async import (ConnectionClosed, ProtocolError, parse_endpoint,
                              recv_json_async, send_json_async)

MAX_FILE = 100 * 1048576
UPLOAD_DIR = "uploads"
RATE_KIB = 0


def is_hex(v, n):
    """IS_HEX(v, n): v là chuỗi đúng n ký tự thập lục phân."""
    return isinstance(v, str) and len(v) == n and all(c in "0123456789abcdefABCDEF" for c in v)


async def handle_put(reader, writer, req):                 # Mã giả 3.9
    name = str(req.get("name", "")).replace("\\", "/").split("/")[-1]   # dòng 3: BASENAME(REPLACE(…))
    size = req.get("size", -1)
    if (name in ("", ".", "..") or not isinstance(size, int) or isinstance(size, bool)   # dòng 4–6
            or size < 0 or size > MAX_FILE or not is_hex(req.get("sha256"), 64)):
        await send_json_async(writer, {"status": 400, "error": "Sai tham số"})
        return
    print(f"[server] Nhận yêu cầu put: {name}, {size} byte", flush=True)
    await send_json_async(writer, {"status": 100})         # sẵn sàng nhận nội dung
    os.makedirs(UPLOAD_DIR, exist_ok=True)
    tmp = os.path.join(UPLOAD_DIR, name + ".part")
    h, remaining = hashlib.sha256(), size
    f = open(tmp, "wb")
    try:                                                   # dòng 14–26
        while remaining > 0:
            chunk = await reader.read(min(65536, remaining))
            if len(chunk) == 0:
                raise ConnectionClosed(size - remaining)
            f.write(chunk)
            h.update(chunk)
            remaining -= len(chunk)
            if RATE_KIB > 0:                               # mô phỏng đường truyền chậm
                await asyncio.sleep(len(chunk) / (RATE_KIB * 1024))
    except BaseException:                                  # CATCH ANY (kể cả tác vụ bị hủy): xóa tệp dở dang
        f.close()
        os.remove(tmp)
        print(f"[server] Lỗi khi mới nhận {size - remaining}/{size} byte: xóa {tmp}", flush=True)
        raise
    f.close()
    if h.hexdigest() == req.get("sha256"):
        os.replace(tmp, os.path.join(UPLOAD_DIR, name))
        print(f"[server] Đã lưu {UPLOAD_DIR}/{name}, mã băm khớp (200)", flush=True)
        await send_json_async(writer, {"status": 200, "saved": name})
    else:
        os.remove(tmp)
        print("[server] Mã băm không khớp (422)", flush=True)
        await send_json_async(writer, {"status": 422, "error": "Mã băm không khớp"})


async def session(reader, writer):
    addr = "%s:%d" % writer.get_extra_info("peername")[:2]
    print(f"[server] Client kết nối: {addr}", flush=True)
    try:
        while True:
            try:
                req = await recv_json_async(reader)
            except ConnectionClosed as e:
                if e.received == 0:
                    break
                raise
            except ValueError:                             # Mã giả 3.7: JsonError → 400, tiếp tục
                await send_json_async(writer, {"status": 400, "error": "JSON sai"})
                continue
            if not isinstance(req, dict):                  # Mã giả 3.7 dòng 24–27
                await send_json_async(writer, {"status": 400, "error": "JSON sai"})
                continue
            if req.get("op") == "put":
                await handle_put(reader, writer, req)
            elif req.get("op") == "bye":
                break
            else:
                await send_json_async(writer, {"status": 400, "error": "Sai thao tác"})
    except (ConnectionClosed, ProtocolError, OSError) as e:
        print(f"[server] Lỗi với {addr}: {e}", flush=True)
    finally:
        writer.close()
        print(f"[server] Client {addr} đã đóng kết nối", flush=True)


async def main():
    global RATE_KIB
    host, port = parse_endpoint(sys.argv[1:3], "0.0.0.0", 9100)
    RATE_KIB = int(sys.argv[3]) if len(sys.argv) > 3 else 0
    ls = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    if RATE_KIB > 0:                                       # bộ đệm nhận nhỏ: mô phỏng đường truyền chậm
        ls.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 65536)
    ls.bind((host, port))
    srv = await asyncio.start_server(session, sock=ls)    # LISTEN trên socket đã cấu hình
    print(f"[server] Đang lắng nghe tại {host}:{port}, giới hạn đọc {RATE_KIB} KiB/s", flush=True)
    async with srv:
        await srv.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
