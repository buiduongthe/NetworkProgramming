//! Bài mở rộng MR6.1 – Tắt server an toàn (graceful shutdown).
//!
//! Chạy (một chương trình, hai vai trò):
//!   cargo run -q --release -- server [host] [port] [--han-cho-ms 2000]
//!   cargo run -q --release -- khach <tên> <host> <port> [ms ...]

mod khach;
mod server;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("server") => server::run(rest),
        Some("khach") if rest.len() >= 3 => khach::run(rest),
        _ => {
            eprintln!("Cách dùng: mr6_1_tat_server_an_toan server [host] [port] [--han-cho-ms 2000] | khach <tên> <host> <port> [ms ...]");
            std::process::exit(2);
        }
    }
}
