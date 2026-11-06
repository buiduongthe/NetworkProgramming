//! Bài mở rộng MR6.6 – Đấu giá trực tuyến: n người cùng tranh một món hàng.
//! Máy chủ TCP đa luồng, 4 cách xử lý tranh chấp (không khóa, khóa loại trừ, lạc quan phiên bản + CAS,
//! hàng đợi một luồng), sổ kiểm toán, đồng hồ phiên gia hạn chống "bắn tỉa", bộ mô phỏng tải.
//!
//! Chạy (một chương trình; bốn vai trò dòng lệnh + hai cửa sổ giao diện + kịch bản giao diện):
//!   cargo run -q --release -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--thoi-gian 0]
//!                                    [--gia 5000000] [--buoc 100000] [--gia-han 10]
//!   cargo run -q --release -- mo_phong <host> <cổng> [--n 200] [--k 200] [--lan 5] [--che-do khoa]
//!                                      [--dat-lai] [--so-sanh] [--tre-ms 2]
//!   cargo run -q --release -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//!   cargo run -q --release -- dau_gia <tên> [host] [cổng] [--thoi-gian 0]
//! Ứng dụng giao diện (egui):
//!   cargo run -q --release -- server_gui [host] [cổng]
//!   cargo run -q --release -- dau_gia_gui [tên] [host] [cổng]
//!   cargo run -q --release -- demo_gui [host] [cổng]        (kịch bản tự thao tác + chụp, dưới xvfb-run)

mod chung;
mod dau_gia;
mod dau_gia_gui;
mod demo_gui;
mod gui_chung;
mod khach;
mod may_chu;
mod mo_phong;
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
        Some("dau_gia") => dau_gia::run(rest),
        Some("server_gui") => server_gui::run(rest),
        Some("dau_gia_gui") => dau_gia_gui::run(rest),
        Some("demo_gui") => demo_gui::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_6_dau_gia server|mo_phong|khach|dau_gia|server_gui|dau_gia_gui|demo_gui ...");
            std::process::exit(2);
        }
    }
}
