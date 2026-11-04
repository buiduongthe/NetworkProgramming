//! Bài 5.5 – Tải tệp bất đồng bộ có báo cáo tiến trình và hủy (Mã giả 5.6; Mục 5.3.4 Giáo trình Lập trình mạng).
//!
//! Chạy:  cargo run -q --release -- server [host] [port] [rate_kib]               (mặc định 0.0.0.0 9100 0 = không giới hạn)
//!        cargo run -q --release -- client [host] [port] [tệp] [hủy_sau_ms]       (mặc định 127.0.0.1 9100 du_lieu_thu.bin)

mod khung_json_async;
mod put_client;
mod put_server;

use std::env;
use std::process;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "server" => put_server::run(rest).await,
        "client" => put_client::run(rest).await,
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server|client ...");
            process::exit(1);
        }
    }
}
