//! Bài 8.4 – Bổ sung TLS và xác thực token cho hệ thống chat (Mã giả 8.6; Mục 8.4.5)
//! Chạy:  cargo run -- server [host] [port]                       (mặc định 0.0.0.0 9200)
//!        cargo run -- client <host> <port> <tệp_token> [tệp_CA]
//!        cargo run -- token <sub> <tệp_token> [aud] [thời_hạn_giây]
mod jwt_hs256;
mod khung;
mod tls_chat_client;
mod tls_chat_server;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("server") => tls_chat_server::run(rest).await,
        Some("client") => tls_chat_client::run(rest).await,
        Some("token") => {
            if let Err(e) = jwt_hs256::run(rest) {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        _ => {
            println!("Cách dùng: cargo run -- server|client|token ...");
            std::process::exit(1);
        }
    }
}
