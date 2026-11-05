//! Bài mở rộng MR6.5 – Phát trực tiếp âm thanh + video 1→n (livestream): máy chủ trung chuyển SFU,
//! khung I/P, bộ đệm GOP, xin khung khóa (PLI), đồng bộ tiếng-hình.
//!
//! Chạy (một chương trình; ba vai trò dòng lệnh + ba cửa sổ giao diện + kịch bản giao diện):
//!   cargo run -q --release -- server_phat [host] [cổng_tcp] [cổng_udp]
//!   cargo run -q --release -- phat <tên> <host> <cổng_tcp> [--camera gia_lap|testsrc|<thiết bị>]
//!                                  [--mic gia_lap|<chỉ số>] [--tep <tệp video> [--tao-tep-mau]]
//!                                  [--fps 10] [--gop 20] [--thoi-gian 30] [--mat-goi 0.0]
//!   cargo run -q --release -- xem <tên> <host> <cổng_tcp> [--thoi-gian 10] [--tre 0] [--dem 5]
//!                                 [--khong-dong-bo] [--chi-am-thanh] [--chi-video] [--mat-goi 0.0]
//!                                 [--ghep ket_qua_video.png]
//! Ứng dụng giao diện (egui; camera/tệp video qua tiến trình ffmpeg, micro/loa thật qua cpal):
//!   cargo run -q --release -- server_gui [host] [cổng_tcp] [cổng_udp]
//!   cargo run -q --release -- phat_gui [tên] [host] [cổng_tcp]
//!   cargo run -q --release -- xem_gui [tên] [host] [cổng_tcp]
//!   cargo run -q --release -- demo_gui [host] [cổng_tcp] [cổng_udp] [ảnh.png]   (kịch bản tự thao tác + chụp)

mod camera;
mod chung;
mod demo_gui;
mod gui_chung;
mod may_chu;
mod may_phat;
mod nguoi_xem;
mod phat;
mod phat_gui;
mod server_gui;
mod server_phat;
mod tep_video;
mod thiet_bi;
mod xem;
mod xem_gui;

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
        Some("server_phat") => server_phat::run(rest),
        Some("phat") => phat::run(Args::parse(rest, &["tao-tep-mau"])),
        Some("xem") => xem::run(Args::parse(rest, &["khong-dong-bo", "chi-am-thanh", "chi-video"])),
        Some("server_gui") => server_gui::run(rest),
        Some("phat_gui") => phat_gui::run(rest),
        Some("xem_gui") => xem_gui::run(rest),
        Some("demo_gui") => demo_gui::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_5_phat_truc_tiep server_phat|phat|xem|server_gui|phat_gui|xem_gui|demo_gui ...");
            std::process::exit(2);
        }
    }
}
