//! Bài 4.1 – UDP Echo Server và UDP Echo Client có thời gian chờ
//! (Mã giả 4.1, 4.2; Mục 4.2.1 Giáo trình Lập trình mạng).
//!
//! Chạy:  cargo run -q --release -- server [host] [port]      (mặc định 0.0.0.0 9000)
//!        cargo run -q --release -- client [host] [port]      (mặc định 127.0.0.1 9000)

mod endpoint;
mod udp_echo_client;
mod udp_echo_server;

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "server" => udp_echo_server::run(rest),
        "client" => udp_echo_client::run(rest),
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server|client [host] [port]");
            process::exit(1);
        }
    }
}
