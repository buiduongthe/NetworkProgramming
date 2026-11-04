"""Bài mở rộng MR5.2 – Máy chủ thử có độ trễ giả lập (chỉ chạy cục bộ).

Giao thức dòng văn bản trên TCP: client gửi "GET <k>\\n", máy chủ chờ <trễ> mili giây (giả lập thời gian
xử lý/đường truyền) rồi trả "OK <k>\\n". Máy chủ dùng asyncio nên phục vụ được nhiều kết nối cùng lúc.
Sau mỗi "đợt" yêu cầu (máy chủ rỗi 300 ms), in số yêu cầu và số kết nối đồng thời lớn nhất của đợt –
nhờ đó thấy được mỗi mô hình phía client gửi đồng thời bao nhiêu yêu cầu.

Chạy:  python3 may_chu_tre.py <host> <port> [trễ_ms]     (mặc định 200)
"""
import asyncio
import sys

state = {"active": 0, "peak": 0, "count": 0, "total": 0, "burst": 0}


def log(text):
    print(f"[server] {text}", flush=True)


async def report_when_idle():
    await asyncio.sleep(0.3)
    if state["active"] == 0 and state["count"] > 0:
        state["burst"] += 1
        log(f"đợt {state['burst']:>2}: {state['count']:>2} yêu cầu, đồng thời tối đa {state['peak']:>2}")
        state["count"] = state["peak"] = 0


async def handle(reader, writer, delay):
    state["active"] += 1
    state["count"] += 1
    state["total"] += 1
    state["peak"] = max(state["peak"], state["active"])
    try:
        line = (await reader.readline()).decode().strip()
        await asyncio.sleep(delay)                       # giả lập độ trễ
        writer.write(f"OK {line.removeprefix('GET ')}\n".encode())
        await writer.drain()
    except (ConnectionError, asyncio.IncompleteReadError):
        pass
    finally:
        writer.close()
        state["active"] -= 1
        if state["active"] == 0:
            asyncio.create_task(report_when_idle())


async def main():
    host, port = sys.argv[1], int(sys.argv[2])
    delay_ms = int(sys.argv[3]) if len(sys.argv) > 3 else 200
    server = await asyncio.start_server(lambda r, w: handle(r, w, delay_ms / 1000), host, port,
                                        backlog=128)
    log(f"lắng nghe {host}:{port}, mỗi yêu cầu trễ {delay_ms} ms")
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        log(f"dừng; đã phục vụ {state['total']} yêu cầu")
