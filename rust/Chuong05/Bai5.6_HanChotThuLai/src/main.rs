//! Bài 5.6 – Yêu cầu có hạn chót, thử lại có lùi theo cấp số nhân và nhiễu ngẫu nhiên
//! (Mã giả 5.7, 5.8; Mục 5.4.2, 5.4.3 Giáo trình Lập trình mạng).
//!
//! Chạy:  cargo run -q --release -- server [host] [port]                          (server "khó tính", mặc định 0.0.0.0 9100)
//!        cargo run -q --release -- client [host] [port] [cổng_không_có_server]   (mặc định 127.0.0.1 9100 9101)

mod deadline_retry;
mod flaky_server;
mod khung_json_async;

use std::env;
use std::process;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "server" => flaky_server::run(rest).await,
        "client" => deadline_retry::run(rest).await,
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server|client ...");
            process::exit(1);
        }
    }
}
