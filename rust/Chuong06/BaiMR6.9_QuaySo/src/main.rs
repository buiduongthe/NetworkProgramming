//! Bài mở rộng MR6.9 – Quay số trúng thưởng: 100 000 lượt quay tranh 10 món quà.
//! Máy chủ TCP đa luồng, 4 cách xử lý tranh chấp (không khóa, khóa loại trừ, lạc quan phiên bản + CAS,
//! hàng đợi một luồng), sổ kiểm toán (thừa quà, quay 2 lần), mỗi người một ô (thêm ô O(1)), bộ mô phỏng tải.
//!
//! Chạy (một chương trình; bốn vai trò dòng lệnh + hai cửa sổ giao diện + kịch bản giao diện):
//!   cargo run -q --release -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-qua 10] [--p 0.0005]
//!   cargo run -q --release -- mo_phong <host> <cổng> [--n 100000] [--k 400] [--lan 1] [--che-do khoa]
//!                                      [--dat-lai] [--so-sanh] [--tre-ms 2]
//!   cargo run -q --release -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//!   cargo run -q --release -- quay_so <tên> [host] [cổng] [--thoi-gian 0]
//! Ứng dụng giao diện (egui):
//!   cargo run -q --release -- server_gui [host] [cổng]
//!   cargo run -q --release -- vong_quay_gui [tên] [host] [cổng]
//!   cargo run -q --release -- demo_gui [host] [cổng]        (kịch bản tự thao tác + chụp, dưới xvfb-run)

mod chung;
mod demo_gui;
mod gui_chung;
mod khach;
mod may_chu;
mod mo_phong;
mod nguoi_ao;
mod quay_so;
mod server;
mod server_gui;
mod vong_quay_gui;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("server") => server::run(rest),
        Some("mo_phong") => mo_phong::run(rest),
        Some("khach") => khach::run(rest),
        Some("quay_so") => quay_so::run(rest),
        Some("server_gui") => server_gui::run(rest),
        Some("vong_quay_gui") => vong_quay_gui::run(rest),
        Some("demo_gui") => demo_gui::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_9_quay_so server|mo_phong|khach|quay_so|server_gui|vong_quay_gui|demo_gui ...");
            std::process::exit(2);
        }
    }
}
