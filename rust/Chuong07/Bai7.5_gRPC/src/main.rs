//! Bài 7.5 – Dịch vụ gRPC tra cứu thuật ngữ (Mã giả 7.6; Mục 7.4.3)
//! Chạy:  cargo run -- server [host] [port] [độ_trễ_ms]   (mặc định 0.0.0.0 5001 0)
//!        cargo run -- client [host] [port] [hạn_chót_ms]  (mặc định 127.0.0.1 5001 2000)
mod glossary_client;
mod glossary_server;

/// Mã do tonic-build sinh từ tra_cuu.proto (package glossary.v1): thông điệp, trait server, client.
pub mod pb {
    tonic::include_proto!("glossary.v1");
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

/// Đọc tham số cổng thứ i (nếu có), kiểm tra 1–65535.
pub fn port_arg(args: &[String], i: usize, default: u16) -> u16 {
    let p = int_arg(args, i, default as u64);
    if !(1..=65535).contains(&p) {
        exit_with(&format!("Cổng không hợp lệ: {p}"));
    }
    p as u16
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("server") => glossary_server::run(rest).await,
        Some("client") => glossary_client::run(rest).await,
        _ => {
            println!("Cách dùng: cargo run -- server|client [host] [port] [ms]");
            std::process::exit(1);
        }
    }
}
