//! Server thử nghiệm tối giản cho client của Bài 2.2.
//!
//! Chỉ chấp nhận kết nối, in địa chỉ client rồi đóng ngay; dùng để tạo trường hợp kết nối
//! thành công. Chuỗi lời gọi SOCKET – BIND – LISTEN – ACCEPT được trình bày chi tiết ở Chương 3.

use crate::endpoint::parse_endpoint;
use std::net::TcpListener;

pub fn run(args: &[String]) -> i32 {
    let (host, port) = match parse_endpoint(args, "0.0.0.0", 9000) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[server] Lỗi cấu hình: {e}");
            return 2;
        }
    };
    // SOCKET(INET, STREAM) + SETOPT(REUSEADDR) + BIND + LISTEN: TcpListener::bind làm cả bốn bước
    // (trên Linux/macOS thư viện chuẩn tự bật SO_REUSEADDR)
    let srv = match TcpListener::bind((host.as_str(), port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[server] Không mở được {host}:{port}: {e}");
            return 1;
        }
    };
    println!("[server] Đang lắng nghe tại {host}:{port}");
    loop {
        match srv.accept() {
            // ACCEPT
            Ok((conn, addr)) => {
                println!("[server] Client kết nối: {addr}");
                drop(conn); // CLOSE ngay
            }
            Err(e) => println!("[server] Lỗi accept: {e}"),
        }
    }
}
