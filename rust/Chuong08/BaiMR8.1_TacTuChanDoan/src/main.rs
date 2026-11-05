//! Bài mở rộng MR8.1 – Tác tử AI chẩn đoán mạng bằng gọi công cụ (tool calling), có lớp kiểm soát.
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 8.1 (gọi API LLM) và Bài 8.3 (MCP server):
//! Bài 8.3 viết phía server công cụ, bài này viết phía "chủ" (host/agent) – vòng lặp
//!     hỏi LLM -> LLM yêu cầu công cụ -> chương trình KIỂM TRA rồi mới chạy -> gửi kết quả -> ...
//! cho tới khi LLM trả lời bằng văn bản. LLM không bao giờ tự chạy lệnh; mọi thao tác mạng do mã của
//! ta thực hiện, sau khi qua các lớp kiểm soát:
//!   1. Chỉ chạy công cụ có trong danh mục (chặn công cụ "bịa" do ảo giác).
//!   2. Tham số phải là JSON hợp lệ, đủ trường, đúng kiểu, trong miền giá trị.
//!   3. Chính sách đích: chỉ máy thuộc mạng phòng thí nghiệm (loopback, mạng riêng RFC 1918).
//!   4. Giới hạn số vòng lặp; ghi nhật ký kiểm toán từng lời gọi.
//! Cấu hình qua biến môi trường: LLM_BASE_URL, LLM_API_KEY, LLM_MODEL (như Bài 8.1).
//!
//! Chạy:  cargo run -q --release -- "<câu hỏi>"

mod http_post;

use serde_json::{json, Map, Value};
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::process;
use std::time::Duration;

const MAX_ROUNDS: usize = 4;
/// Mạng phòng thí nghiệm được phép: (địa chỉ mạng, độ dài tiền tố).
const ALLOWED_NETS: [([u8; 4], u32); 4] = [([127, 0, 0, 0], 8), ([10, 0, 0, 0], 8), ([172, 16, 0, 0], 12), ([192, 168, 0, 0], 16)];

/// Lời gọi công cụ bị từ chối hoặc thất bại – kết quả gửi lại LLM dưới dạng {"loi": ...}.
struct ToolError(String);

fn allowed(ip: IpAddr) -> bool {
    let IpAddr::V4(v4) = ip else { return false }; // các mạng cho phép đều là IPv4
    ALLOWED_NETS.iter().any(|(net, plen)| {
        let mask = u32::MAX << (32 - plen);
        u32::from(v4) & mask == u32::from(Ipv4Addr::from(*net))
    })
}

// ---------- Thân công cụ (chạy bằng socket API, không gọi shell) ----------
/// Phân giải tên thành danh sách địa chỉ (đã sắp xếp, không trùng).
fn resolve(name: &str) -> Vec<String> {
    let mut v: Vec<String> = (name, 0).to_socket_addrs().map(|it| it.map(|a| a.ip().to_string()).collect()).unwrap_or_default();
    v.sort();
    v.dedup();
    v
}

/// Chính sách đích: mọi địa chỉ của host phải thuộc ALLOWED_NETS.
fn check_target(host: &str) -> Result<Vec<String>, ToolError> {
    let addrs = resolve(host);
    if addrs.is_empty() {
        return Err(ToolError(format!("không phân giải được {host}")));
    }
    for a in &addrs {
        if !a.parse().is_ok_and(allowed) {
            return Err(ToolError(format!("{a} nằm ngoài mạng phòng thí nghiệm")));
        }
    }
    Ok(addrs)
}

fn tool_resolve(args: &Map<String, Value>) -> Result<Value, ToolError> {
    Ok(json!({"dia_chi": resolve(args["ten"].as_str().unwrap_or(""))}))
}

fn tool_port(args: &Map<String, Value>) -> Result<Value, ToolError> {
    let addrs = check_target(args["host"].as_str().unwrap_or(""))?;
    let port = args["cong"].as_i64().unwrap_or(0) as u16;
    let timeout = Duration::from_millis(args.get("thoi_gian_cho_ms").and_then(Value::as_u64).unwrap_or(1000));
    let mut state = "FILTERED";
    for a in addrs {
        let sa = SocketAddr::new(a.parse().unwrap(), port);
        match TcpStream::connect_timeout(&sa, timeout) {
            Ok(_) => return Ok(json!({"trang_thai": "OPEN", "dia_chi": a})),
            Err(e) if e.kind() == ErrorKind::ConnectionRefused => state = "CLOSED",
            Err(_) => {}
        }
    }
    Ok(json!({"trang_thai": state}))
}

type ToolFn = fn(&Map<String, Value>) -> Result<Value, ToolError>;

/// Danh mục công cụ: (tên, hàm, mô tả, lược đồ tham số JSON Schema gửi cho LLM).
fn tools() -> Vec<(&'static str, ToolFn, &'static str, Value)> {
    vec![
        (
            "phan_giai_ten",
            tool_resolve,
            "Phân giải tên máy thành danh sách địa chỉ IP",
            json!({"type": "object", "required": ["ten"],
                   "properties": {"ten": {"type": "string", "description": "tên máy cần phân giải"}}}),
        ),
        (
            "kiem_tra_cong",
            tool_port,
            "Kiểm tra cổng TCP: OPEN, CLOSED hoặc FILTERED",
            json!({"type": "object", "required": ["host", "cong"],
                   "properties": {"host": {"type": "string"},
                                  "cong": {"type": "integer", "minimum": 1, "maximum": 65535},
                                  "thoi_gian_cho_ms": {"type": "integer", "minimum": 100, "maximum": 5000}}}),
        ),
    ]
}

/// Kiểm tra tham số theo lược đồ (tập con nhỏ của JSON Schema: required, type, minimum, maximum).
fn validate(schema: &Value, args: &Value) -> Result<(), ToolError> {
    let Some(args) = args.as_object() else { return Err(ToolError("tham số phải là đối tượng JSON".into())) };
    for key in schema["required"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        if !args.contains_key(key) {
            return Err(ToolError(format!("thiếu tham số {key}")));
        }
    }
    for (key, value) in args {
        let Some(spec) = schema["properties"].get(key) else { return Err(ToolError(format!("tham số lạ {key}"))) };
        match spec["type"].as_str() {
            Some("integer") if value.as_i64().is_none() => return Err(ToolError(format!("{key} phải là số nguyên"))),
            Some("string") if !value.is_string() => return Err(ToolError(format!("{key} phải là chuỗi"))),
            _ => {}
        }
        if let (Some(lo), Some(hi)) = (spec["minimum"].as_i64(), spec["maximum"].as_i64()) {
            if !(lo..=hi).contains(&value.as_i64().unwrap_or(i64::MIN)) {
                return Err(ToolError(format!("{key} ngoài miền [{lo}, {hi}]")));
            }
        }
    }
    Ok(())
}

/// Ba lớp kiểm soát trước khi chạy: danh mục, tham số, chính sách đích (trong thân công cụ).
fn run_tool(name: &str, arguments: &str) -> Result<Value, ToolError> {
    let Some((_, f, _, schema)) = tools().into_iter().find(|t| t.0 == name) else {
        return Err(ToolError("công cụ không có trong danh mục".into()));
    };
    let args: Value = serde_json::from_str(arguments).map_err(|_| ToolError("tham số không phải JSON".into()))?;
    validate(&schema, &args)?;
    f(args.as_object().unwrap())
}

/// JSON gọn, khóa theo thứ tự chữ cái (giống sort_keys=True của Python, json.Marshal của Go với map).
fn compact(v: &Value) -> String {
    fn sorted(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                Value::Object(keys.into_iter().map(|k| (k.clone(), sorted(&m[k]))).collect())
            }
            Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    sorted(v).to_string()
}

/// Ngắt dòng theo từ, tối đa `width` ký tự mỗi dòng (như textwrap.wrap của Python).
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_string();
        while word.chars().count() > width {
            // từ dài hơn cả dòng: cắt cứng
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            let head: String = word.chars().take(width).collect();
            word = word.chars().skip(width).collect();
            lines.push(head);
        }
        if cur.is_empty() {
            cur = word;
        } else if cur.chars().count() + 1 + word.chars().count() <= width {
            cur.push(' ');
            cur.push_str(&word);
        } else {
            lines.push(std::mem::replace(&mut cur, word));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

// ---------- Gọi API LLM (không truyền luồng) ----------
fn chat(base_url: &str, model: &str, messages: &[Value]) -> Result<Value, String> {
    let Ok(key) = std::env::var("LLM_API_KEY") else {
        eprintln!("Thiếu biến môi trường LLM_API_KEY");
        process::exit(1)
    };
    let tool_list: Vec<Value> = tools()
        .into_iter()
        .map(|(n, _, d, s)| json!({"type": "function", "function": {"name": n, "description": d, "parameters": s}}))
        .collect();
    let req = json!({"model": model, "messages": messages, "temperature": 0, "tools": tool_list});
    let hdrs = [("Content-Type", "application/json".to_string()), ("Authorization", format!("Bearer {key}"))];
    let (status, body) = http_post::post_json(&format!("{base_url}/chat/completions"), req.to_string().as_bytes(), &hdrs, Duration::from_secs(10)).map_err(|e| e.to_string())?;
    if status != 200 {
        return Err(format!("HTTP Error {status}: {}", String::from_utf8_lossy(&body).chars().take(120).collect::<String>()));
    }
    let v: Value = serde_json::from_slice(&body).map_err(|e| format!("phản hồi không phải JSON: {e}"))?;
    Ok(v["choices"][0]["message"].clone())
}

fn main() {
    let question = std::env::args().nth(1).unwrap_or_else(|| "Kiểm tra localhost cổng 80".into());
    let base_url = std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:9000/v1".into()).trim_end_matches('/').to_string();
    let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "mock-model".into());
    println!("[tac-tu] Hỏi: {question}");
    let mut messages = vec![
        json!({"role": "system", "content": "Bạn là trợ lý chẩn đoán mạng. Chỉ dùng công cụ được cung cấp."}),
        json!({"role": "user", "content": question}),
    ];
    for rnd in 1..=MAX_ROUNDS {
        let msg = chat(&base_url, &model, &messages).unwrap_or_else(|e| {
            println!("[tac-tu] Lỗi gọi LLM: {e}");
            process::exit(1)
        });
        messages.push(msg.clone());
        let calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
        if calls.is_empty() {
            println!("[tac-tu] Vòng {rnd}: LLM trả lời");
            for line in wrap(msg["content"].as_str().unwrap_or(""), 58) {
                println!("  {line}");
            }
            return;
        }
        println!("[tac-tu] Vòng {rnd}: LLM yêu cầu {} công cụ", calls.len());
        for c in calls {
            let name = c["function"]["name"].as_str().unwrap_or("");
            let arguments = c["function"]["arguments"].as_str().unwrap_or("");
            let result = match run_tool(name, arguments) {
                Ok(r) => {
                    println!("  CHẠY  {name}{arguments} -> {}", compact(&r));
                    r
                }
                Err(ToolError(e)) => {
                    println!("  CHẶN  {name}{arguments}: {e}");
                    json!({"loi": e})
                }
            };
            messages.push(json!({"role": "tool", "tool_call_id": c["id"], "content": compact(&result)}));
        }
    }
    println!("[tac-tu] Dừng: quá {MAX_ROUNDS} vòng mà chưa có câu trả lời");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiem_soat() {
        assert!(matches!(run_tool("khoi_dong_lai", "{}"), Err(ToolError(e)) if e == "công cụ không có trong danh mục"));
        assert!(matches!(run_tool("kiem_tra_cong", "{\"host\":\"localhost\"}"), Err(ToolError(e)) if e == "thiếu tham số cong"));
        assert!(matches!(run_tool("kiem_tra_cong", "{\"host\":\"x\",\"cong\":70000}"), Err(ToolError(e)) if e == "cong ngoài miền [1, 65535]"));
        assert!(matches!(run_tool("kiem_tra_cong", "{\"host\":\"8.8.8.8\",\"cong\":53}"), Err(ToolError(e)) if e.contains("ngoài mạng")));
        assert_eq!(compact(&json!({"b": 1, "a": [2]})), "{\"a\":[2],\"b\":1}");
    }
}
