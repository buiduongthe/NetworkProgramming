"""Bài 5.1 – Server tra cứu bất đồng bộ dùng khung JSON (minh họa Mã giả 5.1).

Server dùng lại nghiệp vụ PROCESS_REQUEST của Mã giả 3.7 (dịch vụ tra cứu, cổng mặc định 9100),
nhưng phần truyền nhận dùng RECV_JSON_ASYNC, SEND_JSON_ASYNC (Mục 5.2.4). Mỗi kết nối là một
tác vụ asyncio nên server phục vụ được nhiều client cùng lúc trên một luồng.

Chạy:  python3 frame_server.py [host] [port]      (mặc định 0.0.0.0 9100)
"""
import asyncio
import json
import sys

from khung_json_async import (ConnectionClosed, ProtocolError, json_bytes, parse_endpoint,
                              recv_json_async, send_json_async)

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
            await send_json_async(writer, process_request(req))
    except (ConnectionClosed, ProtocolError, OSError) as e:   # lỗi một client không dừng server
        print(f"[server] Lỗi với {addr}: {e}", flush=True)
    finally:
        writer.close()
        print(f"[server] Client {addr} đã đóng kết nối", flush=True)


async def main():
    host, port = parse_endpoint(sys.argv[1:], "0.0.0.0", 9100)
    srv = await asyncio.start_server(handle_lookup, host, port, backlog=16)   # BIND + LISTEN
    print(f"[server] Đang lắng nghe tại {host}:{port}", flush=True)
    async with srv:
        await srv.serve_forever()                          # mỗi kết nối mới: một tác vụ handle_lookup


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
