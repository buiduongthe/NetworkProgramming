//! Bài 4.2 – Dịch vụ tra cứu thuật ngữ trên UDP (Mã giả 4.3, 4.4; Mục 4.3.1 Giáo trình Lập trình mạng).
//!
//! Chạy (trong thư mục bài, nơi có glossary.json):
//!        cargo run -q --release -- server [host] [port] [so_datagram_bo_qua]   (mặc định 0.0.0.0 9100 0)
//!        cargo run -q --release -- client [host] [port]                        (mặc định 127.0.0.1 9100)

mod endpoint;
mod udp_lookup_client;
mod udp_lookup_server;

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "server" => udp_lookup_server::run(rest),
        "client" => udp_lookup_client::run(rest),
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server [host] [port] [so_bo_qua] | client [host] [port]");
            process::exit(1);
        }
    }
}
