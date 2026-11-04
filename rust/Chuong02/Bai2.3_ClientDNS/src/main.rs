//! Bài 2.3 – Client tra cứu DNS dùng bộ phân giải của hệ điều hành.
//!
//! Cài đặt Mã giả 2.3 (Mục 2.5, Giáo trình Lập trình mạng), kế thừa Mã giả 1.1 RESOLVE_HOST.
//! Mỗi tham số là một tên miền (tra cứu xuôi: tên → địa chỉ IPv4/IPv6)
//! hoặc một địa chỉ IP (tra cứu ngược: địa chỉ → tên, bản ghi PTR).
//!
//! Chạy:  cargo run -q --release -- <tên miền | địa chỉ IP> ...
//! Ví dụ: cargo run -q --release -- localhost 127.0.0.1 8.8.8.8

use std::env;
use std::net::{IpAddr, ToSocketAddrs};
use std::time::Instant;

/// Mã giả 1.1 – RESOLVE_HOST: in và trả về danh sách địa chỉ; lỗi → danh sách rỗng.
fn resolve_host(name: &str) -> Vec<IpAddr> {
    // RESOLVE = getaddrinfo của hệ điều hành (kiểu SOCK_STREAM: mỗi địa chỉ xuất hiện một lần)
    match (name, 0).to_socket_addrs() {
        Ok(iter) => {
            let addrs: Vec<IpAddr> = iter.map(|sa| sa.ip()).collect();
            for a in &addrs {
                let family = if a.is_ipv4() { "INET" } else { "INET6" };
                println!("  {family}  {a}"); // FAMILY(a), ADDRESS(a)
            }
            addrs
        }
        Err(e) => {
            // ResolveError
            println!("  Không phân giải được {name} ({e})");
            Vec::new()
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        // dòng 1–5
        println!("Cách dùng: cargo run -q --release -- <tên miền | địa chỉ IP> ...");
        return;
    }
    for q in &args {
        // dòng 6
        let t0 = Instant::now(); // dòng 7: t0 ← NOW()
        match q.parse::<IpAddr>() {
            // dòng 8: IS_IP_ADDRESS(q)
            Ok(ip) => {
                println!("[dns] Tra cứu ngược: {q}");
                // REVERSE_RESOLVE: bản ghi PTR (getnameinfo với cờ NI_NAMEREQD, qua crate dns-lookup)
                match dns_lookup::lookup_addr(&ip) {
                    Ok(name) => println!("  {q} → {name}"),
                    Err(_) => println!("  {q} → không có bản ghi PTR"), // ResolveError
                }
            }
            Err(_) => {
                println!("[dns] Tra cứu xuôi: {q}");
                resolve_host(q); // = Mã giả 1.1
            }
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("  Thời gian: {ms:.2} ms"); // dòng 18
    }
}
