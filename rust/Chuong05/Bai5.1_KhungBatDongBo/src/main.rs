//! Bài 5.1 – Các hàm gửi, nhận khung JSON bất đồng bộ (Mã giả 5.1; Mục 5.2.4 Giáo trình Lập trình mạng).
//!
//! Chạy:  cargo run -q --release -- server [host] [port]      (mặc định 0.0.0.0 9100)
//!        cargo run -q --release -- client [host] [port]      (mặc định 127.0.0.1 9100)

mod frame_client;
mod frame_server;
mod khung_json_async;

use std::env;
use std::process;

// Vòng lặp sự kiện một luồng (giống asyncio): mọi tác vụ chạy xen kẽ trên luồng chính
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "server" => frame_server::run(rest).await,
        "client" => frame_client::run(rest).await,
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server|client [host] [port]");
            process::exit(1);
        }
    }
}
