"""Bài 5.1 – Client tra cứu bất đồng bộ dùng khung JSON (minh họa Mã giả 5.1).

Kế thừa Mã giả 3.8 (LOOKUP_CLIENT): đọc từng thuật ngữ từ bàn phím, gửi {op:"lookup"} và in
phản hồi; mọi lời gọi vào/ra được thay bằng phiên bản bất đồng bộ (Mục 5.2.4).
Chương trình in thêm 4 byte tiêu đề của mỗi khung và minh họa việc hủy một lời gọi nhận
đang chờ: sau khi hủy, trạng thái định khung không còn tin cậy nên kết nối bị đóng (Mục 5.4.1).

Chạy:  python3 frame_client.py [host] [port]      (mặc định 127.0.0.1 9100)
"""
import asyncio
import sys

from khung_json_async import (NetError, json_bytes, make_frame, parse_endpoint,
                              recv_json_async, safe_connect_async, send_json_async)


async def read_console():
    """READ_CONSOLE bất đồng bộ: đọc bàn phím trên luồng phụ để không chặn vòng lặp sự kiện."""
    line = await asyncio.to_thread(sys.stdin.readline)
    return line.rstrip("\n") if line else None


async def main():
    host, port = parse_endpoint(sys.argv[1:], "127.0.0.1", 9100)
    try:
        reader, writer = await safe_connect_async(host, port, 3000)
    except NetError as e:
        sys.exit(f"[client] Không kết nối được: {e}")
    print(f"[client] Đã kết nối {host}:{port}. Nhập thuật ngữ cần tra (Ctrl+D để kết thúc).", flush=True)
    try:
        while (term := await read_console()) is not None:
            req = {"op": "lookup", "term": term}
            frame = make_frame(req)
            print(f"[client] Gửi khung {len(frame)} byte, tiêu đề {frame[:4].hex(' ').upper()}: "
                  f"{json_bytes(req).decode()}", flush=True)
            await send_json_async(writer, req)
            resp = await asyncio.wait_for(recv_json_async(reader), 5)   # RECV_TIMEOUT 5000 ms
            if resp.get("status") == 200:
                print(f"[client] {resp['term']}: {resp['text']}", flush=True)
            else:
                print(f"[client] Lỗi {resp.get('status')}: {resp.get('error')}", flush=True)
        # Minh họa hủy: chờ một khung mà server không gửi, tín hiệu hủy kích hoạt sau 500 ms
        task = asyncio.create_task(recv_json_async(reader))
        asyncio.get_running_loop().call_later(0.5, task.cancel)          # CANCEL(ct) sau 500 ms
        try:
            await task
        except asyncio.CancelledError:
            print("[client] RECV_JSON_ASYNC bị hủy sau 500 ms (Cancelled); "
                  "trạng thái khung không xác định nên đóng kết nối", flush=True)
    finally:
        writer.close()                                     # CLOSE: không dùng tiếp kết nối đã hủy
        await writer.wait_closed()
        print("[client] Đã đóng kết nối", flush=True)


if __name__ == "__main__":
    asyncio.run(main())
