//! Bài 4.4 – Truyền tệp dừng và chờ trên UDP (Mã giả 4.6, 4.7; Mục 4.4.1–4.4.2 Giáo trình Lập trình mạng).
//!
//! Chạy:  cargo run -q --release -- recv [host] [port] [tep_ra] [mat_goi] [cho_sau_fin_ms]   (mặc định 0.0.0.0 9000 tep_nhan.bin, 120000)
//!        cargo run -q --release -- send <tep> [host] [port] [mat_goi]     (mặc định 127.0.0.1 9000)
//! mat_goi: "0.1" (ngẫu nhiên 10%) hoặc "3,7" (mất datagram gửi đi thứ 3 và 7); bỏ trống: không mất gói

mod endpoint;
mod saw_common;
mod saw_recv;
mod saw_send;

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let code = match role {
        "recv" => saw_recv::run(&args[1..]),
        "send" if args.len() >= 2 => saw_send::run(&args[1..]),
        _ => {
            eprintln!(
                "Cách dùng: cargo run -q --release -- recv [host] [port] [tep_ra] [mat_goi] [cho_sau_fin_ms] | send <tep> [host] [port] [mat_goi]"
            );
            1
        }
    };
    process::exit(code);
}
