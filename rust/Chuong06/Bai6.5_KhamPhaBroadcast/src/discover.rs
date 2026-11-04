//! Bài 6.5 – Khám phá server chat trong LAN bằng broadcast: phía client (DISCOVER).
//!
//! Cài đặt Mã giả 6.7, hàm DISCOVER (Mục 6.4.2, Giáo trình Lập trình mạng). Kế thừa Mã giả 4.4
//! (UDP_REQUEST): gửi một yêu cầu nhưng thu thập NHIỀU phản hồi đến hết thời gian chờ.
//!
//! Chạy:  cargo run -- discover [wait_ms] [disc_port] [địa_chỉ_broadcast]
//!        (mặc định 2000 9999 255.255.255.255)
use crate::{int_arg, port_arg, random_u64, DISC_PORT};
use serde_json::{json, Value};
use std::io::ErrorKind;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

/// Trả về danh sách (ip, cổng dịch vụ, tên) của các server đã trả lời.
fn discover(wait_ms: u64, disc_port: u16, bcast: &str) -> std::io::Result<Vec<(String, Value, Value)>> {
    let s = UdpSocket::bind("0.0.0.0:0")?; // SOCKET(INET, DGRAM), cổng tạm
    s.set_broadcast(true)?; // SETOPT(BROADCAST): bắt buộc
    let id = random_u64() % 999999 + 1; // NEXT_ID(): số ngẫu nhiên 1–999 999
    let q = json!({"type": "discover", "service": "chat", "id": id});
    s.send_to(q.to_string().as_bytes(), (bcast, disc_port))?; // SENDTO
    println!("[client] Phát discover id={id} tới {bcast}:{disc_port}");
    println!("[client] Thu thập phản hồi trong {wait_ms} ms");
    let mut found = Vec::new();
    let deadline = Instant::now() + Duration::from_millis(wait_ms);
    while Instant::now() < deadline {
        // MAX(1, …): thời gian chờ 0 không hợp lệ (ở nhiều nền tảng nghĩa là chờ vô hạn)
        let left = deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
        s.set_read_timeout(Some(left))?; // SETOPT(RECV_TIMEOUT, …)
        let mut buf = [0u8; 2048];
        let (n, addr) = match s.recv_from(&mut buf) {
            Ok(x) => x, // RECVFROM(s, 2048)
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => break, // Timeout
            Err(e) => return Err(e),
        };
        let Ok(r) = serde_json::from_slice::<Value>(&buf[..n]) else { continue }; // JsonError: sai định dạng, bỏ
        if r["id"] == id {
            found.push((addr.ip().to_string(), r["port"].clone(), r["name"].clone()));
        }
    }
    Ok(found) // CLOSE(s) khi s ra khỏi phạm vi
}

pub fn run(args: &[String]) {
    let wait_ms = int_arg(args, 0, 2000);
    let disc_port = port_arg(args, 1, DISC_PORT);
    let bcast = args.get(2).map(String::as_str).unwrap_or("255.255.255.255");
    match discover(wait_ms, disc_port, bcast) {
        Ok(found) => {
            println!("[client] Tìm thấy {} server:", found.len());
            for (ip, port, name) in found {
                println!("[client]   {} tại {ip}:{port}", name.as_str().unwrap_or(""));
            }
        }
        Err(e) => println!("[client] Lỗi: {e}"),
    }
}
