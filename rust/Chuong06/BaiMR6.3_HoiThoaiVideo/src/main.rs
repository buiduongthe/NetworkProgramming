//! Bài mở rộng MR6.3 – Phòng hội thoại video nhiều client (ghép lưới kiểu MCU).
//!
//! Chạy (một chương trình, hai vai trò):
//!   cargo run -q --release -- server [host] [cổng_tcp] [cổng_udp]
//!   cargo run -q --release -- khach <tên> <host> <cổng_tcp> [--fps 10] [--thoi-gian 10]
//!                                   [--mat-goi 0.02] [--ghep ket_qua_video.png]

mod chung;
mod khach;
mod server;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("server") => server::run(rest),
        Some("khach") => khach::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_3_hoi_thoai_video server [host] [cổng_tcp] [cổng_udp] | khach <tên> <host> <cổng_tcp> [tùy chọn]");
            std::process::exit(2);
        }
    }
}
