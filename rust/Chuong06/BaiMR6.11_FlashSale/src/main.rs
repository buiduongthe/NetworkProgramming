//! Bài mở rộng MR6.11 – Flash sale: 10 000 người cùng bấm "MUA NGAY" tranh kho 100 sản phẩm.
//! Máy chủ TCP đa luồng, 4 cách xử lý tranh chấp (không khóa, khóa loại trừ, lạc quan phiên bản + CAS,
//! hàng đợi một luồng), sổ kiểm toán (tồn âm, lệch sổ), đồng hồ mở bán, bộ mô phỏng tải.
//!
//! Chạy (một chương trình; bốn vai trò dòng lệnh + hai cửa sổ giao diện + kịch bản giao diện):
//!   cargo run -q --release -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--ton 100] [--mo-sau 30]
//!   cargo run -q --release -- mo_phong <host> <cổng> [--n 10000] [--k 200] [--lan 1] [--che-do khoa]
//!                                      [--dat-lai] [--so-sanh] [--tre-ms 2]
//!   cargo run -q --release -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//!   cargo run -q --release -- flash_sale <tên> [host] [cổng] [--thoi-gian 0]
//! Ứng dụng giao diện (egui):
//!   cargo run -q --release -- server_gui [host] [cổng]
//!   cargo run -q --release -- flash_sale_gui [tên] [host] [cổng]
//!   cargo run -q --release -- demo_gui [host] [cổng]        (kịch bản tự thao tác + chụp, dưới xvfb-run)

mod chung;
mod demo_gui;
mod flash_sale;
mod flash_sale_gui;
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
        Some("flash_sale") => flash_sale::run(rest),
        Some("server_gui") => server_gui::run(rest),
        Some("flash_sale_gui") => flash_sale_gui::run(rest),
        Some("demo_gui") => demo_gui::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_11_flash_sale server|mo_phong|khach|flash_sale|server_gui|flash_sale_gui|demo_gui ...");
            std::process::exit(2);
        }
    }
}
