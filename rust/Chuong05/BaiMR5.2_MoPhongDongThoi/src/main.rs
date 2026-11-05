//! Bài mở rộng MR5.2 – So sánh các mô hình đồng thời trên cùng một khối việc (Rust).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1, 5.2 – bất đồng bộ, giới hạn đồng
//! thời) và Chương 6 (Bài 6.1, 6.2 – server đa luồng, nhóm luồng, bất đồng bộ).
//!
//! Chạy:  cargo run -q --release -- server <host> <port> [trễ_ms]    máy chủ thử (tokio)
//!        cargo run -q --release -- mang <host> <port>               20 yêu cầu mạng, bốn mô hình
//!        cargo run -q --release -- cpu                              việc nặng CPU, không có GIL
//!        cargo run -q --release -- tron <host> <port>               mạng + CPU, tokio và nhóm luồng
//!        cargo run -q --release -- khoa                             tranh chấp dữ liệu, bế tắc

mod dong_ho;
mod dong_thoi;
mod khoa;
mod may_chu_tre;
mod nhom_luong;

use std::process;

fn usage() -> ! {
    eprintln!("Cách dùng: mr5_2_mo_phong_dong_thoi server <host> <port> [trễ_ms] | mang <host> <port> | cpu | tron <host> <port> | khoa");
    process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let port = |i: usize| -> u16 { args.get(i).and_then(|p| p.parse().ok()).unwrap_or_else(|| usage()) };
    match args.first().map(String::as_str) {
        Some("server") if args.len() >= 3 => {
            let delay = args.get(3).and_then(|d| d.parse().ok()).unwrap_or(200);
            may_chu_tre::run(&args[1], port(2), delay)
        }
        Some("mang") if args.len() >= 3 => dong_thoi::run_net(&args[1], port(2)),
        Some("tron") if args.len() >= 3 => dong_thoi::run_mixed(&args[1], port(2)),
        Some("cpu") => dong_thoi::run_cpu(),
        Some("khoa") => khoa::run(),
        _ => usage(),
    }
}
