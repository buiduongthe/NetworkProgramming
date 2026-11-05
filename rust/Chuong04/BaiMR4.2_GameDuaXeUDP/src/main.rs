//! Bài mở rộng MR4.2 – Đua xe thời gian thực qua UDP: tick có thẩm quyền, ảnh chụp, chịu mất gói.
//!
//! Chạy (một chương trình, hai vai trò):
//!   cargo run -q --release -- server [host] [port] [--so-xe 2] [--mat 0] [--dao 0] [--seed 1] [--so-van 0]
//!   cargo run -q --release -- xe <tên> [host] [port] [--bot lieu|can_than] [--khung 10] [--so-van 1]

mod dua_xe;
mod server;
mod xe;

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
    pub fn num(&self, k: &str, default: i64) -> i64 {
        match self.opt.get(k) {
            Some(v) => v.parse().unwrap_or_else(|_| die(&format!("--{k} phải là số nguyên: {v}"))),
            None => default,
        }
    }
    pub fn text(&self, k: &str, default: &str) -> String {
        self.opt.get(k).cloned().unwrap_or_else(|| default.to_string())
    }
    pub fn port(&self, i: usize, default: u16) -> u16 {
        self.pos.get(i).map(|p| p.parse().unwrap_or_else(|_| die(&format!("Cổng sai: {p}")))).unwrap_or(default)
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
        Some("xe") => xe::run(Args::parse(rest)),
        _ => die("Cách dùng: mr4_2_game_dua_xe_udp server [host] [port] [tùy chọn] | xe <tên> [host] [port] [tùy chọn]"),
    }
}
