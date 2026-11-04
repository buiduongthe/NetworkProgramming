"""Bài 8.3 – Client thử nghiệm MCP qua kênh stdio (đóng vai host, chỉ dùng thư viện chuẩn).

Khởi chạy MCP server như tiến trình con, gửi một chuỗi thông điệp JSON-RPC (mỗi thông điệp một dòng)
và in phản hồi; nhật ký stderr của server được thu riêng và in ở cuối, cho thấy stdout chỉ chứa JSON.
Dùng chung cho bản C#, Java, Python của Bài 8.3 (Mục 8.3.5, 8.5.4 Giáo trình Lập trình mạng).
Client mở sẵn một cổng TCP trên 127.0.0.1 để công cụ check_port thấy trạng thái OPEN.

Chạy:  python3 mcp_test_client.py <cổng_mở> <cổng_đóng> <lệnh chạy server ...>
Ví dụ: ALLOWED_NETS=127.0.0.0/8 python3 mcp_test_client.py 9000 9001 python3 mcp_server.py
"""
import json
import queue
import socket
import subprocess
import sys
import threading

META = {"io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {}}


def req(i, method, params=None, meta=META):
    p = dict(params or {})
    if meta is not None:
        p["_meta"] = meta
    return {"jsonrpc": "2.0", "id": i, "method": method, "params": p}


def call(i, name, args):
    return req(i, "tools/call", {"name": name, "arguments": args})


def scenario(open_port, closed_port):
    """Chuỗi thử nghiệm: (mô tả, thông điệp hoặc dòng thô)."""
    return [
        ("công bố phiên bản, khả năng", req(1, "server/discover")),
        ("liệt kê công cụ", req(2, "tools/list")),
        ("cổng đang mở", call(3, "check_port", {"host": "127.0.0.1", "port": open_port})),
        ("cổng đóng", call(4, "check_port", {"host": "127.0.0.1", "port": closed_port})),
        ("đích ngoài ALLOWED_NETS", call(5, "check_port", {"host": "10.10.1.5", "port": 22})),
        ("phân giải tên", call(6, "resolve_host", {"host": "localhost"})),
        ("ping (socket RAW)", call(7, "ping", {"host": "127.0.0.1"})),
        ("chèn lệnh shell", call(8, "ping", {"host": "8.8.8.8; rm -rf ~"})),
        ("cổng sai", call(9, "check_port", {"host": "127.0.0.1", "port": 70000})),
        ("công cụ không tồn tại", call(10, "scan_network", {"cidr": "10.10.0.0/16"})),
        ("thiếu _meta", req(11, "tools/list", meta=None)),
        ("phiên bản cũ", req(12, "tools/list", meta=dict(META, **{
            "io.modelcontextprotocol/protocolVersion": "2025-11-25"}))),
        ("phương thức không có", req(13, "resources/list")),
        ("thông báo (không có id)", {"jsonrpc": "2.0", "method": "notifications/cancelled",
                                     "params": {"requestId": 3}}),
        ("dòng không phải JSON", '{"jsonrpc":"2.0","id":15,"method":'),
    ]


def dumps(obj):
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":"))


def short(msg):
    """Rút gọn yêu cầu khi in: bỏ _meta (đã in đầy đủ ở yêu cầu đầu tiên)."""
    if not isinstance(msg, dict):
        return msg
    params = msg.get("params", {})
    p = {k: v for k, v in params.items() if k != "_meta"}
    if "_meta" not in params:
        tail = " (không có _meta)"
    elif params["_meta"] != META:
        tail = " (_meta: protocolVersion " + params["_meta"]["io.modelcontextprotocol/protocolVersion"] + ")"
    else:
        tail = ""
    head = f"id={msg['id']} " if "id" in msg else ""
    return f"{head}{msg['method']} {dumps(p)}{tail}"


def main():
    if len(sys.argv) < 4:
        sys.exit("Cách dùng: python3 mcp_test_client.py <cổng_mở> <cổng_đóng> <lệnh chạy server ...>")
    open_port, closed_port, cmd = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3:]
    lsock = socket.create_server(("127.0.0.1", open_port))          # cổng OPEN cho check_port
    print(f"[client] Mở cổng thử 127.0.0.1:{open_port}; khởi chạy MCP server: {' '.join(cmd)}", flush=True)
    proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    lines, errlog = queue.Queue(), []
    threading.Thread(target=lambda: [lines.put(x) for x in proc.stdout] and None, daemon=True).start()
    threading.Thread(target=lambda: errlog.extend(proc.stderr), daemon=True).start()
    for k, (label, msg) in enumerate(scenario(open_port, closed_port)):
        raw = msg if isinstance(msg, str) else dumps(msg)
        shown = raw if k == 0 or isinstance(msg, str) else short(msg)
        print(f"[client] ({label}) -> {shown}", flush=True)
        proc.stdin.write((raw + "\n").encode("utf-8"))              # mỗi thông điệp một dòng
        proc.stdin.flush()
        if isinstance(msg, dict) and "id" not in msg:
            print("[client]    (thông báo: không chờ phản hồi)", flush=True)
            continue
        try:
            reply = lines.get(timeout=15).decode("utf-8").rstrip("\n")
        except queue.Empty:
            print("[client]    Không nhận được phản hồi trong 15 s", flush=True)
            break
        try:
            json.loads(reply)
        except ValueError:
            print("[client]    CẢNH BÁO: stdout có dòng không phải JSON", flush=True)
        print(f"[client]    <- {reply}", flush=True)
    proc.stdin.close()                                               # host đóng stdin: server kết thúc
    code = proc.wait(timeout=15)
    lsock.close()
    print(f"[client] Đã đóng stdin; server kết thúc với mã {code}", flush=True)
    print("----- Nhật ký stderr của server -----", flush=True)
    for x in errlog:
        print(x.decode("utf-8", errors="replace").rstrip("\n"), flush=True)


if __name__ == "__main__":
    main()
