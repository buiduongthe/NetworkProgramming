//! Bài 8.3 – MCP server công cụ mạng qua kênh stdio (đặc tả MCP 2026-07-28 như Mục 8.3 trình bày).
//!
//! Cài đặt Mã giả 8.4 (MCP_STDIO_SERVER, DISPATCH, WRITE_MSG), Mã giả 8.5 (CALL_TOOL) và dùng
//! Mã giả 8.7 (kiểm soát đích) trong net_tools.rs; Mục 8.3.5, 8.5.4 Giáo trình Lập trình mạng.
//! JSON-RPC 2.0, mỗi thông điệp một dòng UTF-8 trên stdin/stdout; nhật ký chỉ ghi ra stderr.
//! Biến môi trường ALLOWED_NETS: các mạng được phép (CIDR, phân cách dấu phẩy), ví dụ 10.10.0.0/16.
//!
//! Chạy (do host khởi chạy như tiến trình con):  ALLOWED_NETS=127.0.0.0/8 cargo run -q --release
//! Thử nhanh:  python3 mcp_test_client.py 9000 9001 cargo run -q --release
mod net_tools;

use net_tools::{family_name, log, ping, port_check, resolve_host, validate_name, validate_port, validate_target, ToolError, ALLOWED_NETS};
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, Read, Write};

const SUPPORTED: [&str; 1] = ["2026-07-28"]; // dòng 1

/// TOOL(tên, mô tả, tham số) -> mô tả công cụ kèm lược đồ JSON Schema của đầu vào.
fn tool(name: &str, description: &str, params: &[(&str, &str)]) -> Value {
    let mut props = Map::new();
    for (k, t) in params {
        props.insert(k.to_string(), json!({"type": t}));
    }
    let required: Vec<&str> = params.iter().map(|(k, _)| *k).collect();
    json!({"name": name, "description": description,
           "inputSchema": {"type": "object", "properties": props, "required": required}})
}

/// TOOLS (dòng 2–6).
fn tools() -> Vec<Value> {
    vec![
        tool("resolve_host", "Phân giải tên miền", &[("host", "string")]),
        tool("check_port", "Kiểm tra cổng TCP", &[("host", "string"), ("port", "integer")]),
        tool("ping", "Đo thời gian khứ hồi ICMP", &[("host", "string")]),
    ]
}

/// Lỗi JSON-RPC: mã và thông điệp.
struct RpcError {
    code: i64,
    message: String,
}

fn rpc_error(code: i64, message: &str) -> RpcError {
    RpcError { code, message: message.to_string() }
}

fn error_msg(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// WRITE_MSG: một thông điệp một dòng, FLUSH ngay.
fn write_msg(obj: &Value) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{}", serde_json::to_string(obj).unwrap()); // JSON gọn, UTF-8
    let _ = out.flush();
}

/// READ_LINE (Mã giả 3.5) trên stdin; None khi host đóng stdin.
fn read_line(r: &mut impl BufRead, max_len: usize) -> io::Result<Option<String>> {
    let mut buf = Vec::new();
    r.by_ref().take(max_len as u64 + 1).read_until(b'\n', &mut buf)?;
    if buf.is_empty() {
        return Ok(None);
    }
    if buf.len() > max_len && !buf.ends_with(b"\n") {
        return Err(io::Error::other("Dòng quá dài")); // ProtocolError
    }
    while buf.last() == Some(&b'\n') || buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

/// Mã giả 8.5 – CALL_TOOL(name, a): kiểm tra tham số trước khi thực thi công cụ.
fn call_tool(name: &Value, a: &Value) -> Result<Value, RpcError> {
    let known = tools().iter().any(|t| t["name"] == *name);
    let Some(name) = name.as_str().filter(|_| known) else {
        return Err(rpc_error(-32602, &format!("Unknown tool: {}", name.as_str().unwrap_or("null"))));
    };
    log(&format!("  gọi công cụ {name} {a}"));
    let out: Result<String, ToolError> = (|| match name {
        "resolve_host" => {
            let addrs = resolve_host(&validate_name(&a["host"])?);
            let text: Vec<String> = addrs.iter().map(|x| format!("{} {x}", family_name(x))).collect(); // TO_TEXT
            Ok(if text.is_empty() { "Không có địa chỉ nào".to_string() } else { text.join("; ") })
        }
        "check_port" => {
            let ip = validate_target(&a["host"])?;
            let port = validate_port(&a["port"])?;
            let target = std::net::SocketAddr::new(ip, port); // in IPv6 dạng [::1]:cổng
            Ok(format!("{target} {}", port_check(&ip.to_string(), port, 2000)))
        }
        _ => {
            // "ping"
            let (addr, rtt, ttl) = ping(validate_target(&a["host"])?, 1, 2000)?;
            Ok(format!("{addr}: thời gian khứ hồi {rtt:.3} ms, TTL {ttl}"))
        }
    })();
    match out {
        Ok(text) => Ok(json!({"resultType": "complete", "isError": false, "content": [{"type": "text", "text": text}]})),
        Err(e) => {
            // CATCH NetError, PolicyError, ResolveError, Timeout: báo lỗi cho mô hình
            log(&format!("  -> {}: {e}", e.kind()));
            Ok(json!({"resultType": "complete", "isError": true, "content": [{"type": "text", "text": e.to_string()}]}))
        }
    }
}

/// Mã giả 8.4 – DISPATCH(method, p).
fn dispatch(method: &Value, p: &Value) -> Result<Value, RpcError> {
    let empty = json!({});
    let p = if p.is_object() { p } else { &empty }; // params vắng: {}
    let m = if p["_meta"].is_object() { &p["_meta"] } else { &empty }; // _meta vắng: {}
    let v = &m["io.modelcontextprotocol/protocolVersion"];
    let caps = &m["io.modelcontextprotocol/clientCapabilities"];
    if v.is_null() || caps.is_null() {
        return Err(rpc_error(-32602, "Invalid params: thiếu _meta"));
    }
    if !SUPPORTED.iter().any(|s| v == s) {
        return Err(rpc_error(-32022, "UnsupportedProtocolVersion"));
    }
    match method.as_str().unwrap_or("") {
        // DISCOVER_RESULT(SUPPORTED)
        "server/discover" => Ok(json!({"resultType": "complete", "supportedVersions": SUPPORTED,
                                        "capabilities": {"tools": {}},
                                        "serverInfo": {"name": "nettools-mcp", "version": "1.0.0"}})),
        "tools/list" => Ok(json!({"resultType": "complete", "tools": tools(), "ttlMs": 3600000, "cacheScope": "public"})),
        "tools/call" => {
            let args = if p["arguments"].is_object() { &p["arguments"] } else { &empty };
            call_tool(&p["name"], args)
        }
        _ => Err(rpc_error(-32601, "Method not found")),
    }
}

/// Mã giả 8.4 – MCP_STDIO_SERVER().
fn main() {
    let nets: Vec<String> = ALLOWED_NETS.iter().map(|n| n.to_string()).collect();
    let nets = if nets.is_empty() { "(rỗng: chặn mọi đích)".to_string() } else { nets.join(", ") };
    log(&format!("MCP server công cụ mạng (stdio), phiên bản {} | ALLOWED_NETS = {nets}", SUPPORTED.join(", ")));
    let mut r = io::stdin().lock();
    loop {
        let line = match read_line(&mut r, 1048576) {
            Ok(Some(line)) => line, // dòng 8
            Ok(None) => break,
            Err(e) => {
                log(&format!("Lỗi đọc stdin: {e}, kết thúc"));
                std::process::exit(1);
            }
        };
        let msg: Value = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(_) => {
                // JsonError
                log("<- dòng không phải JSON -> -32700");
                write_msg(&error_msg(&Value::Null, -32700, "Parse error"));
                continue;
            }
        };
        let id = &msg["id"]; // thông điệp không phải đối tượng: coi như không có id
        let method = &msg["method"];
        let method_text = method.as_str().unwrap_or("null");
        if id.is_null() {
            log(&format!("<- thông báo {method_text}: không trả lời")); // thông báo: không trả lời
            continue;
        }
        log(&format!("<- id={id} {method_text}"));
        match dispatch(method, &msg["params"]) {
            Ok(result) => {
                write_msg(&json!({"jsonrpc": "2.0", "id": id, "result": result}));
                log(&format!("-> id={id} result"));
            }
            Err(e) => {
                write_msg(&error_msg(id, e.code, &e.message));
                log(&format!("-> id={id} error {}", e.code));
            }
        }
    }
    log("Host đã đóng stdin, kết thúc");
}
