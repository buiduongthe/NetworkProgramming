"""Bài 5.3 – Server tra cứu tối giản phục vụ ứng dụng có giao diện.

Dùng lại server của Bài 5.1 (nghiệp vụ Mã giả 3.7, khung JSON Mã giả 5.1), thêm độ trễ giả lập
delay_ms trước mỗi phản hồi để quan sát giao diện không bị đóng băng khi server trả lời chậm.

Chạy:  python3 lookup_server.py [host] [port] [delay_ms]      (mặc định 0.0.0.0 9100 0)
"""
import asyncio
import json
import sys

from khung_json_async import (ConnectionClosed, ProtocolError, json_bytes, parse_endpoint,
                              recv_json_async, send_json_async)

DELAY_MS = 0
DICT = {                                                   # từ điển nhỏ thay cho glossary.json
    "TCP": "Giao thức truyền tin cậy, hướng kết nối",
    "UDP": "Giao thức datagram, không kết nối",
    "Socket": "Điểm cuối truyền thông của một chương trình",
    "Bất đồng bộ": "Khởi động vào/ra rồi làm việc khác, quay lại khi xong",
}


def process_request(req):                                  # Mã giả 3.7: nghiệp vụ, độc lập với socket
    op = req.get("op")
    if op == "lookup":
        if req.get("term") in DICT:
            return {"status": 200, "term": req["term"], "text": DICT[req["term"]]}
        return {"status": 404, "error": "Không có thuật ngữ"}
    if op == "list":
        return {"status": 200, "terms": list(DICT)}
    return {"status": 400, "error": "Sai thao tác"}


async def handle_lookup(reader, writer):                   # HANDLE_LOOKUP, phiên bản bất đồng bộ
    addr = "%s:%d" % writer.get_extra_info("peername")[:2]
    print(f"[server] Client kết nối: {addr}", flush=True)
    try:
        while True:
            try:
                req = await recv_json_async(reader)        # AWAIT: chờ khung mà không chặn luồng
            except ConnectionClosed as e:
                if e.received == 0:                        # đóng tại ranh giới khung: bình thường
                    break
                raise                                      # đóng giữa khung: lỗi
            except (json.JSONDecodeError, UnicodeDecodeError):
                await send_json_async(writer, {"status": 400, "error": "JSON sai"})
                continue
            print(f"[server] Nhận từ {addr}: {json_bytes(req).decode()}", flush=True)
            if req.get("op") == "bye":
                break
            await asyncio.sleep(DELAY_MS / 1000)           # độ trễ giả lập của server
            await send_json_async(writer, process_request(req))
    except (ConnectionClosed, ProtocolError, OSError) as e:   # lỗi một client không dừng server
        print(f"[server] Lỗi với {addr}: {e}", flush=True)
    finally:
        writer.close()
        print(f"[server] Client {addr} đã đóng kết nối", flush=True)


async def main():
    global DELAY_MS
    host, port = parse_endpoint(sys.argv[1:3], "0.0.0.0", 9100)
    DELAY_MS = int(sys.argv[3]) if len(sys.argv) > 3 else 0
    srv = await asyncio.start_server(handle_lookup, host, port, backlog=16)   # BIND + LISTEN
    print(f"[server] Đang lắng nghe tại {host}:{port}, độ trễ giả lập {DELAY_MS} ms", flush=True)
    async with srv:
        await srv.serve_forever()                          # mỗi kết nối mới: một tác vụ handle_lookup


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
