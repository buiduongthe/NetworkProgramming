"""Bài 5.6 – Server tra cứu "khó tính" để chạy thử hạn chót và thử lại.

Server dùng khung JSON của Mã giả 5.1 và cố ý gặp sự cố ở các yêu cầu đầu tiên:
  yêu cầu #1: trả lời chậm 2000 ms (vượt hạn chót của client);
  yêu cầu #2: trả lời {status: 503, retry_after_ms: 300} – quá tải tạm thời;
  từ yêu cầu #3: trả lời bình thường. Trường delay_ms (nếu có) là thời gian xử lý giả lập.

Chạy:  python3 flaky_server.py [host] [port]      (mặc định 0.0.0.0 9100)
"""
import asyncio
import sys

from khung_json_async import (ConnectionClosed, ProtocolError, parse_endpoint,
                              recv_json_async, send_json_async)

DICT = {"TCP": "Giao thức truyền tin cậy, hướng kết nối", "UDP": "Giao thức datagram, không kết nối"}
count = 0                                                  # số yêu cầu đã nhận


def lookup(term):
    if term in DICT:
        return {"status": 200, "term": term, "text": DICT[term]}
    return {"status": 404, "error": "Không có thuật ngữ"}


async def answer(req):
    global count
    count += 1
    n = count
    if n == 1:
        print(f"[server] Yêu cầu #{n}: cố ý trả lời chậm 2000 ms", flush=True)
        await asyncio.sleep(2.0)
        return lookup(req.get("term"))
    if n == 2:
        print(f"[server] Yêu cầu #{n}: trả lời 503, đề nghị chờ 300 ms", flush=True)
        return {"status": 503, "error": "Quá tải tạm thời", "retry_after_ms": 300}
    delay = req.get("delay_ms", 0)
    print(f"[server] Yêu cầu #{n}: xử lý {delay} ms rồi trả lời", flush=True)
    await asyncio.sleep(delay / 1000)
    return lookup(req.get("term"))


async def session(reader, writer):
    try:
        while True:
            req = await recv_json_async(reader)
            resp = await answer(req)
            await send_json_async(writer, resp)
    except ConnectionClosed as e:
        if e.received:
            print(f"[server] Kết nối đóng giữa khung: {e}", flush=True)
    except (ProtocolError, OSError, ValueError) as e:
        print(f"[server] Không gửi được phản hồi, client đã đóng kết nối ({type(e).__name__})", flush=True)
    finally:
        writer.close()


async def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9100)
    srv = await asyncio.start_server(session, host, port)
    print(f"[server] Đang lắng nghe tại {host}:{port}", flush=True)
    async with srv:
        await srv.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
