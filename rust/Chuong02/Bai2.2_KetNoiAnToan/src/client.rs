//! Client minh họa Mã giả 2.2: gọi SAFE_CONNECT và in kết quả hoặc loại lỗi.

use crate::endpoint::parse_endpoint;
use crate::safe_connect::{safe_connect, ConnectError};
use std::time::Instant;

pub fn run(args: &[String]) -> i32 {
    let (host, port) = match parse_endpoint(args, "127.0.0.1", 9000) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[client] Lỗi cấu hình: {e}");
            return 2;
        }
    };
    let timeout_ms: u64 = if args.len() >= 3 {
        match args[2].trim().parse() {
            Ok(v) if v > 0 => v,
            _ => {
                eprintln!("[client] Lỗi cấu hình: Thời gian chờ không hợp lệ: {}", args[2]);
                return 2;
            }
        }
    } else {
        3000
    };
    println!("[client] Kết nối tới {host}:{port}, thời gian chờ {timeout_ms} ms");
    let t0 = Instant::now();
    let s = match safe_connect(&host, port, timeout_ms) {
        Ok(s) => s,
        Err(ConnectError::Net(e)) => {
            let ms = t0.elapsed().as_millis();
            let retry = if e.retry { "có" } else { "không" };
            println!("[client] NetError {}, thử lại: {retry}, sau {ms} ms", e.kind);
            println!("[client] Nguyên nhân: {:?}: {}", e.cause.kind(), e.cause);
            return 1;
        }
        Err(ConnectError::Other(e)) => {
            println!("[client] Lỗi khác (không phân loại): {:?}: {e}", e.kind());
            return 1;
        }
    };
    let ms = t0.elapsed().as_millis();
    // nơi gọi chịu trách nhiệm đóng socket: `s` được đóng khi ra khỏi khối này
    {
        let local = s.local_addr().map(|a| a.to_string()).unwrap_or_default();
        let peer = s.peer_addr().map(|a| a.to_string()).unwrap_or_default();
        println!("[client] Thành công: {local} -> {peer}, sau {ms} ms");
        drop(s); // CLOSE(s)
    }
    println!("[client] Đã đóng kết nối");
    0
}
