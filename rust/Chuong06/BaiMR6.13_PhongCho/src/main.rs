//! Bài mở rộng MR6.13 – Phòng chờ ảo: bán 500 vé cho rất nhiều người, cấp lượt theo thùng token.
//! Máy chủ TCP đa luồng, 4 cách xử lý tranh chấp (không khóa, khóa loại trừ, lạc quan phiên bản + CAS,
//! hàng đợi một luồng), sổ kiểm toán (vượt vé, sai lượt, đảo số), "chờ dài" cho_luot, bộ mô phỏng tải và
//! so sánh bật/tắt phòng chờ.
//!
//! Chạy (một chương trình; năm vai trò dòng lệnh + hai cửa sổ giao diện + kịch bản giao diện):
//!   cargo run -q --release -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--tong-ve 500] [--toc-do 50]
//!                                    [--dung-luong 50] [--luot-s 20] [--gioi-han 64] [--tat-phong-cho]
//!   cargo run -q --release -- mo_phong <host> <cổng> [--n 1000] [--k 200] [--che-do khoa] [--dat-lai]
//!                                      [--so-sanh] [--tre-ms 2]
//!   cargo run -q --release -- so_sanh_phong_cho [host] [cổng] [--n 1000] [--k 200] [--che-do khoa]
//!   cargo run -q --release -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//!   cargo run -q --release -- mua_ve <tên> [host] [cổng] [--tu-dong] [--so-ve 2] [--thoi-gian 0]
//! Ứng dụng giao diện (egui):
//!   cargo run -q --release -- server_gui [host] [cổng]
//!   cargo run -q --release -- phong_cho_gui [tên] [host] [cổng]
//!   cargo run -q --release -- demo_gui [host] [cổng]        (kịch bản tự thao tác + chụp, dưới xvfb-run)

mod chung;
mod demo_gui;
mod gui_chung;
mod khach;
mod may_chu;
mod mo_phong;
mod mua_ve;
mod nguoi_ao;
mod phong_cho_gui;
mod server;
mod server_gui;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("server") => server::run(rest),
        Some("mo_phong") => mo_phong::run(rest),
        Some("so_sanh_phong_cho") => nguoi_ao::run(rest),
        Some("khach") => khach::run(rest),
        Some("mua_ve") => mua_ve::run(rest),
        Some("server_gui") => server_gui::run(rest),
        Some("phong_cho_gui") => phong_cho_gui::run(rest),
        Some("demo_gui") => demo_gui::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_13_phong_cho server|mo_phong|so_sanh_phong_cho|khach|mua_ve|server_gui|phong_cho_gui|demo_gui ...");
            std::process::exit(2);
        }
    }
}
