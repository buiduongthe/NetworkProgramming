//! TCP Echo Server tuần tự – Mã giả 3.1 (Mục 3.2.2, Giáo trình Lập trình mạng).
//! Server nhận dữ liệu từ client và gửi trả nguyên văn; phục vụ lần lượt từng client.

use crate::netutil::parse_endpoint;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process;

/// Vòng phục vụ một client: nhận bao nhiêu gửi trả bấy nhiêu.
fn handle_echo(conn: &mut TcpStream) -> io::Result<()> {
    let mut buf = [0u8; 4096];
    loop {
        let n = conn.read(&mut buf)?; // RECV: trả về 0 khi client đã đóng chiều gửi
        if n == 0 {
            return Ok(());
        }
        conn.write_all(&buf[..n])?; // SEND_ALL: gửi hết, kể cả khi SEND chỉ gửi được một phần
    }
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9000);
    // SOCKET(INET, STREAM) + SETOPT(REUSEADDR) + BIND + LISTEN: TcpListener::bind làm cả bốn bước.
    // Thư viện chuẩn tự bật SO_REUSEADDR trên Linux/macOS và tự chọn backlog (không đặt được 16).
    let srv = match TcpListener::bind((host.as_str(), port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[server] Không mở được {host}:{port}: {e}");
            process::exit(1);
        }
    };
    println!("[server] Đang lắng nghe tại {host}:{port}");
    loop {
        // vòng chấp nhận kết nối
        let (mut conn, addr) = match srv.accept() {
            // ACCEPT: chờ client mới
            Ok(v) => v,
            Err(e) => {
                println!("[server] Lỗi accept: {e}");
                continue;
            }
        };
        println!("[server] Client kết nối: {addr}");
        if let Err(e) = handle_echo(&mut conn) {
            println!("[server] Lỗi với {addr}: {e}"); // lỗi của một client không dừng server
        }
        drop(conn); // CLOSE(conn)
        println!("[server] Client {addr} đã đóng kết nối");
    }
}
