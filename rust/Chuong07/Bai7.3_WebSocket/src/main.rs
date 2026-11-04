//! Bài 7.3 – Server WebSocket tối giản và client thử (Mã giả 7.3, 7.4; Mục 7.3.2)
//! Chạy:  cargo run -- server  [host] [port]   (mặc định 0.0.0.0 8080)
//!        cargo run -- client  [host] [port]   (client tự viết; đọc dòng từ bàn phím)
//!        cargo run -- kiemthu [host] [port]   (khung tự tạo, kể cả khung vi phạm đặc tả)
mod ws_client;
mod ws_kiem_thu;
mod ws_server;

use base64::Engine;
use sha1::{Digest, Sha1};
use std::hash::{BuildHasher, Hasher};

/// GUID cố định của RFC 6455, ghép sau khóa của client khi tính Sec-WebSocket-Accept.
pub const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Sec-WebSocket-Accept = BASE64(SHA1(key + WS_GUID)).
pub fn accept_key(key: &str) -> String {
    let digest = Sha1::digest(format!("{key}{WS_GUID}").as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// n byte ngẫu nhiên chỉ dùng thư viện chuẩn: mỗi RandomState mang khóa băm ngẫu nhiên.
pub fn random_bytes(n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while out.len() < n {
        let r = std::collections::hash_map::RandomState::new().build_hasher().finish();
        out.extend_from_slice(&r.to_le_bytes());
    }
    out.truncate(n);
    out
}

/// In thông báo lỗi rồi thoát chương trình.
pub fn exit_with(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

/// Đọc tham số cổng thứ i (nếu có), kiểm tra 1–65535.
pub fn port_arg(args: &[String], i: usize, default: u16) -> u16 {
    let Some(s) = args.get(i) else { return default };
    let p: i64 = s.parse().unwrap_or_else(|_| exit_with(&format!("Cổng phải là số nguyên: {s}")));
    if !(1..=65535).contains(&p) {
        exit_with(&format!("Cổng không hợp lệ: {p}"));
    }
    p as u16
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        // server chạy trên môi trường bất đồng bộ của tokio
        Some("server") => tokio::runtime::Runtime::new().unwrap().block_on(ws_server::run(rest)),
        // client và chương trình kiểm thử dùng socket chặn của thư viện chuẩn
        Some("client") => ws_client::run(rest),
        Some("kiemthu") => ws_kiem_thu::run(rest),
        _ => {
            println!("Cách dùng: cargo run -- server|client|kiemthu [host] [port]");
            std::process::exit(1);
        }
    }
}
