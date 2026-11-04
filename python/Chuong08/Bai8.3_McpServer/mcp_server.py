"""Bài 8.3 – MCP server công cụ mạng qua kênh stdio (đặc tả MCP 2026-07-28 như Mục 8.3 trình bày).

Cài đặt Mã giả 8.4 (MCP_STDIO_SERVER, DISPATCH, WRITE_MSG), Mã giả 8.5 (CALL_TOOL) và dùng
Mã giả 8.7 (kiểm soát đích) trong net_tools.py; Mục 8.3.5, 8.5.4 Giáo trình Lập trình mạng.
JSON-RPC 2.0, mỗi thông điệp một dòng UTF-8 trên stdin/stdout; nhật ký chỉ ghi ra stderr.
Biến môi trường ALLOWED_NETS: các mạng được phép (CIDR, phân cách dấu phẩy), ví dụ 10.10.0.0/16.

Chạy (do host khởi chạy như tiến trình con):  ALLOWED_NETS=127.0.0.0/8 python3 mcp_server.py
Thử nhanh:  python3 mcp_test_client.py 9000 9001 python3 mcp_server.py
"""
import json
import sys

from net_tools import (ALLOWED_NETS, NetError, PolicyError, ResolveError, family_name, log, ping, port_check,
                       resolve_host, validate_name, validate_port, validate_target)

SUPPORTED = ["2026-07-28"]                                           # dòng 1


def tool(name, description, params):
    """TOOL(tên, mô tả, tham số) -> mô tả công cụ kèm lược đồ JSON Schema của đầu vào."""
    return {"name": name, "description": description,
            "inputSchema": {"type": "object",
                            "properties": {k: {"type": t} for k, t in params.items()},
                            "required": list(params)}}


TOOLS = [                                                            # dòng 2–6
    tool("resolve_host", "Phân giải tên miền", {"host": "string"}),
    tool("check_port", "Kiểm tra cổng TCP", {"host": "string", "port": "integer"}),
    tool("ping", "Đo thời gian khứ hồi ICMP", {"host": "string"}),
]


class RpcError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code, self.message = code, message


def error_msg(msg_id, code, message):
    return {"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": message}}


def write_msg(obj):
    """WRITE_MSG: một thông điệp một dòng, FLUSH ngay (dòng 42–44)."""
    sys.stdout.buffer.write((json.dumps(obj, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8"))
    sys.stdout.buffer.flush()


def read_line(r, max_len):
    """READ_LINE (Mã giả 3.5) trên stdin: r = {"pending": bytes}; NULL khi host đóng stdin."""
    while True:
        i = r["pending"].find(b"\n")
        if i > max_len:                                         # Mã giả 3.5 dòng 5: có LF nhưng quá dài
            raise ValueError("Dòng quá dài")                    # ProtocolError
        if i >= 0:
            line, r["pending"] = r["pending"][:i], r["pending"][i + 1:]
            return line.rstrip(b"\r").decode("utf-8", errors="replace")
        if len(r["pending"]) > max_len:
            raise ValueError("Dòng quá dài")                        # ProtocolError
        chunk = sys.stdin.buffer.read1(4096)
        if not chunk:
            return None
        r["pending"] += chunk


def call_tool(name, a):
    """Mã giả 8.5 – CALL_TOOL(name, a): kiểm tra tham số trước khi thực thi công cụ."""
    if name not in [t["name"] for t in TOOLS]:
        raise RpcError(-32602, "Unknown tool: " + (name if isinstance(name, str) else "null"))
    log("  gọi công cụ", name, json.dumps(a, ensure_ascii=False, separators=(",", ":")))
    try:
        if name == "resolve_host":
            out = resolve_host(validate_name(a.get("host")))
            text = "; ".join(f"{family_name(f)} {x}" for f, x in out) or "Không có địa chỉ nào"   # TO_TEXT
        elif name == "check_port":
            ip = validate_target(a.get("host"))
            port = validate_port(a.get("port"))
            text = f"{ip}:{port} {port_check(ip, port, 2000)}"
        else:                                                         # "ping"
            addr, rtt, ttl = ping(validate_target(a.get("host")), 1, 2000)
            text = f"{addr}: thời gian khứ hồi {rtt:.3f} ms, TTL {ttl}"
        return {"resultType": "complete", "isError": False, "content": [{"type": "text", "text": text}]}
    except (NetError, PolicyError, ResolveError, TimeoutError) as e:  # dòng 17: báo lỗi cho mô hình
        log("  ->", type(e).__name__ + ":", e)
        return {"resultType": "complete", "isError": True, "content": [{"type": "text", "text": str(e)}]}


def dispatch(method, p):
    """Mã giả 8.4 – DISPATCH(method, p)."""
    p = p if isinstance(p, dict) else {}                              # params vắng: {}
    m = p.get("_meta") if isinstance(p.get("_meta"), dict) else {}    # _meta vắng: {}
    v = m.get("io.modelcontextprotocol/protocolVersion")
    caps = m.get("io.modelcontextprotocol/clientCapabilities")
    if v is None or caps is None:
        raise RpcError(-32602, "Invalid params: thiếu _meta")
    if v not in SUPPORTED:
        raise RpcError(-32022, "UnsupportedProtocolVersion")
    if method == "server/discover":                                   # DISCOVER_RESULT(SUPPORTED)
        return {"resultType": "complete", "supportedVersions": SUPPORTED,
                "capabilities": {"tools": {}}, "serverInfo": {"name": "nettools-mcp", "version": "1.0.0"}}
    if method == "tools/list":
        return {"resultType": "complete", "tools": TOOLS, "ttlMs": 3600000, "cacheScope": "public"}
    if method == "tools/call":
        return call_tool(p.get("name"), p["arguments"] if isinstance(p.get("arguments"), dict) else {})
    raise RpcError(-32601, "Method not found")


def mcp_stdio_server():
    """Mã giả 8.4 – MCP_STDIO_SERVER()."""
    log("MCP server công cụ mạng (stdio), phiên bản", ", ".join(SUPPORTED),
        "| ALLOWED_NETS =", ", ".join(map(str, ALLOWED_NETS)) or "(rỗng: chặn mọi đích)")
    r = {"pending": b""}
    while (line := read_line(r, 1048576)) is not None:                # dòng 8
        try:
            msg = json.loads(line)
        except ValueError:                                            # JsonError
            log("<- dòng không phải JSON -> -32700")
            write_msg(error_msg(None, -32700, "Parse error"))
            continue
        msg_id = msg.get("id") if isinstance(msg, dict) else None
        method = msg.get("method") if isinstance(msg, dict) else None
        if msg_id is None:                                            # thông báo: không trả lời
            log(f"<- thông báo {method}: không trả lời")
            continue
        log(f"<- id={msg_id} {method}")
        try:
            write_msg({"jsonrpc": "2.0", "id": msg_id, "result": dispatch(method, msg.get("params"))})
            log(f"-> id={msg_id} result")
        except RpcError as e:
            write_msg(error_msg(msg_id, e.code, e.message))
            log(f"-> id={msg_id} error {e.code}")
        except Exception as e:                                        # bổ sung: lỗi ngoài dự kiến -> -32603
            write_msg(error_msg(msg_id, -32603, "Internal error"))
            log(f"-> id={msg_id} error -32603 ({type(e).__name__}: {e})")
    log("Host đã đóng stdin, kết thúc")


if __name__ == "__main__":
    mcp_stdio_server()
