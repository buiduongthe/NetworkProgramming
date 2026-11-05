//! Bài mở rộng MR4.1 – Thu thập và phân tích NetFlow v5 trên UDP.
//!
//! Chạy (một chương trình, hai vai trò):
//!   cargo run -q --release -- thuthap [host] [port] [--nhan-roi S]          bộ thu thập (collector)
//!   cargo run -q --release -- xuat <host> <port> [--so-luong N] [--bo-goi K]  máy xuất giả lập (exporter)

mod bo_thu_thap;
mod may_xuat;
mod netflow_v5;

use std::process;

/// Tách tham số vị trí và tùy chọn "--khóa giá_trị".
pub fn split_args(args: &[String]) -> (Vec<String>, Vec<(String, String)>) {
    let (mut pos, mut opt) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < args.len() {
        if let Some(k) = args[i].strip_prefix("--") {
            opt.push((k.to_string(), args.get(i + 1).cloned().unwrap_or_default()));
            i += 2;
        } else {
            pos.push(args[i].clone());
            i += 1;
        }
    }
    (pos, opt)
}

pub fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("thuthap") => bo_thu_thap::run(rest),
        Some("xuat") => may_xuat::run(rest),
        _ => die("Cách dùng: mr4_1_thu_thap_netflow thuthap [host] [port] [--nhan-roi S] | xuat <host> <port> [--so-luong N] [--bo-goi K]"),
    }
}
