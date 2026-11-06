"""Bài mở rộng MR6.1 – Tắt server an toàn (graceful shutdown) với asyncio.

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.2 (server bất đồng bộ) và Bài 5.6
(hạn chót, hủy). Giao thức dòng văn bản: client gửi "LAM <ms>", server mô phỏng công việc
dài <ms> mili giây rồi trả "XONG <ms>".
Khi nhận SIGTERM (lệnh kill, trình quản lý dịch vụ, Docker/Kubernetes) hoặc SIGINT (Ctrl+C):
  1. Đóng socket nghe: ngừng nhận kết nối mới (client mới bị từ chối).
  2. Kết nối đang rỗi: gửi "BYE" rồi đóng ngay.
  3. Kết nối đang làm dở: cho làm xong yêu cầu hiện tại, trả kết quả, gửi "BYE", đóng.
  4. Quá hạn chờ (--han-cho-ms): hủy các yêu cầu còn dở, báo "HUY" cho client.
PID được ghi vào server.pid để có thể gửi tín hiệu bằng: kill -TERM $(cat server.pid)

Chạy:  python3 server_tat_an_toan.py [host] [port] [--han-cho-ms 2000]
"""
import argparse
import asyncio
import itertools
import os
import signal
import time

counter = itertools.count(1)
stopping = asyncio.Event()                  # được đặt khi nhận tín hiệu dừng
connections = {}                            # mã kết nối -> tác vụ đang phục vụ
stats = {"xong": 0, "huy": 0}


def log(text):
    print(f"[server] {text}", flush=True)


async def read_line_or_stop(reader):
    """Chờ dòng lệnh kế tiếp HOẶC tín hiệu dừng; trả về None nếu server đang tắt."""
    read = asyncio.ensure_future(reader.readline())
    stop = asyncio.ensure_future(stopping.wait())
    try:
        done, _ = await asyncio.wait({read, stop}, return_when=asyncio.FIRST_COMPLETED)
        return read.result() if read in done else None
    finally:
        for f in (read, stop):
            if not f.done():
                f.cancel()


async def handle(reader, writer):
    cid = next(counter)
    connections[cid] = asyncio.current_task()
    log(f"#{cid} kết nối từ cổng {writer.get_extra_info('peername')[1]}")
    job = None
    try:
        while True:
            line = await read_line_or_stop(reader)
            if line is None:                                    # rỗi đúng lúc server tắt
                writer.write("BYE server đang tắt\n".encode())
                log(f"#{cid} đang rỗi -> gửi BYE, đóng")
                break
            if not line:                                        # client đã đóng
                log(f"#{cid} client đóng kết nối")
                break
            parts = line.decode("utf-8").split()
            if len(parts) != 2 or parts[0] != "LAM" or not parts[1].isdigit():
                writer.write("LOI lệnh không hợp lệ\n".encode())
                continue
            job = int(parts[1])
            log(f"#{cid} bắt đầu LAM {job}")
            await asyncio.sleep(job / 1000)                     # công việc dài (có thể bị hủy)
            writer.write(f"XONG {job}\n".encode())
            stats["xong"] += 1
            if stopping.is_set():                               # làm xong việc dở thì mới chào
                writer.write("BYE server đang tắt\n".encode())
                log(f"#{cid} XONG {job} -> gửi BYE, đóng")
                break
            log(f"#{cid} XONG {job}")
            job = None
            await writer.drain()
    except asyncio.CancelledError:                              # quá hạn chờ tắt
        stats["huy"] += 1
        log(f"#{cid} quá hạn chờ -> hủy LAM {job}")
        writer.write("HUY yêu cầu bị hủy vì server tắt\n".encode())
    finally:
        try:
            await writer.drain()
        except (ConnectionError, asyncio.CancelledError):
            pass
        writer.close()
        connections.pop(cid, None)


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("port", nargs="?", type=int, default=9000)
    ap.add_argument("--han-cho-ms", type=int, default=2000, help="thời gian chờ tối đa khi tắt")
    a = ap.parse_args()
    server = await asyncio.start_server(handle, a.host, a.port)
    loop = asyncio.get_running_loop()
    received = []
    for sig in (signal.SIGTERM, signal.SIGINT):
        try:
            loop.add_signal_handler(sig, lambda s=sig: (received.append(s.name), stopping.set()))
        except NotImplementedError:      # Windows: không có add_signal_handler -> signal.signal (Ctrl+C là SIGINT)
            signal.signal(sig, lambda _n, _f, s=sig: loop.call_soon_threadsafe(
                lambda: (received.append(s.name), stopping.set())))
    with open("server.pid", "w") as f:
        f.write(str(os.getpid()))
    log(f"Lắng nghe tại {a.host}:{a.port}, PID ghi vào server.pid")

    await stopping.wait()
    t0 = time.monotonic()
    server.close()                                              # 1. ngừng nhận kết nối mới
    log(f"Nhận {received[0]}: ngừng nhận kết nối mới")
    log(f"Chờ tối đa {a.han_cho_ms} ms cho {len(connections)} kết nối")
    pending = list(connections.values())
    if pending:
        _, still = await asyncio.wait(pending, timeout=a.han_cho_ms / 1000)
        if still:                                               # 4. quá hạn: hủy phần còn lại
            log(f"Hết hạn chờ: hủy {len(still)} kết nối")
            for t in still:
                t.cancel()
            await asyncio.wait(still)
    os.remove("server.pid")
    log(f"Đã tắt sau {time.monotonic() - t0:.1f} s: xong {stats['xong']} yêu cầu, hủy {stats['huy']}")


if __name__ == "__main__":
    asyncio.run(main())
