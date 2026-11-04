//! Bài 5.3 – Ứng dụng tra cứu có giao diện bất đồng bộ (Mã giả 5.3, 5.4; Mục 5.3.2 Giáo trình Lập trình mạng).
//! Giao diện là một cửa sổ vẽ bằng ký tự trên terminal (xem README).
//!
//! Chạy:  cargo run -q --release -- server [host] [port] [delay_ms]   (mặc định 0.0.0.0 9100 0)
//!        cargo run -q --release -- gui [host] [port]                 (mặc định 127.0.0.1 9100)
//!        cargo run -q --release -- demo <host> <port> <cổng_không_có_server>   (kịch bản tự động)

mod demo_gui;
mod khung_json_async;
mod lookup_gui;
mod lookup_server;

use khung_json_async::parse_endpoint;
use std::env;
use std::process;
use tokio::task::LocalSet;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    // LocalSet: cho phép spawn_local – tác vụ chạy trên chính luồng này (luồng giao diện)
    let local = LocalSet::new();
    match role {
        "server" => lookup_server::run(rest).await,
        "gui" => {
            let (host, port) = parse_endpoint(rest, "127.0.0.1", 9100);
            local.run_until(lookup_gui::run(&host, port)).await;
        }
        "demo" if rest.len() >= 3 => {
            let (host, port) = parse_endpoint(&rest[..2], "127.0.0.1", 9100);
            let bad: u16 = rest[2].trim().parse().unwrap_or(9101);
            local.run_until(demo_gui::run(&host, port, bad)).await;
        }
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- server|gui|demo [host] [port] ...");
            process::exit(1);
        }
    }
}
