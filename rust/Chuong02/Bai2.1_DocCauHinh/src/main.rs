//! Bài 2.1 – Đọc địa chỉ và cổng từ tham số dòng lệnh, biến môi trường.
//!
//! Cài đặt Mã giả 2.1 PARSE_ENDPOINT (Mục 2.1.3, Giáo trình Lập trình mạng).
//! Thứ tự ưu tiên: tham số dòng lệnh > biến môi trường APP_HOST, APP_PORT > giá trị mặc định.
//! Chương trình chỉ in cấu hình nhận được, không mở socket.
//!
//! Chạy:  cargo run -q --release -- [host] [port]        (mặc định 0.0.0.0 9000)
//! Ví dụ: APP_PORT=9100 cargo run -q --release            (Linux/macOS)
//!        $env:APP_PORT="9100"; cargo run -q --release     (Windows PowerShell)

use std::env;
use std::fmt;
use std::process;

/// Lỗi cấu hình: giá trị địa chỉ hoặc cổng không dùng được.
#[derive(Debug)]
struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Mã giả 2.1 – PARSE_ENDPOINT(args, default_host, default_port).
fn parse_endpoint(args: &[String], default_host: &str, default_port: u16) -> Result<(String, u16), ConfigError> {
    let mut host = default_host.to_string(); // dòng 2
    let mut port = default_port.to_string(); // dòng 3
    if let Ok(v) = env::var("APP_HOST") {
        host = v; // dòng 4: biến môi trường ghi đè mặc định
    }
    if let Ok(v) = env::var("APP_PORT") {
        port = v; // dòng 5
    }
    if args.len() >= 1 {
        host = args[0].clone(); // dòng 6: tham số dòng lệnh ghi đè tất cả
    }
    if args.len() >= 2 {
        port = args[1].clone(); // dòng 7
    }
    // dòng 8–12: TO_INT – "abc" không đổi được thành số (FormatError)
    let p: i64 = match port.trim().parse() {
        Ok(v) => v,
        Err(_) => return Err(ConfigError(format!("Cổng phải là số nguyên: {port}"))),
    };
    if p < 1 || p > 65535 {
        // dòng 13–15: cổng là số 16 bit, khác 0
        return Err(ConfigError(format!("Cổng không hợp lệ: {p}")));
    }
    Ok((host, p as u16)) // dòng 16
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    // In dữ liệu vào để thấy rõ giá trị nào thắng
    let env_host = env::var("APP_HOST").unwrap_or_else(|_| "(không đặt)".to_string());
    let env_port = env::var("APP_PORT").unwrap_or_else(|_| "(không đặt)".to_string());
    let arg_text = if args.is_empty() { "(không có)".to_string() } else { args.join(" ") };
    println!("[cấu hình] APP_HOST={env_host}, APP_PORT={env_port}, tham số: {arg_text}");
    match parse_endpoint(&args, "0.0.0.0", 9000) {
        Ok((host, port)) => println!("[cấu hình] Kết quả: host={host}, port={port}"),
        Err(e) => {
            println!("[cấu hình] ConfigError: {e}");
            process::exit(2);
        }
    }
}
