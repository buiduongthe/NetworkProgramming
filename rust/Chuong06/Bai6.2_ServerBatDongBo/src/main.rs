//! Bài 6.2 – Server tra cứu bất đồng bộ (Mã giả 6.3); Mục 6.2.3
//! Chạy:  cargo run -- server [host] [port]               (mặc định 0.0.0.0 9000)
//!        cargo run -- client [host] [port] [n] [hold_ms] (mặc định 127.0.0.1 9000 5 1000)
mod async_frame;
mod async_server;
mod multi_client;
mod netutil;

/// In một dòng trọn vẹn (println! khóa stdout nên các luồng không chen giữa dòng của nhau).
pub fn log(msg: &str) {
    println!("{msg}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    match mode {
        // server chạy trên môi trường bất đồng bộ của tokio (nhóm luồng thợ nhỏ)
        "server" => tokio::runtime::Runtime::new().unwrap().block_on(async_server::run(rest)),
        // client dùng socket đồng bộ và luồng như Bài 6.1
        "client" => multi_client::run(rest),
        _ => {
            println!("Cách dùng: cargo run -- server|client [host] [port] [...]");
            std::process::exit(1);
        }
    }
}
