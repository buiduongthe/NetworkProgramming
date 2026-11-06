"""Bài mở rộng MR8.4 – Client thử nghiệm (đóng vai host MCP) cho bài chống SSRF.

Chỉ dùng thư viện chuẩn. Client tự dựng BA server HTTP cục bộ trên 127.0.0.1:
  - META  : "máy chủ siêu dữ liệu nội bộ" giả, trả một BÍ MẬT (mô phỏng 169.254.169.254);
  - PUB   : "trang công khai hợp lệ" giả (mô phỏng địa chỉ công khai 203.0.113.10);
  - REDIR : bộ chuyển hướng 302 trỏ sang META (mô phỏng SSRF qua chuyển hướng).
Sau đó khởi chạy mcp_server_ssrf.py như tiến trình con, truyền hai bảng giả lập (phân giải tên,
"dial") qua biến môi trường để mọi thứ chạy OFFLINE và tất định, rồi gửi các yêu cầu tools/call.

CHỈ CHẠY CỤC BỘ. Các địa chỉ 203.0.113.0/24 (TEST-NET-3) và 169.254.169.254 đều được ánh xạ về
server cục bộ qua bảng "dial"; không có gói tin nào rời khỏi máy.

Chạy:  python3 mcp_test_client.py <cổng_META> <cổng_PUB> <cổng_REDIR> [lệnh chạy server ...]
Mặc định server là: python3 mcp_server_ssrf.py (bản Go truyền: out/app).
"""
import json
import os
import queue
import subprocess
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


BI_MAT = "SECRET_TOKEN=ABCD-1234-NOI-BO-KHONG-DUOC-LO"
META_IP = "169.254.169.254"                              # IP siêu dữ liệu đám mây (ánh xạ về META)
PUB_IP = "203.0.113.10"                                  # "công khai" (TEST-NET-3, ánh xạ về PUB)
REDIR_IP = "203.0.113.20"                                # "công khai" có chuyển hướng (ánh xạ về REDIR)
META_PATH = "/latest/meta-data/token"

RPC_META = {"io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {}}


def make_handler(kind):
    class H(BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass                                         # không in nhật ký HTTP nội bộ

        def do_GET(self):
            if kind == "meta":
                body = BI_MAT.encode()
                self.send_response(200)
            elif kind == "pub":
                body = b"Trang cong khai hop le (203.0.113.10)"
                self.send_response(200)
            else:                                        # redir -> trỏ sang IP siêu dữ liệu nội bộ
                self.send_response(302)
                self.send_header("Location", f"http://{META_IP}{META_PATH}")
                self.end_headers()
                return
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
    return H


def req(i, method, params=None, meta=RPC_META):
    p = dict(params or {})
    if meta is not None:
        p["_meta"] = meta
    return {"jsonrpc": "2.0", "id": i, "method": method, "params": p}


def call(i, name, url):
    return req(i, "tools/call", {"name": name, "arguments": {"url": url}})


def dumps(obj):
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":"))


def main():
    if len(sys.argv) < 4:
        sys.exit("Cách dùng: python3 mcp_test_client.py <cổng_META> <cổng_PUB> <cổng_REDIR>")
    meta_port, pub_port, redir_port = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3])

    # Dựng ba server cục bộ.
    servers = []
    for kind, port in (("meta", meta_port), ("pub", pub_port), ("redir", redir_port)):
        srv = MayChuHTTP(("127.0.0.1", port), make_handler(kind))
        threading.Thread(target=srv.serve_forever, daemon=True).start()
        servers.append(srv)
    print(f"[client] Dựng META:127.0.0.1:{meta_port} (giữ bí mật), "
          f"PUB:127.0.0.1:{pub_port}, REDIR:127.0.0.1:{redir_port}", flush=True)

    # Hai bảng giả lập truyền cho MCP server qua biến môi trường (chỉ để demo offline).
    resolver = {"rebind-naive.evil.test": [PUB_IP, "127.0.0.1"],   # rebinding: công khai -> loopback
                "rebind-safe.evil.test": [PUB_IP, "127.0.0.1"]}
    dial = {META_IP: meta_port, PUB_IP: pub_port, REDIR_IP: redir_port, "127.0.0.1": meta_port}
    env = dict(os.environ, SSRF_RESOLVER=json.dumps(resolver), SSRF_DIAL=json.dumps(dial))
    env.pop("HTTP_PROXY", None); env.pop("HTTPS_PROXY", None)      # bản an toàn không dùng proxy

    cmd = sys.argv[4:] or [sys.executable, "mcp_server_ssrf.py"]
    print(f"[client] Khởi chạy MCP server: {' '.join(cmd)}", flush=True)
    proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, env=env)
    lines, errlog = queue.Queue(), []
    threading.Thread(target=lambda: [lines.put(x) for x in proc.stdout] and None, daemon=True).start()
    threading.Thread(target=lambda: errlog.extend(proc.stderr), daemon=True).start()

    scenario = [
        ("công bố phiên bản", req(1, "server/discover")),
        ("liệt kê công cụ", req(2, "tools/list")),
        ("[HỢP LỆ] an toàn tải trang công khai", call(3, "fetch_url_safe", f"http://{PUB_IP}/")),
        ("[HỢP LỆ] ngây thơ tải trang công khai", call(4, "fetch_url_naive", f"http://{PUB_IP}/")),
        ("[TẤN CÔNG] IP siêu dữ liệu – AN TOÀN", call(5, "fetch_url_safe", f"http://{META_IP}{META_PATH}")),
        ("[TẤN CÔNG] IP siêu dữ liệu – NGÂY THƠ", call(6, "fetch_url_naive", f"http://{META_IP}{META_PATH}")),
        ("[TẤN CÔNG] DNS rebinding – AN TOÀN", call(7, "fetch_url_safe", "http://rebind-safe.evil.test/secret")),
        ("[TẤN CÔNG] DNS rebinding – NGÂY THƠ", call(8, "fetch_url_naive", "http://rebind-naive.evil.test/secret")),
        ("[TẤN CÔNG] chuyển hướng tới nội bộ – AN TOÀN", call(9, "fetch_url_safe", f"http://{REDIR_IP}/go")),
        ("[TẤN CÔNG] chuyển hướng tới nội bộ – NGÂY THƠ", call(10, "fetch_url_naive", f"http://{REDIR_IP}/go")),
        ("[TẤN CÔNG] giao thức file:// – AN TOÀN", call(11, "fetch_url_safe", "file:///etc/passwd")),
        ("[HỢP LỆ] gỡ thông tin đăng nhập trong URL – AN TOÀN",
         call(12, "fetch_url_safe", f"http://admin:secret@{PUB_IP}/")),
    ]
    for label, msg in scenario:
        proc.stdin.write((dumps(msg) + "\n").encode("utf-8")); proc.stdin.flush()
        try:
            reply = lines.get(timeout=15).decode("utf-8").rstrip("\n")
        except queue.Empty:
            print(f"[client] ({label}) Không nhận được phản hồi trong 15 s", flush=True)
            break
        obj = json.loads(reply)
        print(f"[client] ({label})", flush=True)
        res = obj.get("result")
        if isinstance(res, dict) and "content" in res:
            nhan = "LỖI/TỪ CHỐI" if res.get("isError") else "OK"
            txt = res["content"][0]["text"]
            canh_bao = "  <-- !!! LỘ BÍ MẬT NỘI BỘ !!!" if "SECRET_TOKEN" in txt else ""
            print(f"[client]    [{nhan}] {txt}{canh_bao}", flush=True)
        else:
            print(f"[client]    <- {reply}", flush=True)
    proc.stdin.close()
    code = proc.wait(timeout=15)
    for srv in servers:
        srv.shutdown()
    print(f"[client] Đã đóng stdin; MCP server kết thúc với mã {code}", flush=True)
    print("----- Nhật ký stderr của MCP server -----", flush=True)
    for x in errlog:
        print(x.decode("utf-8", errors="replace").rstrip("\n"), flush=True)


if __name__ == "__main__":
    main()
