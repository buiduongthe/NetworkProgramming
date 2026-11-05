//! Bài mở rộng MR8.4 – MCP server có công cụ fetch_url, so sánh bản NGÂY THƠ và bản AN TOÀN.
//!
//! Nối tiếp Bài 8.3 (MCP server công cụ mạng qua stdio, JSON-RPC 2.0 – giữ nguyên khung thông điệp).
//! Bài mở rộng, không có trong giáo trình. Hai công cụ:
//!   - fetch_url_naive(url): tải URL KHÔNG kiểm tra (minh họa lỗ hổng SSRF – không dùng trong thực tế);
//!   - fetch_url_safe(url):  tải URL có đầy đủ lớp phòng thủ SSRF.
//! JSON-RPC 2.0, mỗi thông điệp một dòng UTF-8 trên stdin/stdout; nhật ký chỉ ghi ra stderr.
//!
//! Chạy (do host khởi chạy như tiến trình con):  mr8_4_chong_ssrf_mcp

mod fetch_tools;

use fetch_tools::Fetcher;
use serde_json::{json, Value};
use std::io::{Read, Write};

const SUPPORTED: [&str; 1] = ["2026-07-28"];

fn log(msg: &str) {
    eprintln!("[ssrf] {msg}");
}

/// Mô tả một công cụ (name, description, inputSchema) – giữ đúng thứ tự khóa như bản Python.
fn tool(name: &str, description: &str) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}
    })
}

fn tools() -> Vec<Value> {
    vec![
        tool("fetch_url_naive", "Tải URL KHÔNG kiểm tra (minh họa lỗ hổng SSRF)"),
        tool("fetch_url_safe", "Tải URL có kiểm soát chống SSRF"),
    ]
}

struct RpcError {
    code: i64,
    message: String,
}

fn error_msg(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn write_msg(obj: &Value) {
    let mut out = serde_json::to_string(obj).unwrap();
    out.push('\n');
    let so = std::io::stdout();
    let mut h = so.lock();
    let _ = h.write_all(out.as_bytes());
    let _ = h.flush();
}

/// Rút gọn phần thân phản hồi để in (một dòng, tối đa 120 ký tự).
fn tom_tat_than(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body).replace('\n', " ").trim().to_string();
    if text.chars().count() > 120 {
        let cat: String = text.chars().take(120).collect();
        format!("{cat}…")
    } else {
        text
    }
}

fn call_tool(fetcher: &mut Fetcher, name: Option<&str>, args: &Value) -> Result<Value, RpcError> {
    let ten_hop_le = tools().iter().any(|t| t["name"].as_str() == name);
    if !ten_hop_le {
        return Err(RpcError { code: -32602, message: format!("Unknown tool: {}", name.unwrap_or("null")) });
    }
    let name = name.unwrap();
    let url = match args.get("url").and_then(Value::as_str) {
        Some(u) => u,
        None => return Ok(ket_qua(true, "Tham số 'url' phải là chuỗi")),
    };
    log(&format!("gọi {name} {url}"));
    let kq = if name == "fetch_url_naive" { fetcher.fetch_naive(url) } else { fetcher.fetch_safe(url) };
    match kq {
        Ok((status, body, steps)) => {
            for s in &steps {
                log(&format!("    {s}"));
            }
            let cuoi: Vec<&String> = steps.iter().rev().take(2).rev().collect();
            let cuoi_txt: Vec<&str> = cuoi.iter().map(|s| s.as_str()).collect();
            let text = format!("HTTP {status} | {} | thân: {}", cuoi_txt.join(" | "), tom_tat_than(&body));
            Ok(ket_qua(false, &text))
        }
        Err(e) => {
            log(&format!("   TỪ CHỐI/LỖI: {} {}", e.ten(), e.thong_bao()));
            Ok(ket_qua(true, e.thong_bao()))
        }
    }
}

fn ket_qua(is_error: bool, text: &str) -> Value {
    json!({"resultType": "complete", "isError": is_error, "content": [{"type": "text", "text": text}]})
}

fn dispatch(fetcher: &mut Fetcher, method: Option<&str>, p: &Value) -> Result<Value, RpcError> {
    let p = if p.is_object() { p.clone() } else { json!({}) };
    let meta = &p["_meta"];
    let v = meta.get("io.modelcontextprotocol/protocolVersion");
    let caps = meta.get("io.modelcontextprotocol/clientCapabilities");
    if v.is_none() || caps.is_none() {
        return Err(RpcError { code: -32602, message: "Invalid params: thiếu _meta".into() });
    }
    if !v.and_then(Value::as_str).is_some_and(|s| SUPPORTED.contains(&s)) {
        return Err(RpcError { code: -32022, message: "UnsupportedProtocolVersion".into() });
    }
    match method {
        Some("server/discover") => Ok(json!({
            "resultType": "complete", "supportedVersions": SUPPORTED,
            "capabilities": {"tools": {}}, "serverInfo": {"name": "ssrf-guard-mcp", "version": "1.0.0"}
        })),
        Some("tools/list") => Ok(json!({"resultType": "complete", "tools": tools(), "ttlMs": 3600000, "cacheScope": "public"})),
        Some("tools/call") => {
            let args = if p["arguments"].is_object() { p["arguments"].clone() } else { json!({}) };
            call_tool(fetcher, p["name"].as_str(), &args)
        }
        _ => Err(RpcError { code: -32601, message: "Method not found".into() }),
    }
}

/// Đọc một dòng (kết thúc '\n') từ stdin, giới hạn độ dài; None khi hết dòng.
fn read_line(stdin: &mut impl Read, pending: &mut Vec<u8>, max_len: usize) -> Result<Option<String>, ()> {
    loop {
        if let Some(i) = pending.iter().position(|&b| b == b'\n') {
            if i > max_len {
                return Err(());
            }
            let mut line: Vec<u8> = pending.drain(..=i).collect();
            line.pop(); // bỏ '\n'
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(String::from_utf8_lossy(&line).into_owned()));
        }
        if pending.len() > max_len {
            return Err(());
        }
        let mut buf = [0u8; 4096];
        match stdin.read(&mut buf) {
            Ok(0) => return Ok(None),
            Ok(n) => pending.extend_from_slice(&buf[..n]),
            Err(_) => return Ok(None),
        }
    }
}

fn main() {
    log(&format!("MCP server chống SSRF (stdio), phiên bản {}", SUPPORTED.join(", ")));
    let mut fetcher = Fetcher::tu_moi_truong();
    let mut stdin = std::io::stdin();
    let mut pending = Vec::new();
    loop {
        let line = match read_line(&mut stdin, &mut pending, 1_048_576) {
            Ok(Some(l)) => l,
            Ok(None) => break,
            Err(()) => {
                write_msg(&error_msg(Value::Null, -32700, "Parse error"));
                continue;
            }
        };
        let msg: Value = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(_) => {
                write_msg(&error_msg(Value::Null, -32700, "Parse error"));
                continue;
            }
        };
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let method = msg.get("method").and_then(Value::as_str);
        if id.is_null() {
            log(&format!("<- thông báo {}: không trả lời", method.unwrap_or("null")));
            continue;
        }
        log(&format!("<- id={} {}", id, method.unwrap_or("null")));
        match dispatch(&mut fetcher, method, &msg["params"]) {
            Ok(result) => write_msg(&json!({"jsonrpc": "2.0", "id": id, "result": result})),
            Err(e) => write_msg(&error_msg(id, e.code, &e.message)),
        }
    }
    log("Host đã đóng stdin, kết thúc");
}
