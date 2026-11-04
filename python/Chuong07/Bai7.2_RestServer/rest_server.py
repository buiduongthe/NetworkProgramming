"""Bài 7.2 – Server REST tối giản trên HTTP/1.1 (asyncio).

Cài đặt Mã giả 7.2 (Mục 7.2.3, Giáo trình Lập trình mạng): HTTP_SESSION, ROUTE, RESPOND.
Kế thừa: Mã giả 6.3 (server bất đồng bộ), 7.1 (READ_HEADERS), 3.7 (từ điển DICT nạp từ glossary.json).
Bổ sung theo bài tập cuối chương: POST /api/v1/terms (201 + Location) và DELETE /api/v1/terms/{term} (204).

Chạy:  python3 rest_server.py [host] [port]      (mặc định 0.0.0.0 8080)
"""
import asyncio
import json
import os
import sys
from urllib.parse import unquote, quote

MAX_BODY = 1048576                                   # 1 MiB, như MAX_FRAME của Mã giả 3.6
HERE = os.path.dirname(os.path.abspath(__file__))
DICT = json.load(open(os.path.join(HERE, "glossary.json"), encoding="utf-8"))
REASON = {200: "OK", 201: "Created", 204: "No Content", 400: "Bad Request", 404: "Not Found",
          405: "Method Not Allowed", 409: "Conflict", 413: "Content Too Large", 501: "Not Implemented"}


def problem(status, title):
    """PROBLEM(status, title) theo RFC 9457."""
    return {"type": "about:blank", "title": title, "status": status}


async def read_line_async(r, max_len):
    """READ_LINE_ASYNC: StreamReader của asyncio đóng vai Reader (có bộ đệm pending riêng)."""
    try:
        line = await r.readuntil(b"\n")
    except asyncio.IncompleteReadError as e:
        if not e.partial:
            return None                              # đối tác đã đóng
        line = e.partial
    except asyncio.LimitOverrunError:
        raise ValueError("Dòng quá dài")
    if len(line.rstrip(b"\n")) > max_len:            # Mã giả 3.5: dòng (không kể LF) dài hơn max_len
        raise ValueError("Dòng quá dài")
    return line.rstrip(b"\r\n").decode("utf-8")


async def read_headers_async(r):
    """Mã giả 7.1 – READ_HEADERS (dòng 25–34), phiên bản bất đồng bộ."""
    hdr = {}
    while True:
        line = await read_line_async(r, 8192)
        if line is None:                             # dòng 29: client đóng giữa phần tiêu đề
            raise ConnectionError("Kết nối đóng khi chưa đọc hết phần tiêu đề")   # ConnectionClosed
        if line == "":                               # dòng 30: dòng trống – hết
            return hdr
        k, _, v = line.partition(":")
        hdr[k.strip().lower()] = v.strip()


async def respond(w, status, obj, extra=None):
    """Mã giả 7.2, dòng 38–48: Content-Length là số byte sau khi mã hóa UTF-8."""
    h = f"HTTP/1.1 {status} {REASON[status]}\r\n"
    for k, v in (extra or {}).items():
        h += f"{k}: {v}\r\n"
    if status == 204:                                # dòng 40–41: 204 không có thân, không có Content-Length
        w.write((h + "\r\n").encode("ascii"))
    else:
        b = json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        ct = "application/problem+json" if status >= 400 else "application/json"
        h += f"Content-Type: {ct}; charset=utf-8\r\n"
        h += f"Content-Length: {len(b)}\r\n\r\n"
        w.write(h.encode("ascii") + b)
    await w.drain()                                  # SEND_ALL_ASYNC


def route(method, path, body):
    """Mã giả 7.2, dòng 23–37 (thêm POST, DELETE)."""
    p = path.strip("/").split("/")                   # ["api","v1","terms",…]
    if p[0:3] != ["api", "v1", "terms"] or len(p) > 4:
        return 404, problem(404, "Không có tài nguyên"), None
    if method == "GET" and len(p) == 3:
        return 200, {"terms": list(DICT.keys())}, None
    if method == "GET" and len(p) == 4:
        t = unquote(p[3])                            # URL_DECODE: %20 -> dấu cách
        if t in DICT:
            return 200, {"term": t, "text": DICT[t]}, None
        return 404, problem(404, "Không có thuật ngữ"), None
    if method == "POST" and len(p) == 3:             # mở rộng: thêm thuật ngữ
        try:
            obj = json.loads(body.decode("utf-8"))
            t, text = obj["term"], obj["text"]
        except (ValueError, KeyError, TypeError):
            return 400, problem(400, "JSON sai hoặc thiếu trường term, text"), None
        if t in DICT:
            return 409, problem(409, "Thuật ngữ đã tồn tại"), None
        DICT[t] = text
        return 201, {"term": t, "text": text}, {"Location": "/api/v1/terms/" + quote(t)}
    if method == "DELETE" and len(p) == 4:           # mở rộng: xóa thuật ngữ
        t = unquote(p[3])
        if DICT.pop(t, None) is None:
            return 404, problem(404, "Không có thuật ngữ"), None
        return 204, None, None
    return 405, problem(405, "Phương thức không hỗ trợ"), None


async def http_session(r, w):
    """Mã giả 7.2, dòng 1–22: phiên keep-alive, nhiều yêu cầu trên một kết nối."""
    a = w.get_extra_info("peername")
    addr = f"{a[0]}:{a[1]}"
    try:
        while True:
            line = await read_line_async(r, 8192)
            if line is None:
                return                               # client đóng kết nối
            parts = line.split(" ")
            if len(parts) != 3:
                await respond(w, 400, problem(400, "Dòng yêu cầu sai"))
                return
            method, target, version = parts
            hdr = await read_headers_async(r)
            n = int(hdr.get("content-length", "0"))  # dòng 8: TO_INT64 (int của Python không giới hạn)
            if "transfer-encoding" in hdr:           # chưa hỗ trợ chunked
                print(f"[server] {addr} {method} {target} -> 501", flush=True)
                await respond(w, 501, problem(501, "Chưa hỗ trợ"))
                return                               # độ dài thân không xác định: đóng
            if n < 0 or n > MAX_BODY:                # dòng 13
                print(f"[server] {addr} {method} {target} -> 413", flush=True)
                await respond(w, 413, problem(413, "Thân quá lớn"))
                return
            body = await r.readexactly(n)            # READ_EXACT_ASYNC
            status, obj, extra = route(method, target.split("?")[0], body)   # PATH(target): bỏ query
            print(f"[server] {addr} {method} {target} -> {status}", flush=True)
            await respond(w, status, obj, extra)
            if hdr.get("connection", "").lower() == "close":
                return
    except (ConnectionError, ValueError, asyncio.IncompleteReadError) as e:
        print(f"[server] Kết thúc phiên {addr}: {e}", flush=True)
    finally:
        w.close()
        print(f"[server] Đóng kết nối {addr}", flush=True)


async def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "0.0.0.0"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 8080
    srv = await asyncio.start_server(http_session, host, port, limit=65536)   # mỗi kết nối một tác vụ
    print(f"[server] Đang lắng nghe tại {host}:{port}", flush=True)
    async with srv:
        await srv.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
