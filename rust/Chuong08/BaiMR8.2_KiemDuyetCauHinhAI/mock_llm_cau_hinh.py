"""Server giả lập API LLM sinh cấu hình mạng dạng JSON (OpenAI Chat Completions, không truyền luồng).

Dùng cho Bài mở rộng MR8.2 ở cả hai ngôn ngữ để chạy thử mà KHÔNG gọi LLM thật.
"Mô hình" trả lời theo kịch bản cố định, dựa vào số lần đã trả lời trong hội thoại:
  - Lần 1: JSON bọc trong khối ```json kèm lời dẫn, có nhiều lỗi (VLAN 5000, địa chỉ cổng
    sai cú pháp, mạng con chồng lấn, ACL "permit ip any any", bật telnet).
  - Lần 2: đã sửa các lỗi trên nhưng sinh lỗi mới (trùng VLAN 99 đang dùng, cổng mặc định
    là địa chỉ mạng).
  - Lần 3 trở đi: cấu hình hợp lệ.
Mô phỏng đúng đặc điểm của LLM thật: đầu ra "trông hợp lý" nhưng không bảo đảm đúng.

Chạy:  python3 mock_llm_cau_hinh.py [host] [port]      (mặc định 127.0.0.1 9000)
"""
import json
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


LAN_1 = """Dưới đây là cấu hình đề xuất:
```json
{
  "vlan": [
    {"id": 30, "ten": "Phong_Lab", "mang": "10.20.0.0/24", "cong_mac_dinh": "10.20.0.1"},
    {"id": 5000, "ten": "WiFi_Khach", "mang": "10.20.1.0/25", "cong_mac_dinh": "10.20.1.300"},
    {"id": 50, "ten": "May_Chu", "mang": "10.20.1.64/26", "cong_mac_dinh": "10.20.1.65"}
  ],
  "acl": [{"ten": "KHACH_RA_NGOAI", "quy_tac": ["permit ip any any"]}],
  "dich_vu": {"telnet": true, "ssh": true}
}
```
Bạn có thể áp dụng ngay."""

LAN_2 = {
    "vlan": [
        {"id": 30, "ten": "Phong_Lab", "mang": "10.20.0.0/24", "cong_mac_dinh": "10.20.0.1"},
        {"id": 40, "ten": "WiFi_Khach", "mang": "10.20.1.0/25", "cong_mac_dinh": "10.20.1.1"},
        {"id": 99, "ten": "May_Chu", "mang": "10.20.1.128/27", "cong_mac_dinh": "10.20.1.128"},
    ],
    "acl": [{"ten": "KHACH_RA_NGOAI", "quy_tac": ["deny ip 10.20.1.0 0.0.0.127 10.20.0.0 0.0.3.255",
                                                  "permit ip 10.20.1.0 0.0.0.127 any"]}],
    "dich_vu": {"telnet": False, "ssh": True},
}

LAN_3 = json.loads(json.dumps(LAN_2))
LAN_3["vlan"][2].update({"id": 50, "cong_mac_dinh": "10.20.1.129"})

counter = 0
lock = threading.Lock()


def log(text):
    print(f"[mock] {text}", flush=True)


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
            turn = sum(1 for m in req["messages"] if m["role"] == "assistant") + 1
        except (ValueError, KeyError) as e:
            return self.send_json(400, {"error": {"message": f"Yêu cầu không hợp lệ: {e}"}})
        if turn == 1:
            content, note = LAN_1, "JSON trong khối markdown, nhiều lỗi"
        elif turn == 2:
            content, note = json.dumps(LAN_2, ensure_ascii=False, indent=2), "sửa lỗi cũ, sinh lỗi mới"
        else:
            content, note = json.dumps(LAN_3, ensure_ascii=False, indent=2), "hợp lệ"
        log(f"#{k} lần sinh {turn}: {note}")
        self.send_json(200, {"id": f"mock-{k}", "object": "chat.completion", "model": req.get("model"),
                             "choices": [{"index": 0, "finish_reason": "stop",
                                          "message": {"role": "assistant", "content": content}}]})


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 9000
    srv = MayChuHTTP((host, port), Handler)
    log(f"LLM giả lập (sinh cấu hình) tại http://{host}:{port}/v1")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        log("Dừng server")
    finally:
        srv.server_close()


if __name__ == "__main__":
    main()
