//! Bài 1.2 – Kiểm tra trạng thái một cổng TCP.
//!
//! Cài đặt Mã giả 1.2 – PORT_CHECK (Mục 1.2.5, Giáo trình Lập trình mạng),
//! kế thừa Mã giả 1.1 – RESOLVE_HOST (Mục 1.2.3).
//! Kết quả là một trong bốn trạng thái: OPEN, CLOSED, FILTERED, UNRESOLVED.
//! Chỉ kiểm tra trên máy và mạng thuộc quyền quản lý của người thực hiện.
//!
//! Chạy:  cargo run -q --release -- <host> <port> [timeout_ms]      (mặc định timeout_ms = 2000)
//! Ví dụ: cargo run -q --release -- localhost 9000

use std::env;
use std::io::{self, ErrorKind};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::process;
use std::time::Duration;

/// FAMILY(a): đổi họ địa chỉ thành tên dùng trong mã giả.
fn family_name(a: &IpAddr) -> &'static str {
    match a {
        IpAddr::V4(_) => "INET",
        IpAddr::V6(_) => "INET6",
    }
}

/// Mã giả 1.1 – RESOLVE_HOST(name) (chép từ Bài 1.1): danh sách địa chỉ; rỗng nếu lỗi.
fn resolve_host(name: &str) -> Vec<IpAddr> {
    match (name, 0).to_socket_addrs() {
        // RESOLVE(name)
        Ok(iter) => {
            let addrs: Vec<IpAddr> = iter.map(|sa| sa.ip()).collect();
            for a in &addrs {
                println!("  {:<6} {}", family_name(a), a); // PRINT(FAMILY(a), ADDRESS(a))
            }
            addrs
        }
        Err(e) => {
            // CATCH ResolveError AS e
            println!("[port-check] Không phân giải được {name} ({e})");
            Vec::new() // RETURN EMPTY_LIST
        }
    }
}

/// Mã giả 1.2 – PORT_CHECK(host, port, timeout_ms).
/// Lỗi khác ba loại mã giả bắt được trả về `Err` cho nơi gọi (như mã giả không bắt chúng).
fn port_check(host: &str, port: u16, timeout_ms: u64) -> io::Result<&'static str> {
    let addrs = resolve_host(host); // dòng 2: lỗi phân giải → rỗng
    if addrs.is_empty() {
        return Ok("UNRESOLVED"); // dòng 3
    }
    let mut state = "FILTERED"; // dòng 4: chưa địa chỉ nào phản hồi
    for a in addrs {
        // dòng 5: FOR EACH a IN addrs
        let target = SocketAddr::new(a, port);
        // dòng 6–9: SOCKET(FAMILY(a), STREAM), SETOPT(CONNECT_TIMEOUT), CONNECT – bắt tay ba bước.
        // connect_timeout tạo socket đúng họ địa chỉ của `target` và chờ tối đa timeout_ms.
        match TcpStream::connect_timeout(&target, Duration::from_millis(timeout_ms)) {
            Ok(_s) => {
                println!("[port-check] {a} cổng {port}: kết nối thành công");
                return Ok("OPEN"); // dòng 10; socket _s tự đóng khi ra khỏi phạm vi (dòng 16)
            }
            Err(e) if e.kind() == ErrorKind::ConnectionRefused => {
                // dòng 11: CATCH ConnectionRefused
                println!("[port-check] {a} cổng {port}: bị từ chối (RST)");
                state = "CLOSED"; // dòng 12
            }
            Err(e) if e.kind() == ErrorKind::TimedOut => {
                // dòng 13: CATCH Timeout
                println!("[port-check] {a} cổng {port}: hết thời gian chờ");
                continue; // dòng 14
            }
            Err(e)
                if e.kind() == ErrorKind::NetworkUnreachable
                    || e.kind() == ErrorKind::HostUnreachable =>
            {
                // dòng 13: NetworkUnreachable, HostUnreachable
                println!("[port-check] {a} cổng {port}: không có đường đi ({e})");
                continue;
            }
            Err(e) => return Err(e), // lỗi khác: để thoát ra như mã giả
        }
        // dòng 16: FINALLY CLOSE(s) – Rust tự đóng socket khi biến ra khỏi phạm vi
    }
    Ok(state) // dòng 19: CLOSED hoặc FILTERED
}

/// Đổi chuỗi thành số nguyên, báo lỗi rõ ràng nếu sai.
fn to_number(text: &str, what: &str) -> i64 {
    match text.trim().parse::<i64>() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("{what} phải là số nguyên: {text}");
            process::exit(2);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("Cách dùng: cargo run -q --release -- <host> <port> [timeout_ms]");
        process::exit(1);
    }
    let host = &args[0];
    let port = to_number(&args[1], "Cổng");
    if !(1..=65535).contains(&port) {
        eprintln!("Cổng không hợp lệ: {port}");
        process::exit(2);
    }
    let timeout_ms = if args.len() > 2 { to_number(&args[2], "Thời gian chờ") } else { 2000 };
    if timeout_ms < 1 {
        eprintln!("Thời gian chờ không hợp lệ: {timeout_ms}");
        process::exit(2);
    }
    println!("[port-check] Kiểm tra {host}:{port} (thời gian chờ {timeout_ms} ms)");
    match port_check(host, port as u16, timeout_ms as u64) {
        Ok(state) => println!("[port-check] Kết quả: {host}:{port} -> {state}"),
        Err(e) => {
            println!("[port-check] Lỗi không thuộc ba loại của mã giả: {e}");
            process::exit(1);
        }
    }
}
