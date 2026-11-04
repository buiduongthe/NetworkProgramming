"""Bài 8.1 – Gọi API LLM có truyền luồng SSE, giới hạn tần suất, thử lại và hạn chót.

Cài đặt Mã giả 8.2 – CALL_LLM (Mục 8.2.3, Giáo trình Lập trình mạng), kế thừa
Mã giả 8.1 – STREAM_COMPLETION (llm_stream.py), Mã giả 5.8 – RETRY_ASYNC và
Mã giả 8.3 – ACQUIRE_TOKENS (thung_token.py, bản đồng bộ chép gọn).
Cấu hình qua biến môi trường: LLM_BASE_URL (mặc định http://127.0.0.1:9000/v1 – server giả lập),
LLM_API_KEY (bắt buộc), LLM_MODEL (mặc định mock-model), LLM_PRICE_IN, LLM_PRICE_OUT
(đơn giá USD cho 1 triệu token, tùy chọn).

Chạy:  python3 mock_llm_server.py 127.0.0.1 9000 --fail-first 1      (cửa sổ 1)
       LLM_API_KEY=khoa-thu python3 llm_client.py "So sánh TCP và UDP."   (cửa sổ 2)
"""
import email.utils
import json
import os
import random
import socket
import sys
import threading
import time
import urllib.error
import urllib.request

from llm_stream import NetError, stream_completion
from thung_token import Bucket, acquire_tokens

BASE_URL = os.environ.get("LLM_BASE_URL", "http://127.0.0.1:9000/v1").rstrip("/")
MODEL = os.environ.get("LLM_MODEL", "mock-model")


bucket = Bucket(cap=20000, rate=2000)        # hạn mức tự đặt: 20 000 token, nạp 2 000 token/giây


def secret(name):
    """SECRET(name): bí mật chỉ lấy từ biến môi trường, không ghi trong mã nguồn, không in ra."""
    value = os.environ.get(name)
    if not value:
        raise KeyError(f"Thiếu biến môi trường {name}")
    return value


def estimate_tokens(req):
    """ESTIMATE_TOKENS: ước lượng thô ~4 ký tự/token cho đầu vào, cộng max_tokens cho đầu ra."""
    return sum(len(m["content"]) for m in req["messages"]) // 4 + req["max_tokens"]


def parse_retry_after(h):
    """PARSE_RETRY_AFTER: Retry-After là số giây hoặc một ngày giờ HTTP (RFC 9110); trả về ms."""
    v = h.get("Retry-After") if h is not None else None
    if not v:
        return None
    if v.strip().isdigit():
        return int(v) * 1000
    try:
        return max(0, int((email.utils.parsedate_to_datetime(v).timestamp() - time.time()) * 1000))
    except (TypeError, ValueError):
        return None


class Cancelled(Exception):
    """Người dùng đã hủy (tín hiệu ct được kích hoạt)."""


def check_cancel(ct):
    """ct là threading.Event (hoặc None): đã đặt nghĩa là người dùng bấm Hủy."""
    if ct is not None and ct.is_set():
        raise Cancelled("Đã hủy")


def https_post(url, req, hdrs, timeout_ms, ct=None):
    """HTTPS_POST: urllib tự bọc TLS khi url là https://; trả về (status, headers, thân).
    urllib không hủy được giữa chừng nên ct chỉ được kiểm tra trước khi gửi yêu cầu."""
    check_cancel(ct)
    data = json.dumps(req, ensure_ascii=False).encode("utf-8")
    rq = urllib.request.Request(url, data=data, method="POST",
                                headers=dict(hdrs, **{"Content-Type": "application/json",
                                                      "Accept": "text/event-stream"}))
    try:
        resp = urllib.request.urlopen(rq, timeout=timeout_ms / 1000)   # chờ kết nối và mỗi lần đọc
        return resp.status, resp.headers, resp
    except urllib.error.HTTPError as e:                               # 4xx, 5xx vẫn là phản hồi
        return e.code, e.headers, e
    except urllib.error.URLError as e:
        if isinstance(e.reason, ConnectionRefusedError):
            raise NetError("REFUSED", retry=False)
        raise NetError(f"UNREACHABLE ({e.reason})", retry=True)
    except (socket.timeout, TimeoutError):
        raise NetError("Hết thời gian chờ phản hồi", retry=True)


def retry_call(op, max_tries, base_ms, cap_ms, ct=None):
    """Mã giả 5.8 – RETRY_ASYNC(op, max_tries, base_ms, cap_ms, ct), bản đồng bộ."""
    for k in range(max_tries):
        try:
            return op(ct)                                               # dòng 4
        except (TimeoutError, NetError) as e:                           # dòng 5: Cancelled không bắt
            ok = isinstance(e, TimeoutError) or e.retry                 # dòng 6: Timeout luôn đáng thử lại
            if not ok or k == max_tries - 1:
                raise
            delay = random.uniform(0, min(cap_ms, base_ms * 2 ** k))   # jitter
            retry_after = getattr(e, "retry_after", None)
            if retry_after is not None:
                delay = max(delay, retry_after)                         # server đề nghị
            print(f"[llm] Lần thử {k + 1}/{max_tries}: {e} – chờ {delay:.0f} ms rồi thử lại", flush=True)
            if ct is not None and ct.wait(delay / 1000):                # DELAY(delay, ct): hủy được khi chờ
                raise Cancelled("Đã hủy")
            if ct is None:
                time.sleep(delay / 1000)


def call_llm(req, on_token, ct=None):
    """Mã giả 8.2 – CALL_LLM(req, on_token, ct); ct: threading.Event của nút Hủy (hoặc None)."""
    started = False

    def one_attempt(ct):                                                # dòng 3
        nonlocal started
        acquire_tokens(bucket, estimate_tokens(req))                    # 8.2.4
        hdrs = {"Authorization": "Bearer " + secret("LLM_API_KEY")}
        status, h, r = https_post(BASE_URL + "/chat/completions", req, hdrs, 30000, ct)   # dòng 6–7
        if status in {408, 429, 500, 502, 503, 504}:
            r.close()
            raise NetError(f"HTTP {status}", retry=not started, retry_after=parse_retry_after(h))
        if status >= 400:
            r.close()
            raise NetError(f"HTTP {status}", retry=False)

        def tok(d):
            nonlocal started
            started = True
            on_token(d)
        try:
            return stream_completion(r, tok)
        finally:
            r.close()

    return retry_call(one_attempt, 5, 500, 20000, ct)                   # dòng 16


def main():
    question = " ".join(sys.argv[1:]) or "So sánh TCP và UDP."
    req = {"model": MODEL, "stream": True, "max_tokens": 512,
           "messages": [{"role": "system", "content": "Trợ lý mạng, trả lời ngắn."},
                        {"role": "user", "content": question}]}
    print(f"[llm] Mô hình {MODEL} tại {BASE_URL}", flush=True)
    print(f"[llm] Câu hỏi: {question}", flush=True)
    try:
        secret("LLM_API_KEY")
    except KeyError as e:
        print(f"[llm] {e.args[0]}", flush=True)
        sys.exit(2)
    t0, first = time.monotonic(), None

    def on_token(d):                                     # UI: UI_INVOKE(APPEND, d)
        nonlocal first
        if first is None:
            first = time.monotonic()
            print("[llm] Trả lời: ", end="")
        print(d, end="", flush=True)

    try:
        st = call_llm(req, on_token, threading.Event())       # ct: tín hiệu hủy (chương trình dòng lệnh không hủy)
    except NetError as e:
        print("" if first is None else "\n", end="")
        note = " – không thử lại vì đã hiển thị một phần câu trả lời" if first is not None else ""
        print(f"[llm] Lỗi: {e}{note}", flush=True)
        sys.exit(1)
    total = time.monotonic()
    print()
    print(f"[llm] TTFT: {(first - t0) * 1000:.0f} ms, tổng thời gian: {(total - t0) * 1000:.0f} ms")
    u = st["usage"] or {}
    n_in, n_out = u.get("prompt_tokens", 0), u.get("completion_tokens", 0)
    print(f"[llm] usage: prompt_tokens = {n_in}, completion_tokens = {n_out}")
    p_in, p_out = os.environ.get("LLM_PRICE_IN"), os.environ.get("LLM_PRICE_OUT")
    if p_in and p_out:                                   # Công thức (8.1): N_in·p_in + N_out·p_out
        cost = (n_in * float(p_in) + n_out * float(p_out)) / 1_000_000
        print(f"[llm] Chi phí ước tính: {n_in} × {p_in} + {n_out} × {p_out} (USD/1 triệu token)"
              f" = {cost:.8f} USD")


if __name__ == "__main__":
    main()
