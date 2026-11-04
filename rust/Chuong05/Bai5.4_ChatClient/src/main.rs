//! Bài 5.4 – Lớp dịch vụ ChatClient bất đồng bộ (Mã giả 5.5; Mục 5.3.3 Giáo trình Lập trình mạng).
//!
//! Chạy:  cargo run -q --release -- server [host] [port]          (server chat tối giản, mặc định 0.0.0.0 9200)
//!        cargo run -q --release -- cli <tên> [host] [port]       (giao diện dòng lệnh, mặc định 127.0.0.1 9200)

mod chat_cli;
mod chat_client;
mod chat_server;
mod khung_json_async;

use std::env;
use std::process;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "server" => chat_server::run(rest).await,
        "cli" => chat_cli::run(rest).await,
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server|cli ...");
            process::exit(1);
        }
    }
}
