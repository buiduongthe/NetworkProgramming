//! Mã giả 2.1 – PARSE_ENDPOINT (rút gọn, chép từ Bài 2.1):
//! tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > giá trị mặc định.

use std::env;
use std::fmt;

/// Lỗi cấu hình (Mã giả 2.1).
#[derive(Debug)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn parse_endpoint(args: &[String], default_host: &str, default_port: u16) -> Result<(String, u16), ConfigError> {
    let mut host = env::var("APP_HOST").unwrap_or_else(|_| default_host.to_string());
    let mut port = env::var("APP_PORT").unwrap_or_else(|_| default_port.to_string());
    if args.len() >= 1 {
        host = args[0].clone();
    }
    if args.len() >= 2 {
        port = args[1].clone();
    }
    let p: i64 = match port.trim().parse() {
        Ok(v) => v,
        Err(_) => return Err(ConfigError(format!("Cổng phải là số nguyên: {port}"))),
    };
    if p < 1 || p > 65535 {
        return Err(ConfigError(format!("Cổng không hợp lệ: {p}")));
    }
    Ok((host, p as u16))
}
