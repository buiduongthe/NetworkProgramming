//! Bài 3.1 – TCP Echo Server tuần tự và TCP Echo Client
//! (Mã giả 3.1, 3.2; Mục 3.2 Giáo trình Lập trình mạng).
//!
//! Chạy:  cargo run -q --release -- server [host] [port]      (mặc định 0.0.0.0 9000)
//!        cargo run -q --release -- client [host] [port]      (mặc định 127.0.0.1 9000)

mod echo_client;
mod echo_server;
mod netutil;

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "server" => echo_server::run(rest),
        "client" => echo_client::run(rest),
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server|client [host] [port]");
            process::exit(1);
        }
    }
}
