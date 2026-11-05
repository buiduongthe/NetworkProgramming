//! Bài mở rộng MR7.1 – Tải tệp HTTP có tải tiếp (Range, If-Range).
//!
//! Chạy (một chương trình, hai vai trò):
//!   cargo run -q --release -- server [host] [port] [--kich-thuoc B] [--cat-sau B] [--so-lan-cat N]
//!   cargo run -q --release -- tai <url> <tệp_lưu> [--so-lan-thu N]
//! HTTP/1.1 được viết tay trên std::net::TcpStream (không dùng crate HTTP) để thấy rõ các tiêu đề.

mod http;
mod server;
mod tai_tiep;

use std::collections::HashMap;

/// Tách tham số vị trí và tùy chọn "--khóa giá_trị".
pub fn split_args(args: &[String]) -> (Vec<String>, HashMap<String, String>) {
    let (mut pos, mut opt) = (Vec::new(), HashMap::new());
    let mut i = 0;
    while i < args.len() {
        if let Some(k) = args[i].strip_prefix("--") {
            opt.insert(k.to_string(), args.get(i + 1).cloned().unwrap_or_default());
            i += 2;
        } else {
            pos.push(args[i].clone());
            i += 1;
        }
    }
    (pos, opt)
}

pub fn sha256_hex(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("server") => server::run(rest),
        Some("tai") => tai_tiep::run(rest),
        _ => {
            eprintln!("Cách dùng: mr7_1_tai_tiep_http server [host] [port] [tùy chọn] | tai <url> <tệp_lưu> [--so-lan-thu N]");
            std::process::exit(2);
        }
    }
}
