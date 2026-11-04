//! Server tra cứu và nhận tệp có kiểm tra mã băm SHA-256.
//!
//! Cài đặt Mã giả 3.9 (Mục 3.5.2, Giáo trình Lập trình mạng): HANDLE_PUT được gắn vào HANDLE_LOOKUP
//! của Mã giả 3.7 bằng nhánh  IF req.op = "put" THEN HANDLE_PUT(conn, req); CONTINUE.
//! Ba biện pháp an toàn: BASENAME chặn "../"; giới hạn MAX_FILE; ghi tệp tạm .part, chỉ đổi tên khi mã băm khớp.

use crate::framing::{recv_json, send_json, show, FrameError};
use crate::netutil::parse_endpoint;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process;

const MAX_FILE: u64 = 100 * 1_048_576;
const UPLOAD_DIR: &str = "uploads";

/// DICT ← LOAD_JSON_FILE("glossary.json").
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

/// Mã giả 3.7 – nghiệp vụ tra cứu, độc lập với socket.
fn process_request(req: &Value, dict: &Map<String, Value>) -> Value {
    let op = req.get("op").and_then(|v| v.as_str());
    match op {
        Some("lookup") => match req.get("term").and_then(|v| v.as_str()) {
            Some(term) if dict.contains_key(term) => json!({"status": 200, "term": term, "text": dict[term]}),
            _ => json!({"status": 404, "error": "Không có thuật ngữ"}),
        },
        Some("list") => json!({"status": 200, "terms": dict.keys().collect::<Vec<_>>()}),
        _ => json!({"status": 400, "error": "Sai thao tác"}),
    }
}

/// BASENAME: bỏ mọi thành phần thư mục, tách theo cả '/' và '\' để an toàn trên mọi hệ điều hành.
fn basename(name: &str) -> String {
    name.replace('\\', "/").rsplit('/').next().unwrap_or("").to_string()
}

/// IS_HEX(text, n): đúng n ký tự, mỗi ký tự là chữ số hex (0–9, a–f, A–F).
fn is_hex(text: &str, n: usize) -> bool {
    text.len() == n && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// Mã giả 3.9 – req = {op, name, size, sha256}; trả chuỗi mô tả kết quả để ghi nhật ký.
fn handle_put(conn: &mut TcpStream, req: &Value) -> Result<String, FrameError> {
    // name ← BASENAME(REPLACE(req.name, "\\", "/")): bỏ thư mục, chặn "../" và "..\\"
    let name = basename(req.get("name").and_then(|v| v.as_str()).unwrap_or(""));
    // IS_INTEGER(req.size): as_u64 chỉ nhận số nguyên ≥ 0 (không nhận số thực, chuỗi, true/false)
    let size = req.get("size").and_then(|v| v.as_u64());
    let sha = req.get("sha256").and_then(|v| v.as_str()).filter(|h| is_hex(h, 64)); // IS_HEX(req.sha256, 64)
    let (size, sha) = match (size, sha) {
        (Some(size), Some(sha)) if !["", ".", ".."].contains(&name.as_str()) && size <= MAX_FILE => (size, sha),
        _ => {
            send_json(conn, &json!({"status": 400, "error": "Sai tham số"}))?;
            return Ok("400 (Sai tham số)".to_string());
        }
    };
    send_json(conn, &json!({"status": 100}))?; // sẵn sàng nhận nội dung
    let tmp = Path::new(UPLOAD_DIR).join(format!("{name}.part"));
    let mut f = File::create(&tmp)?; // OPEN_FILE(tmp, WRITE)
    let mut h = Sha256::new(); // SHA256_INIT()
    let mut remaining = size;
    let mut buf = vec![0u8; 65536];
    let result: Result<(), FrameError> = (|| {
        while remaining > 0 {
            let want = remaining.min(65536) as usize;
            let n = conn.read(&mut buf[..want])?;
            if n == 0 {
                let got = size - remaining;
                let message = format!("Tệp chưa nhận đủ ({got}/{size} byte)");
                return Err(FrameError::ConnectionClosed { received: got as usize, message });
            }
            f.write_all(&buf[..n])?;
            h.update(&buf[..n]); // SHA256_UPDATE
            remaining -= n as u64;
        }
        Ok(())
    })();
    drop(f); // CLOSE_FILE
    if let Err(e) = result {
        // CATCH ANY AS e: mọi lỗi (kể cả RST, hết thời gian chờ) – xóa tệp dở dang rồi RAISE e
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    let digest = format!("{:x}", h.finalize()); // HEX(SHA256_FINAL(h))
    if digest == sha {
        fs::rename(&tmp, Path::new(UPLOAD_DIR).join(&name))?; // RENAME (ghi đè nếu đã có, cả trên Windows)
        send_json(conn, &json!({"status": 200, "saved": name}))?;
        return Ok(format!("200, lưu {UPLOAD_DIR}/{name} ({size} byte)"));
    }
    let _ = fs::remove_file(&tmp);
    send_json(conn, &json!({"status": 422, "error": "Mã băm không khớp"}))?;
    Ok("422 (Mã băm không khớp)".to_string())
}

/// Mã giả 3.7 – HANDLE_LOOKUP, bổ sung nhánh "put" theo Mã giả 3.9.
fn handle_lookup(conn: &mut TcpStream, addr: &str, dict: &Map<String, Value>) -> Result<(), FrameError> {
    loop {
        let req = match recv_json(conn) {
            Ok(v) => v,
            Err(FrameError::ConnectionClosed { received: 0, .. }) => return Ok(()), // đóng tại ranh giới
            Err(FrameError::Json(_)) => {
                send_json(conn, &json!({"status": 400, "error": "JSON sai"}))?;
                continue;
            }
            Err(e) => return Err(e), // đóng giữa khung: lỗi
        };
        if !req.is_object() {
            // IF NOT IS_OBJECT(req): JSON hợp lệ nhưng không phải đối tượng
            send_json(conn, &json!({"status": 400, "error": "JSON sai"}))?;
            continue;
        }
        let op = req.get("op").and_then(|v| v.as_str());
        if op == Some("bye") {
            return Ok(());
        }
        if op == Some("put") {
            let result = handle_put(conn, &req)?;
            println!("[server] {addr}: put '{}' -> {result}", show(&req["name"]));
            continue;
        }
        let resp = process_request(&req, dict);
        send_json(conn, &resp)?;
        let op = op.unwrap_or("?"); // chỉ để ghi nhật ký
        let term = if op == "lookup" { format!(" '{}'", show(&req["term"])) } else { String::new() };
        println!("[server] {addr}: {op}{term} -> {}", resp["status"]);
    }
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9100);
    let dict = load_dict("glossary.json");
    if let Err(e) = fs::create_dir_all(UPLOAD_DIR) {
        eprintln!("[server] Không tạo được thư mục {UPLOAD_DIR}/: {e}");
        process::exit(1);
    }
    // SOCKET + SETOPT(REUSEADDR) + BIND + LISTEN (backlog do thư viện chuẩn chọn)
    let srv = match TcpListener::bind((host.as_str(), port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[server] Không mở được {host}:{port}: {e}");
            process::exit(1);
        }
    };
    println!("[server] Đang lắng nghe tại {host}:{port}, thư mục tải lên: {UPLOAD_DIR}/");
    loop {
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
            println!("[server] Lỗi với {addr}: {e}"); // không dừng server
        }
        drop(conn);
        println!("[server] Client {addr} đã đóng kết nối");
    }
}
