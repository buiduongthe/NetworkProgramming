//! Bài 7.4 – Server Server-Sent Events nhỏ đi kèm bộ đọc luồng.
//!
//! Phát luồng text/event-stream (Mục 7.3.3) bằng mã hóa khối (chunked) vì server không biết trước độ dài.
//! Luồng gồm: chú thích (nhịp tim), retry, hai sự kiện "delta" có id, một sự kiện nhiều dòng data
//! (cố ý chia đôi qua hai khối, cắt giữa dòng) và sự kiện "done"; sau đó gửi khối 0 và đóng kết nối.
//! Mỗi kết nối một luồng (Mã giả 6.1).
//!
//! Chạy:  cargo run -- server [host] [port]      (mặc định 0.0.0.0 8080)
use crate::{exit_with, port_arg};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

const EVENTS: [&str; 7] = [
    ": nhip tim\n\n",
    "retry: 3000\n\n",
    "id: 1\nevent: delta\ndata: {\"text\":\"Giao thức\"}\n\n",
    "id: 2\nevent: delta\ndata:{\"text\":\" TCP\"}\n\n", // không có dấu cách sau ':' cũng hợp lệ
    "id: 3\ndata: dòng thứ nhất\ndata: dòng th",       // sự kiện này bị cắt giữa dòng ...
    "ứ hai\n\n",                                        // ... và kết thúc ở khối sau
    "event: done\ndata: [DONE]\n\n",
];

/// Một khối của Transfer-Encoding: chunked: độ dài hex, CRLF, dữ liệu, CRLF.
fn send_chunk(conn: &mut TcpStream, text: &str) -> io::Result<usize> {
    let b = text.as_bytes();
    let mut out = format!("{:X}\r\n", b.len()).into_bytes();
    out.extend_from_slice(b);
    out.extend_from_slice(b"\r\n");
    conn.write_all(&out)?;
    Ok(b.len())
}

fn stream(conn: &mut TcpStream, addr: &str) -> io::Result<()> {
    let mut head = Vec::new();
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        // đọc dòng yêu cầu và tiêu đề
        let mut chunk = [0u8; 4096];
        let n = conn.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        head.extend_from_slice(&chunk[..n]);
    }
    let text = String::from_utf8_lossy(&head);
    let line = text.split("\r\n").next().unwrap_or("");
    if line.split(' ').nth(1) != Some("/events") {
        return conn.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
    }
    println!("[server] Client {addr} đăng ký luồng: {line}");
    conn.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n\
          Cache-Control: no-cache\r\nTransfer-Encoding: chunked\r\n\r\n",
    )?;
    for (i, ev) in EVENTS.iter().enumerate() {
        let n = send_chunk(conn, ev)?;
        let shown = ev.replace('\n', "\\n"); // in '\n' dạng hai ký tự
        println!("[server] Gửi khối {} ({n} byte): '{shown}'", i + 1);
        thread::sleep(Duration::from_millis(300)); // sự kiện đến dần theo thời gian
    }
    conn.write_all(b"0\r\n\r\n")?; // khối 0: kết thúc thân
    println!("[server] Kết thúc luồng cho {addr}");
    Ok(())
}

fn serve(mut conn: TcpStream, addr: String) {
    if let Err(e) = stream(&mut conn, &addr) {
        println!("[server] Client {addr} ngắt kết nối: {e}");
    }
    // conn được đóng khi ra khỏi hàm
}

pub fn run(args: &[String]) {
    let host = args.first().map(String::as_str).unwrap_or("0.0.0.0");
    let port = port_arg(args, 1, 8080);
    let srv = TcpListener::bind((host, port)).unwrap_or_else(|e| exit_with(&format!("Không mở được cổng {port}: {e}")));
    println!("[server] Đang lắng nghe tại {host}:{port}");
    for conn in srv.incoming() {
        let Ok(conn) = conn else { continue };
        let addr = conn.peer_addr().map(|a| a.to_string()).unwrap_or_default();
        thread::spawn(move || serve(conn, addr)); // mỗi kết nối một luồng
    }
}
