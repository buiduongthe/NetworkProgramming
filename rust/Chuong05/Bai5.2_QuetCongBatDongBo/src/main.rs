//! Bài 5.2 – Kiểm tra nhiều cổng đồng thời có giới hạn (Mã giả 5.2; Mục 5.2.5 Giáo trình Lập trình mạng).
//!
//! Chạy:  cargo run -q --release -- scan [host] [cổng_đầu] [cổng_cuối] [max_parallel]   (mặc định 127.0.0.1 9000 9019 10)
//!        cargo run -q --release -- ports <cổng_mở,...> <cổng_lọc,...>                  (mở cổng minh họa trên 127.0.0.1)

mod demo_ports;
mod scan_async;

use std::env;
use std::process;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match role {
        "scan" => scan_async::run(rest).await,
        "ports" => demo_ports::run(rest).await,
        _ => {
            eprintln!("Cách dùng: cargo run -q --release -- scan|ports ...");
            process::exit(1);
        }
    }
}
