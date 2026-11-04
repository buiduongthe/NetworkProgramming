//! Bài 1.1 – Phân giải tên miền thành danh sách địa chỉ.
//!
//! Cài đặt Mã giả 1.1 – RESOLVE_HOST (Mục 1.2.3, Giáo trình Lập trình mạng).
//! Với mỗi tên nhận từ dòng lệnh, chương trình gọi getaddrinfo của hệ điều hành
//! (qua `ToSocketAddrs` của thư viện chuẩn), in từng địa chỉ kèm họ địa chỉ (INET/INET6);
//! lỗi phân giải được bắt và trả danh sách rỗng.
//!
//! Chạy:  cargo run -q --release -- [tên ...]      (mặc định: localhost)
//! Ví dụ: cargo run -q --release -- localhost example.com

use std::env;
use std::net::{IpAddr, ToSocketAddrs};

/// FAMILY(a): đổi họ địa chỉ thành tên dùng trong mã giả.
fn family_name(a: &IpAddr) -> &'static str {
    match a {
        IpAddr::V4(_) => "INET",
        IpAddr::V6(_) => "INET6",
    }
}

/// Mã giả 1.1 – RESOLVE_HOST(name): trả về danh sách địa chỉ; rỗng nếu lỗi.
fn resolve_host(name: &str) -> Vec<IpAddr> {
    // RESOLVE(name): (name, 0).to_socket_addrs() gọi getaddrinfo với kiểu SOCK_STREAM,
    // nên mỗi địa chỉ chỉ xuất hiện một lần (cổng 0 vì chỉ cần địa chỉ)
    match (name, 0).to_socket_addrs() {
        Ok(iter) => {
            let addrs: Vec<IpAddr> = iter.map(|sa| sa.ip()).collect();
            for a in &addrs {
                // FOR EACH a IN addrs: PRINT(FAMILY(a), ADDRESS(a))
                println!("  {:<6} {}", family_name(a), a);
            }
            addrs // RETURN addrs
        }
        Err(e) => {
            // CATCH ResolveError AS e
            println!("[resolve] Không phân giải được {name} ({e})");
            Vec::new() // RETURN EMPTY_LIST
        }
    }
}

fn main() {
    let mut names: Vec<String> = env::args().skip(1).collect();
    if names.is_empty() {
        names.push("localhost".to_string());
    }
    for name in &names {
        println!("[resolve] Phân giải tên: {name}");
        let addrs = resolve_host(name);
        println!("[resolve] {name} -> {} địa chỉ", addrs.len());
    }
}
