//! Bài mở rộng MR6.2 – Phòng hội thoại âm thanh nhiều client (trộn tiếng "mix-minus").
//!
//! Chạy (một chương trình; ba vai trò dòng lệnh + ba vai trò giao diện):
//!   cargo run -q --release -- server [host] [cổng_tcp] [cổng_udp]
//!   cargo run -q --release -- khach <tên> <host> <cổng_tcp> [--tan-so 440] [--bien-do 0.25] [--wav tệp.wav]
//!                                   [--ghi out/ghi.wav] [--thoi-gian 10] [--mat-goi 0.05] [--dem 3]
//!   cargo run -q --release -- phan_tich <tệp.wav> <tần_số:nhãn> ... [--cua-so 1.0] [--nguong -50]
//! Ứng dụng giao diện (egui; micro/loa thật qua cpal, không có thì giả lập):
//!   cargo run -q --release -- server_gui [ip] [cổng_tcp] [cổng_udp]
//!   cargo run -q --release -- khach_gui [tên] [ip] [cổng_tcp] [--tan-so 440]
//!   cargo run -q --release -- demo_gui [cổng_tcp] [cổng_udp] [ảnh.png]   (kịch bản tự thao tác + chụp)

mod chung;
mod demo_gui;
mod gui_chung;
mod khach;
mod khach_gui;
mod phan_tich;
mod server;
mod server_gui;
mod thiet_bi;

use std::collections::HashMap;

/// Tham số dòng lệnh: các giá trị vị trí và các cặp "--khóa giá_trị" (hoặc cờ "--khóa").
pub struct Args {
    pub pos: Vec<String>,
    pub opt: HashMap<String, String>,
}

impl Args {
    pub fn parse(args: &[String], flags: &[&str]) -> Args {
        let (mut pos, mut opt) = (Vec::new(), HashMap::new());
        let mut i = 0;
        while i < args.len() {
            if let Some(k) = args[i].strip_prefix("--") {
                if flags.contains(&k) {
                    opt.insert(k.to_string(), "1".to_string());
                    i += 1;
                } else {
                    opt.insert(k.to_string(), args.get(i + 1).cloned().unwrap_or_default());
                    i += 2;
                }
            } else {
                pos.push(args[i].clone());
                i += 1;
            }
        }
        Args { pos, opt }
    }
    pub fn f64(&self, k: &str, d: f64) -> f64 {
        self.opt.get(k).and_then(|v| v.parse().ok()).unwrap_or(d)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("server") => server::run(rest),
        Some("khach") => khach::run(Args::parse(rest, &["mic", "loa"])),
        Some("phan_tich") => phan_tich::run(Args::parse(rest, &[])),
        Some("server_gui") => server_gui::run(rest),
        Some("khach_gui") => khach_gui::run(Args::parse(rest, &[])),
        Some("demo_gui") => demo_gui::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_2_hoi_thoai_am_thanh server|khach|phan_tich|server_gui|khach_gui|demo_gui ...");
            std::process::exit(2);
        }
    }
}
