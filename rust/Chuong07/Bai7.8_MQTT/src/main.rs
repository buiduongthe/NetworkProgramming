//! Bài 7.8 – Thiết bị cảm biến và bảng theo dõi dùng MQTT (Mã giả 7.9; Mục 7.5.5)
//! Chạy:  cargo run -- sensor <mã_thiết_bị> [host] [port] [số_lần] [chu_kỳ_ms] [--mat-ket-noi]
//!        cargo run -- dashboard [host] [port] [keep_alive_s] [client_id]
mod dashboard;
mod mqtt_mini;
mod sensor;

/// In thông báo lỗi rồi thoát chương trình.
pub fn exit_with(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

/// Đọc tham số nguyên thứ i (nếu có), ngược lại dùng giá trị mặc định.
pub fn int_arg(args: &[String], i: usize, default: u64) -> u64 {
    match args.get(i) {
        Some(s) => s.parse().unwrap_or_else(|_| exit_with(&format!("Tham số phải là số nguyên: {s}"))),
        None => default,
    }
}

/// Đọc tham số cổng thứ i (nếu có), kiểm tra 1–65535.
pub fn port_arg(args: &[String], i: usize, default: u16) -> u16 {
    let p = int_arg(args, i, default as u64);
    if !(1..=65535).contains(&p) {
        exit_with(&format!("Cổng không hợp lệ: {p}"));
    }
    p as u16
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("sensor") => sensor::run(rest),
        Some("dashboard") => dashboard::run(rest),
        _ => {
            println!("Cách dùng: cargo run -- sensor|dashboard ...");
            std::process::exit(1);
        }
    }
}
