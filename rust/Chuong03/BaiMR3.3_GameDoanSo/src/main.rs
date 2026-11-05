//! Bài mở rộng MR3.3 – Trò chơi đoán số nhiều người chơi qua TCP.
//!
//! Chạy (một chương trình, hai vai trò):
//!   cargo run -q --release -- server [host] [port] [--so-nguoi 2] [--het-gio-ms 30000]
//!                                    [--seed 0] [--min 1] [--max 100] [--so-van 0]
//!   cargo run -q --release -- choi <tên> [host] [port] [--bot KIỂU] [--seed N] [--nghi-ms 300]

mod giao_thuc;
mod nguoi_choi;
mod server;

use std::collections::HashMap;
use std::process;

/// Tham số dòng lệnh: các giá trị vị trí và các cặp "--khóa giá_trị".
pub struct Args {
    pub pos: Vec<String>,
    pub opt: HashMap<String, String>,
}

impl Args {
    pub fn parse(args: &[String]) -> Args {
        let (mut pos, mut opt) = (Vec::new(), HashMap::new());
        let mut i = 0;
        while i < args.len() {
            if let Some(k) = args[i].strip_prefix("--") {
                let v = args.get(i + 1).cloned().unwrap_or_else(|| die(&format!("--{k} thiếu giá trị")));
                opt.insert(k.to_string(), v);
                i += 2;
            } else {
                pos.push(args[i].clone());
                i += 1;
            }
        }
        Args { pos, opt }
    }
    /// Giá trị số của "--khóa" (hoặc mặc định).
    pub fn num(&self, k: &str, default: i64) -> i64 {
        match self.opt.get(k) {
            Some(v) => v.parse().unwrap_or_else(|_| die(&format!("--{k} phải là số nguyên: {v}"))),
            None => default,
        }
    }
}

pub fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("server") => server::run(Args::parse(rest)),
        Some("choi") => nguoi_choi::run(Args::parse(rest)),
        _ => die("Cách dùng: mr3_3_game_doan_so server [host] [port] [--so-nguoi N] ... | choi <tên> [host] [port] [--bot KIỂU]"),
    }
}
