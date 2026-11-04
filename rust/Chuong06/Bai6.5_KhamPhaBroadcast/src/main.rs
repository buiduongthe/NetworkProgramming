//! Bài 6.5 – Khám phá server trong LAN bằng broadcast (Mã giả 6.7); Mục 6.4.2
//! Chạy:  cargo run -- responder [service_port] [tên] [disc_port]        (mặc định 9200 server-chat 9999)
//!        cargo run -- discover [wait_ms] [disc_port] [địa_chỉ_broadcast] (mặc định 2000 9999 255.255.255.255)
mod discover;
mod responder;

use std::hash::{BuildHasher, Hasher};

pub const DISC_PORT: u16 = 9999; // CONST DISC_PORT ← 9999

/// Số ngẫu nhiên 64 bit chỉ dùng thư viện chuẩn: mỗi RandomState mang khóa băm ngẫu nhiên.
pub fn random_u64() -> u64 {
    std::collections::hash_map::RandomState::new().build_hasher().finish()
}

/// In thông báo lỗi rồi thoát chương trình.
pub fn exit_with(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

/// Đọc tham số nguyên thứ i (nếu có), ngược lại dùng giá trị mặc định.
pub fn int_arg(args: &[String], i: usize, default: u64) -> u64 {
    match args.get(i) {
        Some(s) => s.parse().unwrap_or_else(|_| exit_with(&format!("Tham số phải là số nguyên: {s}"))),
        None => default,
    }
}

/// Đọc tham số cổng thứ i, kiểm tra 1–65535.
pub fn port_arg(args: &[String], i: usize, default: u16) -> u16 {
    let p = int_arg(args, i, default as u64);
    if !(1..=65535).contains(&p) {
        exit_with(&format!("Cổng không hợp lệ: {p}"));
    }
    p as u16
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    match mode {
        "responder" => responder::run(rest),
        "discover" => discover::run(rest),
        _ => {
            println!("Cách dùng: cargo run -- responder|discover [...]");
            std::process::exit(1);
        }
    }
}
