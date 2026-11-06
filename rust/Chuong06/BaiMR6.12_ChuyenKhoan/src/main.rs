//! Bài mở rộng MR6.12 – Chuyển khoản: nhiều người chuyển chéo giữa 5–20 tài khoản cùng lúc.
//! Máy chủ TCP đa luồng, 4 cách xử lý tranh chấp (không khóa, khóa hai tài khoản theo thứ tự mã – hoặc
//! "ngây thơ" nguồn rồi đích để thấy khóa chết, lạc quan phiên bản + CAS trên hai ô, hàng đợi một luồng),
//! sổ kiểm toán (tổng lệch, lệch tài khoản, thấu chi), bộ mô phỏng tải.
//!
//! Chạy (một chương trình; bốn vai trò dòng lệnh + hai cửa sổ giao diện + kịch bản giao diện):
//!   cargo run -q --release -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-tk 10] [--so-du 5000000]
//!                                    [--khoa-ngay-tho]
//!   cargo run -q --release -- mo_phong <host> <cổng> [--n 500] [--k 200] [--lan 5] [--che-do khoa]
//!                                      [--dat-lai] [--so-sanh] [--tre-ms 2]
//!   cargo run -q --release -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//!   cargo run -q --release -- ngan_hang <tên> [host] [cổng] [--tk TK01] [--thoi-gian 0]
//! Ứng dụng giao diện (egui):
//!   cargo run -q --release -- server_gui [host] [cổng]
//!   cargo run -q --release -- ngan_hang_gui [tên] [host] [cổng]
//!   cargo run -q --release -- demo_gui [host] [cổng]        (kịch bản tự thao tác + chụp, dưới xvfb-run)

mod chung;
mod demo_gui;
mod gui_chung;
mod khach;
mod may_chu;
mod mo_phong;
mod ngan_hang;
mod ngan_hang_gui;
mod nguoi_ao;
mod server;
mod server_gui;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("server") => server::run(rest),
        Some("mo_phong") => mo_phong::run(rest),
        Some("khach") => khach::run(rest),
        Some("ngan_hang") => ngan_hang::run(rest),
        Some("server_gui") => server_gui::run(rest),
        Some("ngan_hang_gui") => ngan_hang_gui::run(rest),
        Some("demo_gui") => demo_gui::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_12_chuyen_khoan server|mo_phong|khach|ngan_hang|server_gui|ngan_hang_gui|demo_gui ...");
            std::process::exit(2);
        }
    }
}
