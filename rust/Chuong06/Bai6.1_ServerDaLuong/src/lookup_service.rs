//! Bài 6.1 – Nghiệp vụ tra cứu thuật ngữ, chép gọn từ Mã giả 3.7 (Mục 3.5.1).
//!
//! PROCESS_REQUEST độc lập với socket; HANDLE_LOOKUP là vòng phục vụ một client.
//! Cả hai server của bài (thread_server.rs, pool_server.rs) dùng lại nguyên vẹn.
use crate::netutil::{recv_json, send_json, NetError};
use serde_json::{json, Map, Value};
use std::net::TcpStream;
use std::sync::LazyLock;

/// LOAD_JSON_FILE("glossary.json"): đọc một lần, dùng chung cho mọi luồng.
static DICT: LazyLock<Map<String, Value>> = LazyLock::new(|| {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/glossary.json"); // tệp nằm cạnh mã nguồn
    let text = std::fs::read_to_string(path).expect("Không đọc được glossary.json");
    serde_json::from_str(&text).expect("glossary.json sai định dạng")
});

/// Mã giả 3.7, dòng 2–12: ánh xạ thao tác → phản hồi.
pub fn process_request(req: &Value) -> Value {
    match req["op"].as_str() {
        Some("lookup") => {
            let term = req["term"].as_str().unwrap_or("");
            match DICT.get(term) {
                Some(text) => json!({"status": 200, "term": term, "text": text}),
                None => json!({"status": 404, "error": "Không có thuật ngữ"}),
            }
        }
        Some("list") => {
            let terms: Vec<&String> = DICT.keys().collect();
            json!({"status": 200, "terms": terms})
        }
        _ => json!({"status": 400, "error": "Sai thao tác"}),
    }
}

/// Mã giả 3.7, dòng 14–27: nhận yêu cầu, trả phản hồi đến khi client gửi bye hoặc đóng.
pub fn handle_lookup(conn: &mut TcpStream) -> Result<(), NetError> {
    loop {
        let req = match recv_json(conn) {
            Ok(r) => r,
            Err(NetError::ConnectionClosed(0)) => return Ok(()), // đóng tại ranh giới khung
            Err(NetError::Json(_)) => {
                // JsonError (gồm lỗi giải mã UTF-8)
                send_json(conn, &json!({"status": 400, "error": "JSON sai"}))?;
                continue;
            }
            Err(e) => return Err(e), // đóng giữa khung, khung quá lớn, lỗi mạng
        };
        if req["op"] == "bye" {
            return Ok(());
        }
        send_json(conn, &process_request(&req))?;
    }
}
