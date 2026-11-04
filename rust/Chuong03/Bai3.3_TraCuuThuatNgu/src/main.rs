//! Bài 3.3 – Dịch vụ tra cứu thuật ngữ dùng khung JSON
//! (Mã giả 3.7, 3.8; Mục 3.5.1, 3.5.3 Giáo trình Lập trình mạng).
//!
//! Chạy trong thư mục bài (server đọc glossary.json ở thư mục hiện hành):
//!        cargo run -q --release -- server  [host] [port]      (mặc định 0.0.0.0 9100)
//!        cargo run -q --release -- client  [host] [port]      (mặc định 127.0.0.1 9100)
//!        cargo run -q --release -- kiemthu [host] [port]      (client kiểm thử, mặc định 127.0.0.1 9100)

mod framing;
mod kiem_thu;
mod lookup_client;
mod lookup_server;
mod netutil;

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "server" => lookup_server::run(rest),
        "client" => lookup_client::run(rest),
        "kiemthu" => kiem_thu::run(rest),
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server|client|kiemthu [host] [port]");
            process::exit(1);
        }
    }
}
