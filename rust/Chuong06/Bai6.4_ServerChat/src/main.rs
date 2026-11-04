//! Bài 6.4 – Server chat nhiều phòng (Mã giả 6.5, 6.6) và client dòng lệnh (Mã giả 5.5); Mục 6.3
//! Chạy:  cargo run -- server [host] [port]          (mặc định 0.0.0.0 9200)
//!        cargo run -- client [host] [port] [tên]    (mặc định 127.0.0.1 9200 user)
mod async_frame;
mod chat_broadcast;
mod chat_client;
mod chat_server;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    match mode {
        "server" => chat_server::run(rest).await,
        "client" => chat_client::run(rest).await,
        _ => {
            println!("Cách dùng: cargo run -- server|client [host] [port] [tên]");
            std::process::exit(1);
        }
    }
}
