"""Server giả lập API LLM tương thích OpenAI Chat Completions (chỉ dùng thư viện chuẩn).

Dùng cho Bài 8.1 (Mã giả 8.1, 8.2; Mục 8.2 Giáo trình Lập trình mạng) ở cả ba ngôn ngữ,
để chạy thử mà KHÔNG gọi dịch vụ LLM thật.
- POST /v1/chat/completions, "stream": true -> phản hồi text/event-stream (SSE), mã hóa khối (chunked):
  các sự kiện data: {"choices":[{"delta":{"content":...}}]}, một sự kiện chỉ mang "usage"
  (mảng choices rỗng), rồi data: [DONE].
- Thiếu tiêu đề Authorization: Bearer ... -> 401.
- --fail-first N   : N yêu cầu đầu tiên nhận 429 Too Many Requests kèm Retry-After.
- --cut-request K  : yêu cầu thứ K bị ngắt kết nối đột ngột giữa luồng (không có [DONE]).

Chạy:  python3 mock_llm_server.py [host] [port] [--fail-first N] [--retry-after S] [--cut-request K]
       (mặc định 127.0.0.1 9000; client đặt LLM_BASE_URL=http://127.0.0.1:9000/v1)
"""
import argparse
import json
import threading
import time
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class MayChuHTTP(ThreadingHTTPServer):
    """ThreadingHTTPServer không in traceback khi client đóng ngang (Windows: đóng socket còn dữ liệu chưa đọc -> RST
    -> ConnectionResetError/ConnectionAbortedError; mặc định http.server in cả traceback ra màn hình)."""

    def handle_error(self, request, client_address):
        if isinstance(sys.exc_info()[1], ConnectionError):
            return
        super().handle_error(request, client_address)


ANSWER = ("TCP là giao thức hướng kết nối, tin cậy và giữ đúng thứ tự; "
          "UDP không kết nối, nhẹ và nhanh hơn nhưng không bảo đảm dữ liệu tới nơi.")
PIECES = [w if i == 0 else " " + w for i, w in enumerate(ANSWER.split(" "))]
OPTS = None
counter = 0
counter_lock = threading.Lock()


def log(text):
    print(f"[mock] {text}", flush=True)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):              # tắt nhật ký mặc định của http.server
        pass

    def send_json(self, status, obj, extra=None):
        body = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        for k, v in (extra or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)

    def write_chunk(self, text):                    # một khối của Transfer-Encoding: chunked
        data = text.encode("utf-8")
        self.wfile.write(f"{len(data):X}\r\n".encode("ascii") + data + b"\r\n")
        self.wfile.flush()

    def event(self, obj):                           # một sự kiện SSE: dòng data + dòng trống
        self.write_chunk("data: " + (obj if isinstance(obj, str) else json.dumps(obj, ensure_ascii=False)) + "\n\n")

    def do_POST(self):
        global counter
        with counter_lock:
            counter += 1
            k = counter
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        if self.path != "/v1/chat/completions":
            log(f"#{k} POST {self.path} -> 404")
            return self.send_json(404, {"error": {"message": "Không có đường dẫn này"}})
        auth = self.headers.get("Authorization", "")
        if not auth.startswith("Bearer ") or len(auth) <= len("Bearer "):
            log(f"#{k} POST {self.path} -> 401 (thiếu khóa API)")
            return self.send_json(401, {"error": {"message": "Thiếu hoặc sai khóa API"}})
        if k <= OPTS.fail_first:
            log(f"#{k} POST {self.path} -> 429 (Retry-After: {OPTS.retry_after})")
            return self.send_json(429, {"error": {"message": "Vượt giới hạn tần suất"}},
                                  {"Retry-After": str(OPTS.retry_after)})
        try:
            req = json.loads(body.decode("utf-8"))
        except ValueError:
            log(f"#{k} POST {self.path} -> 400 (JSON hỏng)")
            return self.send_json(400, {"error": {"message": "JSON không hợp lệ"}})
        model = req.get("model", "mock-model")
        prompt_tokens = sum(len(m.get("content", "")) for m in req.get("messages", [])) // 4
        pieces = PIECES[: max(1, int(req.get("max_tokens", 512)))]
        usage = {"prompt_tokens": prompt_tokens, "completion_tokens": len(pieces),
                 "total_tokens": prompt_tokens + len(pieces)}
        if not req.get("stream"):
            log(f"#{k} POST {self.path} model={model} stream=false -> 200")
            return self.send_json(200, {"choices": [{"index": 0, "finish_reason": "stop",
                                        "message": {"role": "assistant", "content": "".join(pieces)}}],
                                        "usage": usage})
        cut = (k == OPTS.cut_request)
        log(f"#{k} POST {self.path} model={model} stream=true -> 200"
            + (" (sẽ ngắt giữa chừng)" if cut else ""))
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Transfer-Encoding", "chunked")
        self.send_header("Connection", "close")
        self.end_headers()
        base = {"id": f"mock-{k}", "object": "chat.completion.chunk", "model": model}
        self.write_chunk(": mock keep-alive\n\n")                       # dòng chú thích SSE
        self.event(dict(base, choices=[{"index": 0, "delta": {"role": "assistant"}}]))  # không có content
        for i, piece in enumerate(pieces):
            time.sleep(0.12)                                           # mô phỏng thời gian sinh token
            if cut and i == 4:
                log(f"#{k} ngắt kết nối sau {i} phần")
                self.connection.shutdown(2)                            # đóng đột ngột: không khối kết thúc
                return
            self.event(dict(base, choices=[{"index": 0, "delta": {"content": piece}, "finish_reason": None}]))
        self.event(dict(base, choices=[{"index": 0, "delta": {}, "finish_reason": "stop"}]))
        self.event(dict(base, choices=[], usage=usage))                # sự kiện chỉ mang usage
        self.event("[DONE]")
        self.wfile.write(b"0\r\n\r\n")                                 # khối độ dài 0: hết thân
        log(f"#{k} đã gửi {len(pieces)} phần, usage={usage['prompt_tokens']}/{usage['completion_tokens']}")


def main():
    global OPTS
    ap = argparse.ArgumentParser(description="Server giả lập API LLM (SSE)")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("port", nargs="?", type=int, default=9000)
    ap.add_argument("--fail-first", type=int, default=0, help="số yêu cầu đầu nhận 429")
    ap.add_argument("--retry-after", type=int, default=1, help="giá trị Retry-After (giây)")
    ap.add_argument("--cut-request", type=int, default=0, help="thứ tự yêu cầu bị ngắt giữa luồng")
    OPTS = ap.parse_args()
    srv = MayChuHTTP((OPTS.host, OPTS.port), Handler)
    log(f"Server giả lập LLM tại http://{OPTS.host}:{OPTS.port}/v1 "
        f"(fail-first={OPTS.fail_first}, cut-request={OPTS.cut_request})")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        log("Dừng server")
    finally:
        srv.server_close()


if __name__ == "__main__":
    main()
