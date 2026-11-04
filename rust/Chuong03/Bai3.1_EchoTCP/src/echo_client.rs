//! TCP Echo Client (phiên bản đầu, chưa định khung) – Mã giả 3.2 (Mục 3.2.3, Giáo trình Lập trình mạng).
//! Đọc từng dòng từ bàn phím, gửi tới server và in phản hồi.
//! Lưu ý (như trong giáo trình): lời gọi read() giả định nhận đủ phản hồi trong một lần;
//! điều này không đúng với thông điệp lớn và được khắc phục ở Bài 3.2 (định khung).

use crate::netutil::{parse_endpoint, safe_connect};
use std::io::{self, BufRead, Read, Write};
use std::net::Shutdown;
use std::process;
use std::time::Duration;

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "127.0.0.1", 9000);
    let mut s = match safe_connect(&host, port, 3000) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[client] {e}");
            process::exit(1);
        }
    };
    s.set_read_timeout(Some(Duration::from_millis(5000))).ok(); // RECV_TIMEOUT 5000 ms
    println!("[client] Đã kết nối {host}:{port}. Nhập dòng cần gửi (Ctrl+D để kết thúc).");
    if let Err(e) = echo_loop(&mut s) {
        println!("[client] Lỗi: {e}");
    }
    drop(s); // CLOSE(s)
    println!("[client] Đã đóng kết nối");
}

fn echo_loop(s: &mut std::net::TcpStream) -> io::Result<()> {
    let mut buf = [0u8; 4096];
    for line in io::stdin().lock().lines() {
        // READ_CONSOLE()
        let line = line?;
        s.write_all(line.as_bytes())?; // SEND_ALL(ENCODE(line, "UTF-8")) – chuỗi Rust vốn là UTF-8
        let n = s.read(&mut buf)?; // giả định nhận đủ một lần
        if n == 0 {
            break;
        }
        let reply = String::from_utf8_lossy(&buf[..n]);
        println!("[client] Gửi: '{line}' -> Nhận: '{reply}'");
    }
    s.shutdown(Shutdown::Write)?; // SHUTDOWN(WRITE): báo hết dữ liệu cho server
    Ok(())
}
