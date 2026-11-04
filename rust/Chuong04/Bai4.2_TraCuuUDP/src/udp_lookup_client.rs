//! Client tra cứu thuật ngữ trên UDP, gửi lại có lùi thời gian chờ.
//!
//! Cài đặt Mã giả 4.4 UDP_REQUEST (Mục 4.3.1, Giáo trình Lập trình mạng).
//! Kế thừa: Mã giả 4.2 (client UDP có thời gian chờ), vòng đọc thuật ngữ của client tra cứu TCP.
//! Mỗi yêu cầu mang id; client gửi lại tối đa max_tries lần, thời gian chờ nhân đôi sau mỗi lần,
//! bỏ qua datagram không đến từ server hoặc không khớp id.

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

/// Mã giả 4.4 – UDP_REQUEST; các dòng in "[client]   ..." là nhật ký minh họa, không có trong mã giả.
fn udp_request(s: &UdpSocket, server: SocketAddr, mut req: Value, timeout_ms: u64, max_tries: u32) -> Result<Value, String> {
    let id = next_id();
    req["id"] = json!(id); // dòng 2: req.id ← NEXT_ID()
    let payload = req.to_string().into_bytes(); // dòng 3: ENCODE(JSON_ENCODE(req), "UTF-8")
    let mut timeout_ms = timeout_ms;
    let mut buf = vec![0u8; 65535];
    for attempt in 1..=max_tries {
        // dòng 4
        s.send_to(&payload, server).map_err(|e| e.to_string())?; // dòng 5
        println!("[client]   Lần {attempt}: gửi id={id} ({} byte), chờ tối đa {timeout_ms} ms", payload.len());
        let deadline = Instant::now() + Duration::from_millis(timeout_ms); // dòng 6
        while Instant::now() < deadline {
            // dòng 7
            let remaining = deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
            s.set_read_timeout(Some(remaining)).ok(); // dòng 8: phần thời gian còn lại
            let (n, addr) = match s.recv_from(&mut buf) {
                // dòng 10
                Ok(v) => v,
                // dòng 12–13: CATCH Timeout, ConnectionReset → BREAK
                // (Timeout: Linux báo WouldBlock, Windows báo TimedOut; ConnectionReset: Windows báo ICMP port unreachable)
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::ConnectionReset) => break,
                Err(e) => return Err(e.to_string()),
            };
            // dòng 11: JSON_DECODE; dòng 14–15: không phải JSON thì bỏ
            let Some(resp) = std::str::from_utf8(&buf[..n]).ok().and_then(|t| serde_json::from_str::<Value>(t).ok()) else {
                continue;
            };
            if addr == server && resp.is_object() && resp["id"] == json!(id) {
                return Ok(resp); // dòng 17
            }
            println!("[client]   Bỏ datagram lạ hoặc muộn từ {addr}");
        }
        println!("[client]   Hết {timeout_ms} ms không có phản hồi");
        timeout_ms *= 2; // dòng 19: lùi theo cấp số nhân
    }
    Err(format!("Không có phản hồi sau {max_tries} lần")) // dòng 21: RAISE NetError
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "127.0.0.1", 9100);
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
    println!(
        "[client] Gửi yêu cầu tới {host}:{port} qua UDP (chờ 500 ms, tối đa 4 lần). Nhập thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc)."
    );
    for line in io::stdin().lock().lines() {
        // READ_CONSOLE()
        let Ok(line) = line else { break };
        let term = line.trim();
        if term.is_empty() {
            continue;
        }
        println!("[client] Tra cứu '{term}'");
        let t0 = Instant::now();
        let result = udp_request(&s, server, json!({"op": "lookup", "term": term}), 500, 4);
        let ms = t0.elapsed().as_millis();
        match result {
            Err(e) => println!("[client] Lỗi: {e} ({ms} ms)"),
            Ok(resp) if resp["status"] == 200 => {
                println!("[client] {}: {} ({ms} ms)", show(&resp["term"]), show(&resp["text"]))
            }
            Ok(resp) => println!("[client] Lỗi {}: {} ({ms} ms)", resp["status"], show(&resp["error"])),
        }
    }
    drop(s);
    println!("[client] Đã đóng socket");
}
