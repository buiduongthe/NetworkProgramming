//! Bài 6.6 – Phát và nhận thông báo dịch vụ qua multicast (Mã giả 6.8); Mục 6.4.3
//! Chạy:  cargo run -- announcer [service_port] [tên] [period_ms] [count] [mport]  (mặc định 9200 server-chat 5000 0 9998)
//!        cargo run -- listener [run_ms] [mport]                                   (mặc định 0 9998)
mod announcer;
mod listener;

use std::hash::{BuildHasher, Hasher};
use std::net::Ipv4Addr;

pub const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 42, 99); // CONST GROUP ← "239.255.42.99"
pub const MPORT: u16 = 9998; // CONST MPORT ← 9998

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
        "announcer" => announcer::run(rest),
        "listener" => listener::run(rest),
        _ => {
            println!("Cách dùng: cargo run -- announcer|listener [...]");
            std::process::exit(1);
        }
    }
}
