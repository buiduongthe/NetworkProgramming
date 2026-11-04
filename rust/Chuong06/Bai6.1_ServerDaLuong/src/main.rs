//! Bài 6.1 – Server tra cứu đa luồng (Mã giả 6.1) và server nhóm luồng (Mã giả 6.2); Mục 6.2.1–6.2.2
//! Chạy:  cargo run -- threaded [host] [port] [max_clients]   (mặc định 0.0.0.0 9000 500)
//!        cargo run -- pool [host] [port] [k] [queue_size]     (mặc định 0.0.0.0 9000 8 16)
//!        cargo run -- client [host] [port] [n] [hold_ms]      (mặc định 127.0.0.1 9000 5 1000)
mod lookup_service;
mod multi_client;
mod netutil;
mod pool_server;
mod thread_server;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    match mode {
        "threaded" => thread_server::run(rest),
        "pool" => pool_server::run(rest),
        "client" => multi_client::run(rest),
        _ => {
            println!("Cách dùng: cargo run -- threaded|pool|client [host] [port] [...]");
            std::process::exit(1);
        }
    }
}
