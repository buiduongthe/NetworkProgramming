//! Server tra cứu thuật ngữ dùng khung JSON.
//!
//! Cài đặt Mã giả 3.7 (Mục 3.5.1, Giáo trình Lập trình mạng) theo đặc tả giao thức dịch vụ tra cứu:
//! TCP cổng 9100, khung tiền tố độ dài 4 byte, nội dung JSON UTF-8.
//! Kế thừa: khung sườn ECHO_SERVER (Mã giả 3.1) với HANDLE_ECHO thay bằng HANDLE_LOOKUP;
//! SEND_JSON/RECV_JSON (Mã giả 3.6, tệp framing.rs).

use crate::framing::{recv_json, send_json, show, FrameError};
use crate::netutil::parse_endpoint;
use serde_json::{json, Map, Value};
use std::fs;
use std::net::{TcpListener, TcpStream};
use std::process;

/// DICT ← LOAD_JSON_FILE("glossary.json") – đọc một lần khi server khởi động.
fn load_dict(path: &str) -> Map<String, Value> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("[server] Không đọc được {path}: {e}");
        process::exit(1);
    });
    serde_json::from_str(&text).unwrap_or_else(|e| {
        eprintln!("[server] {path} không phải đối tượng JSON: {e}");
        process::exit(1);
    })
}

/// Nghiệp vụ, độc lập với socket: nhận đối tượng yêu cầu, trả đối tượng phản hồi.
pub fn process_request(req: &Value, dict: &Map<String, Value>) -> Value {
    let op = req.get("op").and_then(|v| v.as_str()); // req không phải đối tượng → None
    match op {
        Some("lookup") => match req.get("term").and_then(|v| v.as_str()) {
            Some(term) if dict.contains_key(term) => json!({"status": 200, "term": term, "text": dict[term]}),
            _ => json!({"status": 404, "error": "Không có thuật ngữ"}),
        },
        Some("list") => json!({"status": 200, "terms": dict.keys().collect::<Vec<_>>()}),
        _ => json!({"status": 400, "error": "Sai thao tác"}), // OTHERWISE
    }
}

fn handle_lookup(conn: &mut TcpStream, addr: &str, dict: &Map<String, Value>) -> Result<(), FrameError> {
    loop {
        let req = match recv_json(conn) {
            Ok(v) => v,
            Err(FrameError::ConnectionClosed { received: 0, .. }) => return Ok(()), // đóng tại ranh giới
            Err(FrameError::Json(_)) => {
                send_json(conn, &json!({"status": 400, "error": "JSON sai"}))?;
                println!("[server] {addr}: (JSON sai) -> 400");
                continue;
            }
            Err(e) => return Err(e), // đóng giữa khung, khung quá lớn, lỗi socket
        };
        if !req.is_object() {
            // IF NOT IS_OBJECT(req): JSON hợp lệ nhưng không phải đối tượng, vd. [1, 2] hoặc "abc"
            send_json(conn, &json!({"status": 400, "error": "JSON sai"}))?;
            println!("[server] {addr}: (không phải đối tượng JSON) -> 400");
            continue;
        }
        if req.get("op").and_then(|v| v.as_str()) == Some("bye") {
            return Ok(());
        }
        let resp = process_request(&req, dict);
        send_json(conn, &resp)?;
        // chỉ để ghi nhật ký
        let op = req.get("op").and_then(|v| v.as_str()).unwrap_or("?");
        let term = if op == "lookup" { format!(" '{}'", show(&req["term"])) } else { String::new() };
        println!("[server] {addr}: {op}{term} -> {}", resp["status"]);
    }
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9100);
    let dict = load_dict("glossary.json");
    // SOCKET(INET, STREAM) + SETOPT(REUSEADDR) + BIND + LISTEN (backlog do thư viện chuẩn chọn)
    let srv = match TcpListener::bind((host.as_str(), port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[server] Không mở được {host}:{port}: {e}");
            process::exit(1);
        }
    };
    println!("[server] Đang lắng nghe tại {host}:{port} ({} thuật ngữ)", dict.len());
    loop {
        // vòng chấp nhận kết nối
        let (mut conn, peer) = match srv.accept() {
            // ACCEPT
            Ok(v) => v,
            Err(e) => {
                println!("[server] Lỗi accept: {e}");
                continue;
            }
        };
        let addr = peer.to_string();
        println!("[server] Client kết nối: {addr}");
        if let Err(e) = handle_lookup(&mut conn, &addr, &dict) {
            // NetError, ConnectionClosed, ProtocolError: không dừng server
            println!("[server] Lỗi với {addr}: {e}");
        }
        drop(conn); // CLOSE(conn)
        println!("[server] Client {addr} đã đóng kết nối");
    }
}
