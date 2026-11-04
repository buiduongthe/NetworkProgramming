//! Bài 3.2 – Thư viện định khung và chương trình minh họa
//! (Mã giả 3.3–3.6; Mục 3.2.4, 3.4 Giáo trình Lập trình mạng).
//!
//! Chạy:  cargo run -q --release -- server [host] [port]               (mặc định 0.0.0.0 9000)
//!        cargo run -q --release -- client [host] [port] [so_ky_tu]    (mặc định 127.0.0.1 9000 0)
//!        cargo run -q --release -- line   [host] [port]               (minh họa READ_LINE, mặc định 127.0.0.1 9000)

mod frame_client;
mod frame_server;
mod framing;
mod line_demo;
mod netutil;

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "server" => frame_server::run(rest),
        "client" => frame_client::run(rest),
        "line" => line_demo::run(rest),
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server|client|line [host] [port] [so_ky_tu]");
            process::exit(1);
        }
    }
}
