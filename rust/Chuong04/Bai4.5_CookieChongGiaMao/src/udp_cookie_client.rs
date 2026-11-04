//! Client tra cứu UDP có xử lý mã thách thức (cookie) của server.
//!
//! Theo đoạn văn sau Mã giả 4.8 (Mục 4.6.1): khi nhận phản hồi 401 kèm cookie, client lưu mã và gửi lại
//! yêu cầu có trường cookie; các yêu cầu sau dùng lại mã đó. Kế thừa: Mã giả 4.4 UDP_REQUEST.
//! Tham số thứ ba (thêm cho bài thực hành): cookie ban đầu, mô phỏng client dùng cookie giả mạo
//! hoặc cookie của địa chỉ khác.

use crate::endpoint::parse_endpoint;
use serde_json::{json, Value};
use std::io::{self, BufRead, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static LAST_ID: AtomicU64 = AtomicU64::new(0);

/// NEXT_ID(): mã định danh tăng dần 1, 2, 3...
fn next_id() -> u64 {
    LAST_ID.fetch_add(1, Ordering::Relaxed) + 1
}

fn show(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Mã giả 4.4 – UDP_REQUEST (chép từ Bài 4.2), thêm tham số min_len để đệm yêu cầu;
/// dòng "[client]   ..." là nhật ký minh họa.
fn udp_request(s: &UdpSocket, server: SocketAddr, mut req: Value, timeout_ms: u64, max_tries: u32, min_len: usize) -> Result<Value, String> {
    let id = next_id();
    req["id"] = json!(id);
    let mut payload = req.to_string().into_bytes();
    if payload.len() < min_len {
        payload.resize(min_len, b' '); // khoảng trắng sau JSON vẫn là JSON hợp lệ
    }
    let mut timeout_ms = timeout_ms;
    let mut buf = vec![0u8; 65535];
    for attempt in 1..=max_tries {
        s.send_to(&payload, server).map_err(|e| e.to_string())?;
        println!("[client]   Lần {attempt}: gửi id={id} ({} byte), chờ tối đa {timeout_ms} ms", payload.len());
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
            s.set_read_timeout(Some(remaining)).ok();
            let (n, addr) = match s.recv_from(&mut buf) {
                Ok(v) => v,
                // CATCH Timeout, ConnectionReset → BREAK (Windows báo ICMP port unreachable bằng ConnectionReset)
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::ConnectionReset) => break,
                Err(e) => return Err(e.to_string()),
            };
            let Some(resp) = std::str::from_utf8(&buf[..n]).ok().and_then(|t| serde_json::from_str::<Value>(t).ok()) else {
                continue; // không phải JSON: bỏ
            };
            if addr == server && resp.is_object() && resp["id"] == json!(id) {
                return Ok(resp);
            }
            println!("[client]   Bỏ datagram lạ hoặc muộn từ {addr}");
        }
        println!("[client]   Hết {timeout_ms} ms không có phản hồi");
        timeout_ms *= 2;
    }
    Err(format!("Không có phản hồi sau {max_tries} lần"))
}

/// Kích thước tối thiểu của yêu cầu chưa có cookie: 3 × 40 = 120 byte ≥ kích thước thông điệp thách thức
/// (khoảng 100 byte), để server được phép gửi thách thức (tương tự gói Initial ≥ 1200 byte của QUIC).
const MIN_FIRST_REQ: usize = 40;

fn lookup(s: &UdpSocket, server: SocketAddr, term: &str, cookie: &Option<String>) -> Result<Value, String> {
    let mut req = json!({"op": "lookup", "term": term});
    let pad = match cookie {
        Some(c) => {
            req["cookie"] = json!(c); // gửi kèm cookie đã lưu
            0
        }
        None => MIN_FIRST_REQ, // yêu cầu đầu tiên: đệm thêm khoảng trắng cho đủ lớn
    };
    udp_request(s, server, req, 500, 4, pad)
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(&args[..args.len().min(2)], "127.0.0.1", 9100);
    let mut cookie: Option<String> = args.get(2).cloned();
    let server: SocketAddr = match (host.as_str(), port).to_socket_addrs().map(|mut it| it.next()) {
        Ok(Some(a)) => a,
        _ => {
            eprintln!("[client] Không phân giải được {host}");
            process::exit(1);
        }
    };
    let s = UdpSocket::bind(if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).unwrap_or_else(|e| {
        eprintln!("[client] Không tạo được socket: {e}");
        process::exit(1);
    });
    println!("[client] Gửi yêu cầu tới {host}:{port} qua UDP. Nhập thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc).");
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let term = line.trim();
        if term.is_empty() {
            continue;
        }
        let with = if cookie.is_some() { "kèm cookie" } else { "không kèm cookie" };
        println!("[client] Tra cứu '{term}' ({with})");
        let result = lookup(&s, server, term, &cookie).and_then(|resp| {
            match (resp["status"].as_u64(), resp["cookie"].as_str()) {
                (Some(401), Some(c)) => {
                    // lưu mã thách thức rồi gửi lại đúng một lần
                    println!("[client] Nhận thách thức 401, lưu cookie {}…; gửi lại kèm cookie", &c[..c.len().min(16)]);
                    cookie = Some(c.to_string());
                    lookup(&s, server, term, &cookie)
                }
                _ => Ok(resp),
            }
        });
        match result {
            Err(e) => println!("[client] Lỗi: {e}"),
            Ok(resp) if resp["status"] == 200 => println!("[client] {}: {}", show(&resp["term"]), show(&resp["text"])),
            Ok(resp) => {
                let err = resp.get("error").map(show).unwrap_or_else(|| "không được phục vụ".to_string());
                println!("[client] Lỗi {}: {err}", resp["status"]);
            }
        }
    }
    drop(s);
    println!("[client] Đã đóng socket");
}
