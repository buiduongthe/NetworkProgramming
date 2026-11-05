//! Bài mở rộng MR3.1 – Bộ chuyển tiếp cổng TCP (TCP port forwarder).
//!
//! Bài mở rộng, không có trong giáo trình; vận dụng Chương 3 (vòng đời socket TCP, SEND_ALL,
//! SHUTDOWN một chiều) và Chương 6 (mỗi kết nối một luồng).
//!
//! Chạy (một chương trình, ba vai trò):
//!   cargo run -q --release -- dich [host] [port]                                  server đích "chữ hoa"
//!   cargo run -q --release -- chuyentiep <host_nghe> <cổng_nghe> <host_đích> <cổng_đích>
//!   cargo run -q --release -- client <host> <port> <chuỗi> [số_lần_lặp]

mod chuyen_tiep;
mod client_thu;
mod server_dich;

use std::process;

/// Thông báo lỗi hệ điều hành gọn như e.strerror của Python: bỏ phần " (os error N)".
pub fn loi(e: &std::io::Error) -> String {
    let s = e.to_string();
    match s.find(" (os error") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// Đọc số cổng; sai thì báo lỗi và thoát.
pub fn port_arg(s: &str) -> u16 {
    s.parse().unwrap_or_else(|_| {
        eprintln!("Cổng không hợp lệ: {s}");
        process::exit(2)
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("dich") => server_dich::run(rest),
        Some("chuyentiep") => chuyen_tiep::run(rest),
        Some("client") => client_thu::run(rest),
        _ => {
            eprintln!("Cách dùng: mr3_1_chuyen_tiep_cong dich|chuyentiep|client ...");
            process::exit(2);
        }
    }
}
