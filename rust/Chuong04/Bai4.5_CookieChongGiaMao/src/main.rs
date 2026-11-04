//! Bài 4.5 – Xác thực địa chỉ nguồn bằng mã thách thức không trạng thái
//! (Mã giả 4.8; Mục 4.6.1 Giáo trình Lập trình mạng).
//!
//! Chạy (trong thư mục bài, nơi có glossary.json):
//!        cargo run -q --release -- server [host] [port]                    (mặc định 0.0.0.0 9100)
//!        cargo run -q --release -- client [host] [port] [cookie_ban_dau]   (mặc định 127.0.0.1 9100)

mod endpoint;
mod udp_cookie_client;
mod udp_cookie_server;

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "server" => udp_cookie_server::run(rest),
        "client" => udp_cookie_client::run(rest),
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server [host] [port] | client [host] [port] [cookie]");
            process::exit(1);
        }
    }
}
