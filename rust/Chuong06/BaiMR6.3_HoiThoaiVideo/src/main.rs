//! Bài mở rộng MR6.3 – Phòng hội thoại video nhiều client (ghép lưới kiểu MCU).
//!
//! Chạy (một chương trình; hai vai trò dòng lệnh + ba vai trò giao diện):
//!   cargo run -q --release -- server [host] [cổng_tcp] [cổng_udp]
//!   cargo run -q --release -- khach <tên> <host> <cổng_tcp> [--fps 10] [--thoi-gian 10]
//!                                   [--mat-goi 0.02] [--ghep ket_qua_video.png]
//! Ứng dụng giao diện (egui; camera thật qua tiến trình ffmpeg, không có thì camera giả lập):
//!   cargo run -q --release -- server_gui [ip] [cổng_tcp] [cổng_udp]
//!   cargo run -q --release -- khach_gui [tên] [ip] [cổng_tcp]
//!   cargo run -q --release -- demo_gui [cổng_tcp] [cổng_udp] [ảnh.png]   (kịch bản tự thao tác + chụp)

mod camera_ffmpeg;
mod chung;
mod demo_gui;
mod gui_chung;
mod khach;
mod khach_gui;
mod server;
mod server_gui;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("server") => server::run(rest),
        Some("khach") => khach::run(rest),
        Some("server_gui") => server_gui::run(rest),
        Some("khach_gui") => khach_gui::run(rest),
        Some("demo_gui") => demo_gui::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_3_hoi_thoai_video server [host] [cổng_tcp] [cổng_udp] | khach <tên> <host> <cổng_tcp> [tùy chọn] | server_gui | khach_gui | demo_gui");
            std::process::exit(2);
        }
    }
}
