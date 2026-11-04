//! Mã giả 2.1 – PARSE_ENDPOINT (chép từ Bài 2.1): tham số dòng lệnh > biến môi trường
//! APP_HOST/APP_PORT > giá trị mặc định. Cấu hình sai thì in thông báo và kết thúc chương trình.

use std::env;
use std::process;

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
