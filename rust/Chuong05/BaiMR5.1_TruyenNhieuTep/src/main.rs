//! Bài mở rộng MR5.1 – Truyền nhiều tệp đồng thời có điều khiển start/pause/resume/stop,
//! giới hạn tốc độ theo phiên và theo tệp (thùng token), tải tiếp bằng offset + kiểm SHA-256.
//!
//! Vai:
//!   server <host> <port> [thư_mục_lưu]   – server, mỗi kết nối một lượt tải xuống/tải lên.
//!   client <host> <port> [thư_mục_tải]   – giao diện dòng lệnh: nhập lệnh qua bàn phím.
//!   demo   <host> <port>                 – tự dựng server cục bộ, tạo tệp thử, chạy kịch bản tự động.

mod client;
mod khung;
mod server;

use std::process;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("Cách dùng: mr5_1_truyen_nhieu_tep server|client|demo <host> <port> [thư_mục]");
        process::exit(2);
    }
    let (vai, host) = (args[0].as_str(), args[1].clone());
    let port: u16 = args[2].parse().unwrap_or_else(|_| {
        eprintln!("Cổng sai: {}", args[2]);
        process::exit(2)
    });
    match vai {
        "server" => server::run(&host, port, args.get(3).map(String::as_str).unwrap_or("kho_server")),
        "client" => client::chay_giao_dien(&host, port, args.get(3).map(String::as_str).unwrap_or("tai_ve")),
        "demo" => client::chay_demo(&host, port),
        _ => {
            eprintln!("Vai phải là server, client hoặc demo");
            process::exit(2);
        }
    }
}
