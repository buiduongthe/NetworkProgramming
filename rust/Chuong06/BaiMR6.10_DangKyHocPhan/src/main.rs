//! Bài mở rộng MR6.10 – Đăng ký học phần: hàng nghìn sinh viên tranh vài lớp sĩ số 50.
//! Máy chủ TCP đa luồng, 4 cách xử lý tranh chấp (không khóa, khóa loại trừ theo lớp, lạc quan phiên bản +
//! CAS, hàng đợi một luồng), sổ kiểm toán (vượt sĩ số, đăng ký trùng), bộ mô phỏng tải.
//!
//! Chạy (một chương trình; bốn vai trò dòng lệnh + hai cửa sổ giao diện + kịch bản giao diện):
//!   cargo run -q --release -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-lop 3] [--si-so 50]
//!   cargo run -q --release -- mo_phong <host> <cổng> [--n 2000] [--k 200] [--lan 1] [--che-do khoa]
//!                                      [--dat-lai] [--so-sanh] [--tre-ms 2]
//!   cargo run -q --release -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//!   cargo run -q --release -- dang_ky <tên> [host] [cổng] [--thoi-gian 0]
//! Ứng dụng giao diện (egui):
//!   cargo run -q --release -- server_gui [host] [cổng]
//!   cargo run -q --release -- dang_ky_gui [tên] [host] [cổng]
//!   cargo run -q --release -- demo_gui [host] [cổng]        (kịch bản tự thao tác + chụp, dưới xvfb-run)

mod chung;
mod dang_ky;
mod dang_ky_gui;
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
        Some("dang_ky") => dang_ky::run(rest),
        Some("server_gui") => server_gui::run(rest),
        Some("dang_ky_gui") => dang_ky_gui::run(rest),
        Some("demo_gui") => demo_gui::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_10_dang_ky_hoc_phan server|mo_phong|khach|dang_ky|server_gui|dang_ky_gui|demo_gui ...");
            std::process::exit(2);
        }
    }
}
