//! Tiện ích kết nối kế thừa từ Chương 2, chép gọn vào bài để bài tự chứa.
//!
//! - parse_endpoint: Mã giả 2.1 (PARSE_ENDPOINT, Mục 2.1.3)
//! - safe_connect:   Mã giả 2.2 (SAFE_CONNECT, Mục 2.2)
#![allow(dead_code)] // thư viện chép nguyên: một số hàm không dùng trong bài này

use std::env;
use std::fmt;
use std::io::{self, ErrorKind};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::process;
use std::time::Duration;

/// Lỗi mạng đã phân loại: kind ∈ {NOT_FOUND, REFUSED, UNREACHABLE}, retry = có nên thử lại.
#[derive(Debug)]
pub struct NetError {
    pub kind: &'static str,
    pub retry: bool,
    pub cause: io::Error,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let retry = if self.retry { "có" } else { "không" };
        write!(f, "{} (thử lại: {retry}): {}", self.kind, self.cause)
    }
}

/// Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > giá trị mặc định.
/// Cấu hình sai (ConfigError) thì in thông báo và kết thúc chương trình.
pub fn parse_endpoint(args: &[String], default_host: &str, default_port: u16) -> (String, u16) {
    let mut host = env::var("APP_HOST").unwrap_or_else(|_| default_host.to_string());
    let mut port = env::var("APP_PORT").unwrap_or_else(|_| default_port.to_string());
    if args.len() >= 1 {
        host = args[0].clone();
    }
    if args.len() >= 2 {
        port = args[1].clone();
    }
    let p: i64 = match port.trim().parse() {
        // TO_INT
        Ok(v) => v,
        Err(_) => {
            eprintln!("Cổng phải là số nguyên: {port}"); // ConfigError
            process::exit(1);
        }
    };
    if !(1..=65535).contains(&p) {
        eprintln!("Cổng không hợp lệ: {p}");
        process::exit(1);
    }
    (host, p as u16)
}

/// Mã giả 2.2: thử lần lượt các địa chỉ, có thời gian chờ, phân loại lỗi.
pub fn safe_connect(host: &str, port: u16, timeout_ms: u64) -> Result<TcpStream, NetError> {
    let addrs: Vec<SocketAddr> = match (host, port).to_socket_addrs() {
        // RESOLVE
        Ok(iter) => iter.collect(),
        Err(e) => return Err(NetError { kind: "NOT_FOUND", retry: false, cause: e }),
    };
    let mut refused = 0;
    let mut last_error = io::Error::new(ErrorKind::NotFound, "không có địa chỉ nào");
    for a in &addrs {
        // CONNECT_TIMEOUT + CONNECT: bắt tay ba bước
        match TcpStream::connect_timeout(a, Duration::from_millis(timeout_ms)) {
            Ok(s) => return Ok(s),
            Err(e) => {
                if e.kind() == ErrorKind::ConnectionRefused {
                    refused += 1; // nhận RST
                }
                last_error = e; // Timeout, Unreachable...; socket hỏng tự đóng
            }
        }
    }
    if refused > 0 && refused == addrs.len() {
        return Err(NetError { kind: "REFUSED", retry: false, cause: last_error });
    }
    Err(NetError { kind: "UNREACHABLE", retry: true, cause: last_error })
}
