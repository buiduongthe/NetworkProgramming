//! Bài mở rộng MR6.4 – Caro nhiều phòng qua TCP, khám phá server trong LAN (UDP broadcast).
//!
//! Chạy (một chương trình, ba vai trò):
//!   cargo run -q --release -- server [host] [port] [--kham-pha 9999] [--ten Caro] [--het-gio-ms 30000]
//!   cargo run -q --release -- choi <tên> [host] [port] [--tim CỔNG] [--tao P | --vao P | --xem P]
//!                                  [--bot KIỂU] [--seed 1] [--thu-loi] [--so-van 1] [--tre-ms 300]
//!   cargo run -q --release -- tim [cổng_khám_phá] [thời_gian_chờ_ms] [địa_chỉ_broadcast]

mod ban_co;
mod giao_thuc;
mod nguoi_choi;
mod server;

use std::collections::HashMap;
use std::process;

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
                    opt.insert(k.to_string(), String::new());
                    i += 1;
                } else {
                    let v = args.get(i + 1).cloned().unwrap_or_else(|| die(&format!("--{k} thiếu giá trị")));
                    opt.insert(k.to_string(), v);
                    i += 2;
                }
            } else {
                pos.push(args[i].clone());
                i += 1;
            }
        }
        Args { pos, opt }
    }
    pub fn num(&self, k: &str, d: i64) -> i64 {
        match self.opt.get(k) {
            Some(v) => v.parse().unwrap_or_else(|_| die(&format!("--{k} phải là số nguyên: {v}"))),
            None => d,
        }
    }
    pub fn port(&self, i: usize, d: u16) -> u16 {
        self.pos.get(i).map(|p| p.parse().unwrap_or_else(|_| die(&format!("Cổng sai: {p}")))).unwrap_or(d)
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
        Some("server") => server::run(Args::parse(rest, &[])),
        Some("choi") => nguoi_choi::run(Args::parse(rest, &["thu-loi"])),
        Some("tim") => {
            let a = Args::parse(rest, &[]);
            let port = a.port(0, 9999);
            let wait_ms: u64 = a.pos.get(1).and_then(|w| w.parse().ok()).unwrap_or(1000);
            let bcast = a.pos.get(2).cloned().unwrap_or_else(|| "255.255.255.255".into());
            println!("[tim] Phát discover tới {bcast}:{port}, chờ {wait_ms} ms");
            let found = giao_thuc::discover(port, wait_ms, &bcast);
            println!("[tim] Tìm thấy {} server:", found.len());
            for f in found {
                println!("[tim]   {} tại {}:{} – {} phòng, {} người", f.name, f.ip, f.port, f.rooms, f.players);
            }
        }
        _ => die("Cách dùng: mr6_4_game_caro server|choi|tim ..."),
    }
}
