"""Bài mở rộng MR8.4 – MCP server có công cụ fetch_url, so sánh bản NGÂY THƠ và bản AN TOÀN.

Nối tiếp Bài 8.3 (MCP server công cụ mạng qua stdio, JSON-RPC 2.0 – giữ nguyên khung thông điệp).
Bài mở rộng, không có trong giáo trình. Hai công cụ:
  - fetch_url_naive(url): tải URL KHÔNG kiểm tra (minh họa lỗ hổng SSRF – không dùng trong thực tế);
  - fetch_url_safe(url):  tải URL có đầy đủ lớp phòng thủ SSRF.
JSON-RPC 2.0, mỗi thông điệp một dòng UTF-8 trên stdin/stdout; nhật ký chỉ ghi ra stderr.

Chạy (do host khởi chạy như tiến trình con):  python3 mcp_server_ssrf.py
"""
import json
import sys

from fetch_tools import FetchError, SsrfError, fetch_naive, fetch_safe, log_err

SUPPORTED = ["2026-07-28"]


def tool(name, description, params):
    return {"name": name, "description": description,
            "inputSchema": {"type": "object",
                            "properties": {k: {"type": t} for k, t in params.items()},
                            "required": list(params)}}


TOOLS = [
    tool("fetch_url_naive", "Tải URL KHÔNG kiểm tra (minh họa lỗ hổng SSRF)", {"url": "string"}),
    tool("fetch_url_safe", "Tải URL có kiểm soát chống SSRF", {"url": "string"}),
]


class RpcError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code, self.message = code, message


def error_msg(msg_id, code, message):
    return {"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": message}}


def write_msg(obj):
    sys.stdout.buffer.write((json.dumps(obj, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8"))
    sys.stdout.buffer.flush()


def read_line(r, max_len):
    while True:
        i = r["pending"].find(b"\n")
        if i > max_len or (i < 0 and len(r["pending"]) > max_len):
            raise ValueError("Dòng quá dài")
        if i >= 0:
            line, r["pending"] = r["pending"][:i], r["pending"][i + 1:]
            return line.rstrip(b"\r").decode("utf-8", errors="replace")
        chunk = sys.stdin.buffer.read1(4096)
        if not chunk:
            return None
        r["pending"] += chunk


def tom_tat_than(body):
    """Rút gọn phần thân phản hồi để in (một dòng, tối đa 120 ký tự)."""
    text = body.decode("utf-8", errors="replace").replace("\n", " ").strip()
    return text[:120] + ("…" if len(text) > 120 else "")


def call_tool(name, a):
    if name not in [t["name"] for t in TOOLS]:
        raise RpcError(-32602, "Unknown tool: " + (name if isinstance(name, str) else "null"))
    url = a.get("url")
    if not isinstance(url, str):
        return {"resultType": "complete", "isError": True,
                "content": [{"type": "text", "text": "Tham số 'url' phải là chuỗi"}]}
    log_err("gọi", name, url)
    try:
        if name == "fetch_url_naive":
            status, body, steps = fetch_naive(url)
        else:
            status, body, steps = fetch_safe(url)
        for s in steps:
            log_err("   ", s)
        text = f"HTTP {status} | " + " | ".join(steps[-2:]) + " | thân: " + tom_tat_than(body)
        return {"resultType": "complete", "isError": False, "content": [{"type": "text", "text": text}]}
    except (SsrfError, FetchError) as e:
        log_err("   TỪ CHỐI/LỖI:", type(e).__name__, e)
        return {"resultType": "complete", "isError": True, "content": [{"type": "text", "text": str(e)}]}


def dispatch(method, p):
    p = p if isinstance(p, dict) else {}
    m = p.get("_meta") if isinstance(p.get("_meta"), dict) else {}
    v = m.get("io.modelcontextprotocol/protocolVersion")
    caps = m.get("io.modelcontextprotocol/clientCapabilities")
    if v is None or caps is None:
        raise RpcError(-32602, "Invalid params: thiếu _meta")
    if v not in SUPPORTED:
        raise RpcError(-32022, "UnsupportedProtocolVersion")
    if method == "server/discover":
        return {"resultType": "complete", "supportedVersions": SUPPORTED,
                "capabilities": {"tools": {}}, "serverInfo": {"name": "ssrf-guard-mcp", "version": "1.0.0"}}
    if method == "tools/list":
        return {"resultType": "complete", "tools": TOOLS, "ttlMs": 3600000, "cacheScope": "public"}
    if method == "tools/call":
        return call_tool(p.get("name"), p["arguments"] if isinstance(p.get("arguments"), dict) else {})
    raise RpcError(-32601, "Method not found")


def mcp_stdio_server():
    log_err("MCP server chống SSRF (stdio), phiên bản", ", ".join(SUPPORTED))
    r = {"pending": b""}
    while (line := read_line(r, 1048576)) is not None:
        try:
            msg = json.loads(line)
        except ValueError:
            write_msg(error_msg(None, -32700, "Parse error"))
            continue
        msg_id = msg.get("id") if isinstance(msg, dict) else None
        method = msg.get("method") if isinstance(msg, dict) else None
        if msg_id is None:
            log_err(f"<- thông báo {method}: không trả lời")
            continue
        log_err(f"<- id={msg_id} {method}")
        try:
            write_msg({"jsonrpc": "2.0", "id": msg_id, "result": dispatch(method, msg.get("params"))})
        except RpcError as e:
            write_msg(error_msg(msg_id, e.code, e.message))
        except Exception as e:
            write_msg(error_msg(msg_id, -32603, "Internal error"))
            log_err(f"-> id={msg_id} error -32603 ({type(e).__name__}: {e})")
    log_err("Host đã đóng stdin, kết thúc")


if __name__ == "__main__":
    mcp_stdio_server()
