"""Bài 8.1 – Mã giả 8.3 ACQUIRE_TOKENS (chép gọn từ Bài 8.2, bản đồng bộ: khóa luồng, time.sleep).

Thùng token để client tự giới hạn tần suất gọi LLM (Mục 8.2.4). Được llm_client.py dùng.
"""
import threading
import time


class Bucket:
    """Mã giả 8.3 – thùng token (bản đồng bộ: khóa luồng, time.sleep)."""

    def __init__(self, cap, rate):
        self.cap, self.rate, self.tokens = cap, rate, cap
        self.last, self.lock = time.monotonic(), threading.Lock()


def acquire_tokens(b, n):
    if n > b.cap:
        raise ValueError("n vượt dung lượng")                   # ArgumentError
    while True:
        with b.lock:
            now = time.monotonic()
            b.tokens = min(b.cap, b.tokens + (now - b.last) * b.rate)
            b.last = now
            if b.tokens >= n:
                b.tokens -= n
                return
            wait_ms = (n - b.tokens) / b.rate * 1000
        time.sleep(wait_ms / 1000)                               # không giữ khóa khi chờ
