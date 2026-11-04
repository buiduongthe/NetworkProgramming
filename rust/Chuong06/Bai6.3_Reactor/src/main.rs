//! Bài 6.3 – Server echo một luồng theo mô hình reactor (Mã giả 6.4); Mục 6.2.4
//! Chạy:  cargo run -- server [host] [port]                 (mặc định 0.0.0.0 9000)
//!        cargo run -- client [host] [port] [n] [size]      (mặc định 127.0.0.1 9000 3 1048576)
mod echo_clients;
mod netutil;
mod reactor_server;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    match mode {
        "server" => reactor_server::run(rest),
        "client" => echo_clients::run(rest),
        _ => {
            println!("Cách dùng: cargo run -- server|client [host] [port] [...]");
            std::process::exit(1);
        }
    }
}
