//! Bài 6.7 – Kiểm thử tải (Mã giả 6.9) với bản sao server Mã giả 6.1, 6.2; Mục 6.5.1
//! Chạy:  cargo run -- threaded [host] [port] [max_clients] [delay_ms]   (mặc định 0.0.0.0 9000 500 50)
//!        cargo run -- pool [host] [port] [k] [queue_size] [delay_ms]    (mặc định 0.0.0.0 9000 8 16 50)
//!        cargo run -- load [host] [port] [n_requests] [levels]          (mặc định 127.0.0.1 9000 20 1,10,100)
mod async_frame;
mod load_test;
mod lookup_service;
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
        // bộ sinh tải chạy trên môi trường bất đồng bộ của tokio
        "load" => tokio::runtime::Runtime::new().unwrap().block_on(load_test::run(rest)),
        _ => {
            println!("Cách dùng: cargo run -- threaded|pool|load [host] [port] [...]");
            std::process::exit(1);
        }
    }
}
