"""Server giả lập API LLM có gọi công cụ (tool calling, dạng OpenAI Chat Completions).

Dùng cho Bài mở rộng MR8.1 ở cả hai ngôn ngữ (Python, Go) để chạy thử mà KHÔNG gọi LLM thật.
"Mô hình" là một kịch bản cố định, tất định:
  - Đọc câu hỏi cuối của người dùng, tìm tên máy/địa chỉ và các số cổng.
  - Chưa có kết quả công cụ: nếu đích là tên máy -> yêu cầu phan_giai_ten;
    rồi yêu cầu kiem_tra_cong cho từng cổng (nhiều lời gọi song song trong một phản hồi).
  - Câu hỏi có "khởi động lại": cố ý "bịa" thêm công cụ khoi_dong_lai_dich_vu KHÔNG có trong
    danh sách tools – mô phỏng ảo giác (hallucination) để thử lớp kiểm soát của tác tử.
  - Đã đủ kết quả: trả lời bằng văn bản dựa trên kết quả công cụ (finish_reason "stop").
Thiếu tiêu đề Authorization: Bearer ... -> 401.

Chạy:  python3 mock_llm_cong_cu.py [host] [port]      (mặc định 127.0.0.1 9000)
"""
import json
import re
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class MayChuHTTP(ThreadingHTTPServer):
    """ThreadingHTTPServer không in traceback khi client đóng ngang (Windows: đóng socket còn dữ liệu chưa đọc -> RST
    -> ConnectionResetError/ConnectionAbortedError; mặc định http.server in cả traceback ra màn hình)."""

    def handle_error(self, request, client_address):
        if isinstance(sys.exc_info()[1], ConnectionError):
            return
        super().handle_error(request, client_address)


counter = 0
lock = threading.Lock()


def log(text):
    print(f"[mock] {text}", flush=True)


def tool_call(n, name, args):
    return {"id": f"call_{n}", "type": "function",
            "function": {"name": name, "arguments": json.dumps(args, ensure_ascii=False, separators=(",", ":"))}}


def plan(messages):
    """Quyết định bước kế tiếp: trả về (danh_sách_tool_calls, None) hoặc (None, câu_trả_lời)."""
    question = next(m["content"] for m in reversed(messages) if m["role"] == "user")
    m = re.search(r"\b(localhost|\d{1,3}(?:\.\d{1,3}){3}|[a-z0-9-]+(?:\.[a-z0-9-]+)+)\b", question)
    host = m.group(1) if m else "localhost"
    ports = [int(p) for p in re.findall(r"\d+", " ".join(re.findall(r"cổng ([\d ,và]+)", question)))]
    results = {}                                                # tên công cụ -> danh sách kết quả
    calls = {}
    for msg in messages:
        if msg["role"] == "assistant" and msg.get("tool_calls"):
            for c in msg["tool_calls"]:
                calls[c["id"]] = c["function"]
        if msg["role"] == "tool":
            fn = calls.get(msg["tool_call_id"], {"name": "?", "arguments": "{}"})
            results.setdefault(fn["name"], []).append((json.loads(fn["arguments"]), json.loads(msg["content"])))
    is_name = not re.fullmatch(r"\d{1,3}(?:\.\d{1,3}){3}", host)
    if is_name and "phan_giai_ten" not in results:
        return [tool_call(1, "phan_giai_ten", {"ten": host})], None
    if "kiem_tra_cong" not in results and ports:
        tcs = [tool_call(10 + i, "kiem_tra_cong", {"host": host, "cong": p}) for i, p in enumerate(ports)]
        if "khởi động lại" in question:                          # ảo giác: công cụ không tồn tại
            tcs.append(tool_call(99, "khoi_dong_lai_dich_vu", {"host": host, "dich_vu": "dns"}))
        return tcs, None
    # Đủ thông tin: tổng hợp câu trả lời
    errors = [f"{name} bị từ chối ({res['loi']})" for name, items in results.items()
              for _, res in items if "loi" in res]
    parts = []
    for args, res in results.get("kiem_tra_cong", []):
        st = res.get("trang_thai")
        if st == "OPEN":
            parts.append(f"cổng {args['cong']} OPEN, dịch vụ đang lắng nghe")
        elif st == "CLOSED":
            parts.append(f"cổng {args['cong']} CLOSED, không có chương trình lắng nghe "
                         f"(hệ điều hành trả RST): hãy khởi động dịch vụ hoặc sửa cấu hình cổng")
        elif st:
            parts.append(f"cổng {args['cong']} {st}: kiểm tra tường lửa và đường đi")
    answer = ""
    if parts:
        answer = f"Kết quả chẩn đoán {host}: " + "; ".join(parts) + "."
    if errors:
        answer += (" " if answer else "") + "Tôi không thực hiện được: " + "; ".join(errors) + \
                  ". Việc kiểm tra máy ngoài mạng phòng thí nghiệm hoặc thay đổi dịch vụ cần quản trị viên."
    return None, answer or "Không có đủ thông tin để chẩn đoán."


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        pass

    def send_json(self, status, obj):
        body = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        global counter
        with lock:
            counter += 1
            k = counter
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        if self.path != "/v1/chat/completions":
            return self.send_json(404, {"error": {"message": "Không có đường dẫn này"}})
        if not self.headers.get("Authorization", "").startswith("Bearer "):
            log(f"#{k} -> 401 (thiếu khóa API)")
            return self.send_json(401, {"error": {"message": "Thiếu khóa API"}})
        try:
            req = json.loads(body)
            calls, answer = plan(req["messages"])
        except (ValueError, KeyError, StopIteration) as e:
            log(f"#{k} -> 400 ({e})")
            return self.send_json(400, {"error": {"message": f"Yêu cầu không hợp lệ: {e}"}})
        offered = [t["function"]["name"] for t in req.get("tools", [])]
        if calls:
            msg = {"role": "assistant", "content": None, "tool_calls": calls}
            reason = "tool_calls"
            log(f"#{k} {len(req['messages'])} tin nhắn, {len(offered)} công cụ -> gọi "
                + ", ".join(c["function"]["name"] for c in calls))
        else:
            msg = {"role": "assistant", "content": answer}
            reason = "stop"
            log(f"#{k} {len(req['messages'])} tin nhắn -> trả lời ({len(answer)} ký tự)")
        self.send_json(200, {"id": f"mock-{k}", "object": "chat.completion", "model": req.get("model"),
                             "choices": [{"index": 0, "message": msg, "finish_reason": reason}]})


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 9000
    srv = MayChuHTTP((host, port), Handler)
    log(f"LLM giả lập (gọi công cụ) tại http://{host}:{port}/v1")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        log("Dừng server")
    finally:
        srv.server_close()


if __name__ == "__main__":
    main()
