//! Echo Server dùng khung JSON (tiền tố độ dài 4 byte).
//!
//! Khung sườn ECHO_SERVER của Mã giả 3.1 (Mục 3.2.2), thay HANDLE_ECHO bằng HANDLE_FRAMES
//! dùng RECV_FRAME/SEND_JSON của Mã giả 3.6 (Mục 3.4.3). Mỗi yêu cầu {"text": ...} được
//! trả lời bằng {"status": 200, "text": ...}; yêu cầu sai nhận {"status": 400, "error": ...}.

use crate::framing::{json_decode, recv_frame, send_json, FrameError};
use crate::netutil::parse_endpoint;
use serde_json::json;
use std::net::{TcpListener, TcpStream};
use std::process;

/// Vòng phục vụ một client: mỗi khung nhận được là một thông điệp hoàn chỉnh.
fn handle_frames(conn: &mut TcpStream, addr: &str) -> Result<(), FrameError> {
    loop {
        // RECV_JSON tách hai bước (RECV_FRAME rồi JSON_DECODE) để ghi kích thước khung
        let payload = match recv_frame(conn) {
            Ok(p) => p,
            Err(FrameError::ConnectionClosed { received: 0, .. }) => return Ok(()), // đóng tại ranh giới khung
            Err(e) => return Err(e),                                                // đóng giữa khung: lỗi
        };
        println!("[server] Nhận khung {} byte từ {addr}", payload.len());
        let req = match json_decode(&payload) {
            Ok(v) => v,
            Err(_) => {
                send_json(conn, &json!({"status": 400, "error": "JSON sai"}))?;
                continue;
            }
        };
        match req.get("text").and_then(|t| t.as_str()) {
            Some(text) => send_json(conn, &json!({"status": 200, "text": text}))?,
            None => send_json(conn, &json!({"status": 400, "error": "Thiếu trường text"}))?,
        }
    }
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9000);
    // SOCKET + SETOPT(REUSEADDR) + BIND + LISTEN (backlog do thư viện chuẩn chọn)
    let srv = match TcpListener::bind((host.as_str(), port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[server] Không mở được {host}:{port}: {e}");
            process::exit(1);
        }
    };
    println!("[server] Đang lắng nghe tại {host}:{port}");
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
        if let Err(e) = handle_frames(&mut conn, &addr) {
            // NetError, ConnectionClosed, ProtocolError: không dừng server
            println!("[server] Lỗi với {addr}: {e}");
        }
        drop(conn); // CLOSE(conn)
        println!("[server] Client {addr} đã đóng kết nối");
    }
}
