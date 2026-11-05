//! Bài mở rộng MR3.2 – Tự động hóa cấu hình thiết bị qua CLI (kiểu expect/Netmiko, chỉ dùng socket).
//!
//! Chạy (một chương trình, hai vai trò):
//!   cargo run -q --release -- thietbi [host] [port]                   thiết bị mô phỏng R1
//!   cargo run -q --release -- tudong <host> <port> <muc_tieu.json> [--thu]   client tự động hóa
//! Tài khoản lấy từ biến môi trường DEVICE_USER, DEVICE_PASSWORD (không ghi cứng trong mã).

mod ipmask;
mod thiet_bi;
mod tu_dong;

use std::process;

/// Biểu diễn chuỗi kiểu repr() của Python ('...') để thông điệp giống bản Python.
pub fn repr(s: &str) -> String {
    let mut out = String::from("'");
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("thietbi") => thiet_bi::run(rest),
        Some("tudong") => tu_dong::run(rest),
        _ => {
            eprintln!("Cách dùng: mr3_2_tu_dong_hoa_cli thietbi [host] [port] | tudong <host> <port> <muc_tieu.json> [--thu]");
            process::exit(2);
        }
    }
}
