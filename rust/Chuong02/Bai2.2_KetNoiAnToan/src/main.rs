//! Bài 2.2 – Kết nối có thời gian chờ và phân loại lỗi
//! (Mã giả 2.2 SAFE_CONNECT; Mục 2.2.3 Giáo trình Lập trình mạng).
//!
//! Chạy:  cargo run -q --release -- client [host] [port] [timeout_ms]   (mặc định 127.0.0.1 9000 3000)
//!        cargo run -q --release -- server [host] [port]                (server thử nghiệm, mặc định 0.0.0.0 9000)

mod client;
mod endpoint;
mod safe_connect;
mod test_server;

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    let code = match role {
        "server" => test_server::run(rest),
        "client" => client::run(rest),
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server|client [host] [port] [timeout_ms]");
            1
        }
    };
    process::exit(code);
}
