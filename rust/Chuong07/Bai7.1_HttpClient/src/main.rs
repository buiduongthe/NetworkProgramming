//! Bài 7.1 – Client HTTP/1.1 tối giản trên socket TCP (Mã giả 7.1; Mục 7.2.1 Giáo trình Lập trình mạng)
//! Chạy:  cargo run -- client [host] [port] [đường_dẫn ...]   (mặc định 127.0.0.1 8080 /)
//!        cargo run -- server [host] [port]                  (server thử, mặc định 0.0.0.0 8080)
mod http_client;
mod server_thu;

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
        Some("server") => server_thu::run(rest),
        Some("client") => http_client::run(rest),
        _ => {
            println!("Cách dùng: cargo run -- client|server [host] [port] [đường_dẫn ...]");
            std::process::exit(1);
        }
    }
}
