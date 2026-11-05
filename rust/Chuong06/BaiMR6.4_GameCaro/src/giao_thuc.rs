//! Giao thức trò chơi Caro: khung JSON có tiền tố độ dài (chép từ MR3.3, Bài 3.2).
//!
//! Khung = 4 byte độ dài (big-endian) + nội dung JSON mã hóa UTF-8. Mỗi thông điệp có trường "type":
//!   client -> server: hello {name}, list, create {room}, join {room}, watch {room}, move {r, c},
//!                     rematch, leave, bye
//!   server -> client: welcome, rooms, joined, start, board, moved, over, rematch_wait, left,
//!                     room_closed, error {code, msg}
//! Khám phá server trong LAN (UDP broadcast, như Bài 6.5):
//!   client phát {"type": "discover", "service": "caro", "id": n}
//!   server trả unicast {"id": n, "name", "port", "rooms", "players"}

use serde_json::{json, Value};
use std::io::{self, ErrorKind, Read, Write};
use std::net::{TcpStream, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_FRAME: usize = 64 * 1024; // thông điệp trò chơi rất nhỏ: giới hạn 64 KiB chống khung khổng lồ
pub const DISC_SERVICE: &str = "caro";

/// Mã giả 3.6 (SEND_FRAME): 4 byte độ dài + nội dung, gửi hết bằng write_all (SEND_ALL).
pub fn send_json(mut s: &TcpStream, obj: &Value) -> io::Result<()> {
    let p = obj.to_string().into_bytes();
    let mut f = (p.len() as u32).to_be_bytes().to_vec();
    f.extend_from_slice(&p);
    s.write_all(&f)
}

/// Mã giả 3.6 (RECV_FRAME): đọc tiêu đề, kiểm tra giới hạn, đọc đúng n byte (RECV_EXACT = read_exact).
pub fn recv_frame(mut s: &TcpStream) -> io::Result<Vec<u8>> {
    let mut h = [0u8; 4];
    s.read_exact(&mut h)?;
    let n = u32::from_be_bytes(h) as usize;
    if n > MAX_FRAME {
        return Err(io::Error::new(ErrorKind::InvalidData, format!("Khung quá lớn: {n} byte")));
    }
    let mut b = vec![0u8; n];
    s.read_exact(&mut b)?;
    Ok(b)
}

/// Giải mã nội dung khung; trả None nếu không phải đối tượng JSON hợp lệ.
pub fn decode_json(payload: &[u8]) -> Option<Value> {
    serde_json::from_slice::<Value>(payload).ok().filter(Value::is_object)
}

pub fn recv_json(s: &TcpStream) -> io::Result<Option<Value>> {
    Ok(decode_json(&recv_frame(s)?))
}

/// Chuỗi của một trường (None nếu thiếu/null).
pub fn chuoi(v: &Value) -> Option<String> {
    v.as_str().map(String::from)
}

/// Biểu diễn giá trị kiểu repr() của Python, để thông điệp giống bản Python.
pub fn repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::String(t) => format!("'{}'", t.replace('\\', "\\\\").replace('\'', "\\'")),
        other => other.to_string(),
    }
}

/// Một server tìm thấy.
pub struct Found {
    pub ip: String,
    pub port: u64,
    pub name: String,
    pub rooms: u64,
    pub players: u64,
}

/// Mã giả 6.7 (DISCOVER, Bài 6.5): phát một yêu cầu broadcast, thu thập MỌI phản hồi đến hết thời gian chờ.
/// Trả danh sách server, sắp theo tên để kết quả ổn định.
pub fn discover(disc_port: u16, wait_ms: u64, bcast: &str) -> Vec<Found> {
    let mut found = Vec::new();
    let Ok(s) = UdpSocket::bind("0.0.0.0:0") else { return found };
    s.set_broadcast(true).ok(); // bắt buộc để gửi tới địa chỉ broadcast
    let qid = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() % 999_999 + 1).unwrap_or(1);
    let q = json!({"type": "discover", "service": DISC_SERVICE, "id": qid});
    if s.send_to(q.to_string().as_bytes(), (bcast, disc_port)).is_err() {
        return found;
    }
    let deadline = Instant::now() + Duration::from_millis(wait_ms);
    let mut buf = [0u8; 2048];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        s.set_read_timeout(Some(left)).ok();
        let Ok((n, addr)) = s.recv_from(&mut buf) else { break };
        if let Some(r) = decode_json(&buf[..n]).filter(|r| r["id"] == qid) {
            // chỉ nhận phản hồi đúng yêu cầu của mình
            found.push(Found {
                ip: addr.ip().to_string(),
                port: r["port"].as_u64().unwrap_or(0),
                name: chuoi(&r["name"]).unwrap_or_default(),
                rooms: r["rooms"].as_u64().unwrap_or(0),
                players: r["players"].as_u64().unwrap_or(0),
            });
        }
    }
    found.sort_by(|a, b| (&a.name, &a.ip, a.port.to_string()).cmp(&(&b.name, &b.ip, b.port.to_string())));
    found
}
