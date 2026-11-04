//! Bài 6.3 – Các thủ tục kế thừa dùng chung (chép gọn từ Chương 2 và Chương 3).
//!
//! - parse_endpoint: Mã giả 2.1 (Mục 2.1.3)
//! - safe_connect:   Mã giả 2.2 (rút gọn, Mục 2.2)
//! - recv_exact:     Mã giả 3.4 (Mục 3.4.3)
use std::fmt;
use std::io::{self, Read};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Các loại lỗi khi nhận dữ liệu.
#[derive(Debug)]
pub enum NetError {
    /// Đối tác đóng kết nối; giá trị = số byte của khung đang nhận dở.
    ConnectionClosed(usize),
    /// Lỗi mạng của hệ điều hành (gồm hết thời gian chờ).
    Io(io::Error),
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetError::ConnectionClosed(n) => write!(f, "Kết nối bị đóng sau {n} byte"),
            NetError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for NetError {
    fn from(e: io::Error) -> Self {
        NetError::Io(e)
    }
}

/// In thông báo lỗi rồi thoát chương trình (giống sys.exit của Python).
pub fn exit_with(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

/// Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > mặc định.
pub fn parse_endpoint(args: &[String], default_host: &str, default_port: u16) -> (String, u16) {
    let mut host = std::env::var("APP_HOST").unwrap_or(default_host.to_string());
    let mut port = std::env::var("APP_PORT").unwrap_or(default_port.to_string());
    if args.len() >= 1 {
        host = args[0].clone();
    }
    if args.len() >= 2 {
        port = args[1].clone();
    }
    let p: i64 = match port.trim().parse() {
        Ok(p) => p,
        Err(_) => exit_with(&format!("Cổng phải là số nguyên: {port}")),
    };
    if !(1..=65535).contains(&p) {
        exit_with(&format!("Cổng không hợp lệ: {p}"));
    }
    (host, p as u16)
}

/// Đọc tham số nguyên thứ i (nếu có), ngược lại dùng giá trị mặc định.
pub fn int_arg(args: &[String], i: usize, default: u64) -> u64 {
    match args.get(i) {
        Some(s) => s
            .parse()
            .unwrap_or_else(|_| exit_with(&format!("Tham số phải là số nguyên: {s}"))),
        None => default,
    }
}

/// Mã giả 2.2 (rút gọn): thử lần lượt các địa chỉ, có thời gian chờ kết nối.
pub fn safe_connect(host: &str, port: u16, timeout_ms: u64) -> Result<TcpStream, String> {
    let mut last_error = String::from("không phân giải được tên");
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("Không kết nối được {host}:{port}: {e}"))?;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)) {
            Ok(s) => return Ok(s),
            Err(e) => last_error = e.to_string(),
        }
    }
    Err(format!("Không kết nối được {host}:{port}: {last_error}"))
}

/// Mã giả 3.4: gọi RECV lặp lại đến khi đủ n byte.
pub fn recv_exact(s: &mut TcpStream, n: usize) -> Result<Vec<u8>, NetError> {
    let mut buf = vec![0u8; n];
    let mut got = 0;
    while got < n {
        let k = s.read(&mut buf[got..])?; // RECV
        if k == 0 {
            return Err(NetError::ConnectionClosed(got));
        }
        got += k;
    }
    Ok(buf)
}
