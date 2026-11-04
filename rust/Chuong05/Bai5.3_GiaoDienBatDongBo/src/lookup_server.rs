//! Server tra cứu tối giản phục vụ ứng dụng có giao diện.
//!
//! Dùng lại server của Bài 5.1 (nghiệp vụ Mã giả 3.7, khung JSON Mã giả 5.1), thêm độ trễ giả lập
//! delay_ms trước mỗi phản hồi để quan sát giao diện không bị đóng băng khi server trả lời chậm.

use crate::khung_json_async::{json_bytes, parse_endpoint, recv_json_async, send_json_async, CancelToken, FrameError};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::process;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, Duration};

/// Từ điển nhỏ thay cho glossary.json.
const DICT: [(&str, &str); 4] = [
    ("TCP", "Giao thức truyền tin cậy, hướng kết nối"),
    ("UDP", "Giao thức datagram, không kết nối"),
    ("Socket", "Điểm cuối truyền thông của một chương trình"),
    ("Bất đồng bộ", "Khởi động vào/ra rồi làm việc khác, quay lại khi xong"),
];

/// Mã giả 3.7: nghiệp vụ, độc lập với socket.
fn process_request(req: &Value) -> Value {
    match req.get("op").and_then(|v| v.as_str()) {
        Some("lookup") => {
            let term = req.get("term").and_then(|v| v.as_str()).unwrap_or("");
            match DICT.iter().find(|(k, _)| *k == term) {
                Some((k, text)) => json!({"status": 200, "term": k, "text": text}),
                None => json!({"status": 404, "error": "Không có thuật ngữ"}),
            }
        }
        Some("list") => json!({"status": 200, "terms": DICT.iter().map(|(k, _)| *k).collect::<Vec<_>>()}),
        _ => json!({"status": 400, "error": "Sai thao tác"}),
    }
}

/// HANDLE_LOOKUP, phiên bản bất đồng bộ.
async fn handle_lookup(mut conn: TcpStream, peer: SocketAddr, delay_ms: u64) {
    let addr = peer.to_string();
    println!("[server] Client kết nối: {addr}");
    let ct = CancelToken::new(); // tín hiệu hủy của phiên (không bị hủy trong bài này)
    let result: Result<(), FrameError> = async {
        loop {
            let req = match recv_json_async(&mut conn, &ct).await {
                // AWAIT: chờ khung mà không chặn luồng
                Ok(v) => v,
                Err(FrameError::ConnectionClosed(0)) => break, // đóng tại ranh giới khung: bình thường
                Err(FrameError::Json(_)) => {
                    send_json_async(&mut conn, &json!({"status": 400, "error": "JSON sai"}), &ct).await?;
                    continue;
                }
                Err(e) => return Err(e), // đóng giữa khung: lỗi
            };
            println!("[server] Nhận từ {addr}: {}", String::from_utf8_lossy(&json_bytes(&req)));
            if req.get("op").and_then(|v| v.as_str()) == Some("bye") {
                break;
            }
            sleep(Duration::from_millis(delay_ms)).await; // độ trễ giả lập của server
            send_json_async(&mut conn, &process_request(&req), &ct).await?;
        }
        Ok(())
    }
    .await;
    if let Err(e) = result {
        println!("[server] Lỗi với {addr}: {e}"); // lỗi một client không dừng server
    }
    drop(conn);
    println!("[server] Client {addr} đã đóng kết nối");
}

pub async fn run(args: &[String]) {
    let (host, port) = parse_endpoint(&args[..args.len().min(2)], "0.0.0.0", 9100);
    let delay_ms: u64 = args.get(2).map(|t| t.trim().parse().unwrap_or(0)).unwrap_or(0);
    let srv = match TcpListener::bind((host.as_str(), port)).await {
        // BIND + LISTEN
        Ok(l) => l,
        Err(e) => {
            eprintln!("[server] Không mở được {host}:{port}: {e}");
            process::exit(1);
        }
    };
    println!("[server] Đang lắng nghe tại {host}:{port}, độ trễ giả lập {delay_ms} ms");
    loop {
        tokio::select! {
            r = srv.accept() => match r {
                // mỗi kết nối mới: một tác vụ handle_lookup chạy song song
                Ok((conn, peer)) => { tokio::spawn(handle_lookup(conn, peer, delay_ms)); }
                Err(e) => println!("[server] Lỗi accept: {e}"),
            },
            _ = tokio::signal::ctrl_c() => {
                println!("[server] Dừng server");
                return;
            }
        }
    }
}
