"""Bài 5.6 – Yêu cầu có hạn chót và thử lại có lùi theo cấp số nhân + nhiễu ngẫu nhiên.

Cài đặt Mã giả 5.7 REQUEST_WITH_DEADLINE (Mục 5.4.2) và Mã giả 5.8 RETRY_ASYNC (Mục 5.4.3).
Kế thừa: Mã giả 5.1 (khung JSON bất đồng bộ), 2.2 (SAFE_CONNECT, NetError có trường retry),
4.4 (UDP_REQUEST – vòng gửi lại có lùi thời gian).
Ánh xạ asyncio: CANCEL_AFTER ↔ asyncio.timeout(); hủy của người dùng (user_ct) ↔ task.cancel();
LINK(...) có sẵn: asyncio.timeout đổi việc hủy do hết hạn thành TimeoutError, còn hủy từ bên
ngoài giữ nguyên CancelledError – nhờ đó phân biệt được hai nguyên nhân.

Chạy:  python3 deadline_retry.py [host] [port] [cổng_không_có_server]
       (mặc định 127.0.0.1 9100 9101)
"""
import asyncio
import random
import sys
import time

from khung_json_async import (NetError, json_bytes, parse_endpoint, recv_json_async,
                              safe_connect_async, send_json_async)


class Timeout(Exception):
    """Hết hạn chót của toàn bộ yêu cầu."""


async def request_with_deadline(reader, writer, req, timeout_ms):   # Mã giả 5.7
    try:
        async with asyncio.timeout(timeout_ms / 1000):    # LINK(user_ct, CANCEL_AFTER(timeout_ms))
            await send_json_async(writer, req)
            return await recv_json_async(reader)
    except asyncio.CancelledError:                         # user_ct: người dùng bấm Hủy
        writer.close()                                     # trạng thái khung không xác định
        raise
    except TimeoutError:                                   # hết hạn chót
        writer.close()
        raise Timeout(f"Quá hạn {timeout_ms} ms") from None


async def retry_async(op, max_tries, base_ms, cap_ms, ct=None):   # Mã giả 5.8
    """ct: tín hiệu hủy của nơi gọi, truyền tiếp cho op(ct). Trong asyncio, hủy là task.cancel(): CancelledError
    không phải Timeout hay NetError nên không bị bắt (dòng 5) và asyncio.sleep cũng dừng ngay (dòng 12)."""
    if max_tries < 1:
        raise ValueError("max_tries phải ≥ 1")
    for k in range(max_tries):
        try:
            return await op(ct)                            # dòng 4: mỗi lần gọi là một lần thử mới
        except (Timeout, NetError) as e:                   # dòng 5: Cancelled không bắt
            ok = isinstance(e, Timeout) or e.retry         # dòng 6: Timeout luôn đáng thử lại
            if not ok or k == max_tries - 1:               # dòng 7
                raise
            delay = random.uniform(0, min(cap_ms, base_ms * 2 ** k))   # dòng 8: jitter
            retry_after = getattr(e, "retry_after", None)
            if retry_after is not None:                    # dòng 9–11: server đề nghị
                delay = max(delay, retry_after)
            code = "TIMEOUT" if isinstance(e, Timeout) else e.code
            log(f"  lỗi {code} (có thể thử lại); chờ {delay:.0f} ms, "
                f"giới hạn trên {min(cap_ms, base_ms * 2 ** k)} ms")
            await asyncio.sleep(delay / 1000)              # dòng 12: DELAY_ASYNC(delay, ct), hủy được khi chờ


T0 = time.monotonic()


def log(text):
    print(f"[client] [t={(time.monotonic() - T0) * 1000:5.0f} ms] {text}", flush=True)


def make_op(host, port, req, deadline_ms):
    """Tạo hàm op(ct): mỗi lần gọi là một lần thử mới (kết nối mới, yêu cầu mới)."""
    tries = 0

    async def op(ct):
        nonlocal tries
        tries += 1
        log(f"Lần thử {tries}: gửi {json_bytes(req).decode()}")
        reader, writer = await safe_connect_async(host, port, 3000)
        resp = await request_with_deadline(reader, writer, req, deadline_ms)   # có thể ném Timeout
        if resp.get("status") == 503:
            raise NetError("UNAVAILABLE", retry=True, retry_after=resp.get("retry_after_ms"))
        writer.close()
        return resp
    return op


async def scenario(title, coro_factory, cancel_ms=0):
    global T0
    T0 = time.monotonic()
    print(f"[client] === {title} ===", flush=True)
    task = asyncio.create_task(coro_factory())
    if cancel_ms:
        asyncio.get_running_loop().call_later(cancel_ms / 1000, task.cancel)   # nút Hủy
    try:
        log(f"Thành công: {json_bytes(await task).decode()}")
    except asyncio.CancelledError:
        log("Người dùng đã hủy (Cancelled) – không thử lại")
    except NetError as e:
        log(f"Thất bại, không thử lại: {e.code} (retry={e.retry})")
    except Timeout as e:
        log(f"Thất bại sau lần thử cuối: {e}")


async def main():
    host, port = parse_endpoint(sys.argv[1:3], "127.0.0.1", 9100)
    bad = int(sys.argv[3]) if len(sys.argv) > 3 else 9101
    await scenario("Kịch bản 1: hạn chót 1000 ms, thử lại tối đa 5 lần (base 200, cap 5000)",
                   lambda: retry_async(make_op(host, port, {"op": "lookup", "term": "TCP"}, 1000),
                                       5, 200, 5000, None))
    await scenario("Kịch bản 2: server xử lý 3000 ms, hạn chót 5000 ms, người dùng bấm Hủy sau 500 ms",
                   lambda: retry_async(make_op(host, port, {"op": "lookup", "term": "UDP",
                                                            "delay_ms": 3000}, 5000), 5, 200, 5000, None),
                   cancel_ms=500)
    await scenario(f"Kịch bản 3: cổng {bad} không có server (REFUSED)",
                   lambda: retry_async(make_op(host, bad, {"op": "lookup", "term": "TCP"}, 1000),
                                       5, 200, 5000, None))


if __name__ == "__main__":
    asyncio.run(main())
