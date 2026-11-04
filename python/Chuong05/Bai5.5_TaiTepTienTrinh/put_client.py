"""Bài 5.5 – Tải tệp bất đồng bộ có báo cáo tiến trình và hủy (asyncio).

Cài đặt Mã giả 5.6 PUT_FILE_ASYNC (Mục 5.3.4, Giáo trình Lập trình mạng).
Kế thừa Mã giả 3.10 (PUT_CLIENT): giữ nguyên giao thức và trình tự 4 bước; thêm async/await,
hàm báo tiến trình on_progress (tối đa ~10 lần/giây) và hủy. Trong asyncio, tín hiệu hủy ct
tương ứng với task.cancel(): CancelledError phát sinh tại điểm await đang chờ.
Nếu tệp chưa tồn tại, chương trình tạo một tệp thử 4 MiB.

Chạy:  python3 put_client.py [host] [port] [tệp] [hủy_sau_ms]
       (mặc định 127.0.0.1 9100 du_lieu_thu.bin; không hủy)
"""
import asyncio
import hashlib
import os
import random
import socket
import sys
import time

from khung_json_async import (NetError, ProtocolError, json_bytes, parse_endpoint, recv_json_async,
                              safe_connect_async, send_json_async)


def sha256_of_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


async def put_file_async(reader, writer, path, on_progress):          # Mã giả 5.6
    size = os.path.getsize(path)
    digest = await asyncio.to_thread(sha256_of_file, path)            # tệp lớn: lâu, chạy ở luồng phụ
    hdr = {"op": "put", "name": os.path.basename(path), "size": size, "sha256": digest}
    await send_json_async(writer, hdr)
    if (await recv_json_async(reader)).get("status") != 100:
        raise ProtocolError("Server từ chối")
    f = open(path, "rb")
    sent, last = 0, 0.0
    try:
        while True:
            chunk = await asyncio.to_thread(f.read, 65536)            # READ_ASYNC
            if chunk == b"":
                break
            writer.write(chunk)                                       # SEND_ALL_ASYNC:
            await writer.drain()                                      # điểm có thể hủy
            sent += len(chunk)                                        # đã vào bộ đệm gửi, chưa chắc tới server
            if time.monotonic() - last >= 0.1:                        # tối đa ~10 lần/giây
                on_progress(sent, size)
                last = time.monotonic()
    finally:
        f.close()
    resp = await recv_json_async(reader)                              # dòng 24: server đã nhận đủ
    if resp.get("status") == 200:                                     # dòng 25: chỉ báo 100% khi server xác nhận
        on_progress(size, size)
    return resp


def make_test_file(path, mib):
    with open(path, "wb") as f:
        f.write(random.Random(5).randbytes(mib * 1048576))
    print(f"[client] Tạo tệp thử {path} ({mib} MiB)", flush=True)


def show_progress(sent, size):                                        # on_progress
    print(f"[client] Tiến trình: {sent / 1048576:.2f}/{size / 1048576:.2f} MiB "
          f"({sent * 100 // size}%)", flush=True)


async def main():
    host, port = parse_endpoint(sys.argv[1:3], "127.0.0.1", 9100)
    path = sys.argv[3] if len(sys.argv) > 3 else "du_lieu_thu.bin"
    cancel_ms = int(sys.argv[4]) if len(sys.argv) > 4 else 0
    if not os.path.exists(path):
        make_test_file(path, 4)
    try:
        reader, writer = await safe_connect_async(host, port, 3000)
    except NetError as e:
        sys.exit(f"[client] Không kết nối được: {e}")
    # Bộ đệm gửi nhỏ (64 KiB) để số byte "đã gửi" bám sát số byte server đã nhận (minh họa)
    writer.get_extra_info("socket").setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 65536)
    print(f"[client] Đã kết nối {host}:{port}, tải lên {path}"
          + (f", sẽ bấm Hủy sau {cancel_ms} ms" if cancel_ms else ""), flush=True)
    t0 = time.monotonic()
    task = asyncio.create_task(put_file_async(reader, writer, path, show_progress))
    if cancel_ms:
        asyncio.get_running_loop().call_later(cancel_ms / 1000, task.cancel)   # nút Hủy: CANCEL(cts)
    try:
        resp = await task
        print(f"[client] Kết quả: {json_bytes(resp).decode()} sau {time.monotonic() - t0:.2f} s", flush=True)
    except asyncio.CancelledError:
        print(f"[client] Đã hủy sau {time.monotonic() - t0:.2f} s; đóng kết nối "
              "(trạng thái giao thức không còn tin cậy)", flush=True)
    except (ProtocolError, OSError) as e:
        print(f"[client] Lỗi: {e}", flush=True)
    finally:
        writer.close()                                     # hủy giữa chừng: đóng kết nối (Mục 5.4.1)
        await writer.wait_closed()


if __name__ == "__main__":
    asyncio.run(main())
