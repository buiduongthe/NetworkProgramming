//! Bài mở rộng MR6.8 – Đặt vé xem phim: n người cùng chọn ghế (phòng chiếu 8 × 12).
//! Giữ chỗ có thời hạn (TTL, quá hạn tự nhả) -> thanh toán -> vé. Máy chủ TCP đa luồng, 4 cách xử lý tranh
//! chấp (không khóa, khóa loại trừ theo từng ghế, lạc quan phiên bản + CAS, hàng đợi một luồng), sổ
//! kiểm toán đếm "ghế trùng" và "thanh toán sai", bộ mô phỏng tải.
//!
//! Chạy (một chương trình; bốn vai trò dòng lệnh + hai cửa sổ giao diện + kịch bản giao diện):
//!   cargo run -q --release -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--ttl 30] [--ghe-nong E7]
//!   cargo run -q --release -- mo_phong <host> <cổng> [--n 200] [--k 200] [--lan 1] [--che-do khoa]
//!                                      [--dat-lai] [--so-sanh] [--tre-ms 2]
//!   cargo run -q --release -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai ttl_s=3 ghe_nong=E7 | thong_ke
//!   cargo run -q --release -- dat_ve <tên> [host] [cổng] [--thoi-gian 0]
//! Ứng dụng giao diện (egui):
//!   cargo run -q --release -- server_gui [host] [cổng]
//!   cargo run -q --release -- dat_ve_gui [tên] [host] [cổng]
//!   cargo run -q --release -- demo_gui [host] [cổng]        (kịch bản tự thao tác + chụp, dưới xvfb-run)

mod chung;
mod dat_ve;
mod dat_ve_gui;
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
        Some("dat_ve") => dat_ve::run(rest),
        Some("server_gui") => server_gui::run(rest),
        Some("dat_ve_gui") => dat_ve_gui::run(rest),
        Some("demo_gui") => demo_gui::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_8_dat_ve_phim server|mo_phong|khach|dat_ve|server_gui|dat_ve_gui|demo_gui ...");
            std::process::exit(2);
        }
    }
}
