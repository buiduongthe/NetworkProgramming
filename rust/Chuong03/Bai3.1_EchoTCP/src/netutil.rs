//! Tiện ích kế thừa từ Chương 2, chép gọn vào bài để bài tự chứa.
//! - parse_endpoint: Mã giả 2.1 (PARSE_ENDPOINT, rút gọn: chỉ đọc APP_PORT)
//! - safe_connect:   Mã giả 2.2 (SAFE_CONNECT, rút gọn: không phân loại lỗi)

use std::env;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::process;
use std::time::Duration;

/// Mã giả 2.1: ưu tiên tham số dòng lệnh > biến môi trường APP_PORT > giá trị mặc định.
pub fn parse_endpoint(args: &[String], default_host: &str, default_port: u16) -> (String, u16) {
    let mut host = default_host.to_string();
    let mut port = env::var("APP_PORT").unwrap_or_else(|_| default_port.to_string());
    if args.len() >= 1 {
        host = args[0].clone();
    }
    if args.len() >= 2 {
        port = args[1].clone();
    }
    let p: i64 = match port.trim().parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Cổng phải là số nguyên: {port}");
            process::exit(1);
        }
    };
    if !(1..=65535).contains(&p) {
        eprintln!("Cổng không hợp lệ: {p}");
        process::exit(1);
    }
    (host, p as u16)
}

/// Mã giả 2.2 (rút gọn): thử lần lượt các địa chỉ, có thời gian chờ kết nối.
pub fn safe_connect(host: &str, port: u16, timeout_ms: u64) -> io::Result<TcpStream> {
    let mut last_error = io::Error::new(io::ErrorKind::NotFound, "không có địa chỉ nào");
    for addr in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)) {
            Ok(s) => return Ok(s),
            Err(e) => last_error = e, // socket hỏng tự đóng khi ra khỏi phạm vi
        }
    }
    Err(io::Error::new(last_error.kind(), format!("Không kết nối được {host}:{port}: {last_error}")))
}
