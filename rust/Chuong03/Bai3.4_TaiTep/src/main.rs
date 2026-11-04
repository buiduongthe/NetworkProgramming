//! Bài 3.4 – Tải tệp lên server có kiểm tra SHA-256
//! (Mã giả 3.9, 3.10; Mục 3.5.2 Giáo trình Lập trình mạng).
//!
//! Chạy trong thư mục bài (server đọc glossary.json, lưu tệp vào uploads/ ở thư mục hiện hành):
//!        cargo run -q --release -- server  [host] [port]      (mặc định 0.0.0.0 9100)
//!        cargo run -q --release -- client  [host] [port]      (mặc định 127.0.0.1 9100; lệnh: put <tệp> | <thuật ngữ>)
//!        cargo run -q --release -- kiemthu [host] [port]      (client kiểm thử, mặc định 127.0.0.1 9100)

mod framing;
mod kiem_thu;
mod netutil;
mod put_client;
mod put_server;

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "server" => put_server::run(rest),
        "client" => put_client::run(rest),
        "kiemthu" => kiem_thu::run(rest),
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server|client|kiemthu [host] [port]");
            process::exit(1);
        }
    }
}
