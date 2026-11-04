"""Bài 8.1 – Nhận câu trả lời của LLM theo luồng SSE.

Cài đặt Mã giả 8.1 – STREAM_COMPLETION (Mục 8.2.2, Giáo trình Lập trình mạng),
kế thừa Mã giả 7.5 – SSE_READ (Mục 7.3.3) và lớp lỗi NetError của Mã giả 2.2.
Tệp này được llm_client.py dùng; không chạy riêng.
"""
import http.client
import json


class NetError(Exception):
    """NetError của Mã giả 2.2: mang mã lỗi, cờ có thể thử lại và Retry-After (ms)."""

    def __init__(self, message, retry=False, retry_after=None):
        super().__init__(message)
        self.retry = retry
        self.retry_after = retry_after


def read_line(r, max_len):
    """READ_LINE (Mã giả 3.5) trên thân phản hồi; urllib đã giải mã khối (chunked).
    Trả về None khi thân kết thúc hoặc kết nối bị ngắt (SSE_READ chỉ dừng ở hai trường hợp này)."""
    try:
        raw = r.readline(max_len + 1)
    except (OSError, http.client.HTTPException):   # ngắt kết nối, khối bị cắt dở, hết thời gian chờ
        return None
    if raw == b"":
        return None
    if len(raw) > max_len and not raw.endswith(b"\n"):
        raise NetError("Dòng SSE quá dài")              # ProtocolError("Dòng quá dài")
    return raw.rstrip(b"\n").rstrip(b"\r").decode("utf-8")


def sse_read(r, on_event):
    """Mã giả 7.5 – SSE_READ(r, on_event)."""
    ev = {"type": "message", "data": []}
    while (line := read_line(r, 65536)) is not None:
        if line == "":                                   # dòng trống: hết một sự kiện
            if ev["data"]:
                on_event(ev["type"], "\n".join(ev["data"]))
            ev = {"type": "message", "data": []}
        elif not line.startswith(":"):                   # ":": dòng chú thích
            field, _, value = line.partition(":")
            if value.startswith(" "):
                value = value[1:]                        # STRIP_ONE_SPACE
            if field == "event":
                ev["type"] = value
            elif field == "data":
                ev["data"].append(value)
            # "id", "retry": không dùng trong bài này
    # NULL: server đóng luồng


def stream_completion(r, on_token):
    """Mã giả 8.1 – STREAM_COMPLETION(r, on_token); r: thân phản hồi sau tiêu đề."""
    st = {"text": "", "usage": None, "done": False}             # dòng 1

    def on_event(_type, data):                                   # dòng 2
        if data == "[DONE]":                                     # dòng 3: không phải JSON
            st["done"] = True
            return
        chunk = json.loads(data)                                 # dòng 4
        if chunk.get("usage") is not None:                       # dòng 5–6
            st["usage"] = chunk["usage"]
        if len(chunk.get("choices") or []) == 0:                 # dòng 7: sự kiện chỉ mang usage
            return
        d = (chunk["choices"][0].get("delta") or {}).get("content")   # dòng 8
        if d is not None:                                        # dòng 9–12
            st["text"] += d
            on_token(d)

    sse_read(r, on_event)                                        # dòng 14
    if not st["done"]:                                           # dòng 15
        raise NetError("Luồng bị ngắt giữa chừng")
    return st                                                    # dòng 16
