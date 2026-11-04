//! Bài 7.2 – Server REST tối giản trên HTTP/1.1 (Mã giả 7.2; Mục 7.2.3 Giáo trình Lập trình mạng)
//! Chạy:  cargo run -- server [host] [port]      (mặc định 0.0.0.0 8080)
//!        cargo run -- client [host] [port]      (mặc định 127.0.0.1 8080; đọc yêu cầu từ bàn phím)
mod rest_client;
mod rest_server;

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
        Some("server") => tokio::runtime::Runtime::new().unwrap().block_on(rest_server::run(rest)),
        // client dùng socket đồng bộ của thư viện chuẩn
        Some("client") => rest_client::run(rest),
        _ => {
            println!("Cách dùng: cargo run -- server|client [host] [port]");
            std::process::exit(1);
        }
    }
}
