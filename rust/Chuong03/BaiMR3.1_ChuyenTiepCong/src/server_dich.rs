//! Server đích để thử bộ chuyển tiếp cổng.
//!
//! Dịch vụ "chữ hoa": đọc toàn bộ dữ liệu tới khi client đóng chiều gửi (read trả 0),
//! rồi gửi lại bản viết hoa và đóng kết nối. Vì server chỉ trả lời SAU KHI thấy EOF,
//! bộ chuyển tiếp ở giữa bắt buộc phải chuyển tiếp đúng việc đóng một chiều (half-close);
//! nếu không, client và server sẽ chờ nhau mãi.

use crate::{loi, port_arg};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process;
use std::thread;

fn handle(mut conn: TcpStream) -> io::Result<()> {
    let addr = conn.peer_addr()?;
    let mut body = Vec::new();
    conn.read_to_end(&mut body)?; // đọc tới khi client SHUTDOWN(WRITE)
    let reply = String::from_utf8_lossy(&body).to_uppercase().into_bytes();
    conn.write_all(&reply)?;
    println!("[dich] {addr} gửi {} byte, trả {} byte chữ hoa", body.len(), reply.len());
    Ok(()) // conn tự đóng khi ra khỏi hàm
}

pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let port = args.get(1).map(|p| port_arg(p)).unwrap_or(9000);
    let srv = TcpListener::bind((host.as_str(), port)).unwrap_or_else(|e| {
        eprintln!("[dich] Không mở được {host}:{port}: {}", loi(&e));
        process::exit(1)
    });
    ctrlc::set_handler(|| {
        println!("[dich] Dừng server");
        process::exit(0);
    })
    .expect("Không cài được hàm bắt Ctrl+C");
    println!("[dich] Server chữ hoa lắng nghe tại {host}:{port}");
    for conn in srv.incoming().flatten() {
        thread::spawn(move || {
            if let Err(e) = handle(conn) {
                println!("[dich] Lỗi: {}", loi(&e));
            }
        });
    }
}
