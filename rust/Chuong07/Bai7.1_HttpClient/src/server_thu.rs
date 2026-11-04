//! Bài 7.1 – Server HTTP thử nghiệm cho client tối giản.
//!
//! Server tuần tự (khung của Mã giả 3.1) trả về ba kiểu định khung thân của HTTP/1.1 (Mục 7.2.1):
//!   /api/v1/terms/TCP  -> Content-Length
//!   /chunked           -> Transfer-Encoding: chunked (có phần mở rộng khối và trailer)
//!   /dong-ket-noi      -> không có độ dài, server đóng kết nối khi hết dữ liệu
//!   đường dẫn khác     -> 404
//!
//! Chạy:  cargo run -- server [host] [port]      (mặc định 0.0.0.0 8080)
use crate::{exit_with, port_arg};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};

/// Thân JSON của thuật ngữ TCP (viết gọn, không khoảng trắng).
const TERM: &str = r#"{"term":"TCP","text":"Giao thức vận chuyển hướng kết nối"}"#;

/// Đọc dòng yêu cầu và tiêu đề đến dòng trống (đủ cho server thử).
fn read_request_head(conn: &mut TcpStream) -> io::Result<Option<String>> {
    let mut buf = Vec::new();
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let mut chunk = [0u8; 4096];
        let n = conn.read(&mut chunk)?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let text = String::from_utf8_lossy(&buf);
    Ok(text.split("\r\n").next().map(str::to_string))
}

fn respond(conn: &mut TcpStream, path: &str) -> io::Result<&'static str> {
    if path == "/api/v1/terms/TCP" {
        let body = TERM.as_bytes();
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        conn.write_all(&[head.as_bytes(), body].concat())?;
        return Ok("Content-Length");
    }
    if path == "/chunked" {
        let parts = ["Thân phản hồi ", "được chia thành ", "nhiều khối."];
        let mut out = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n".to_vec();
        out.extend_from_slice(b"Transfer-Encoding: chunked\r\nTrailer: X-So-Khoi\r\n\r\n");
        for (i, p) in parts.iter().enumerate() {
            let b = p.as_bytes();
            let ext = if i == 0 { ";vi-du=mo-rong" } else { "" }; // phần mở rộng khối (bị client bỏ qua)
            out.extend_from_slice(format!("{:X}{ext}\r\n", b.len()).as_bytes());
            out.extend_from_slice(b);
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(b"0\r\nX-So-Khoi: 3\r\n\r\n"); // khối 0, trailer, dòng trống
        conn.write_all(&out)?;
        return Ok("chunked");
    }
    if path == "/dong-ket-noi" {
        let text = "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n\
                    Không có Content-Length: client đọc đến khi kết nối đóng.";
        conn.write_all(text.as_bytes())?;
        return Ok("đóng kết nối");
    }
    let body = br#"{"type":"about:blank","title":"Not Found","status":404}"#;
    let head = format!(
        "HTTP/1.1 404 Not Found\r\nContent-Type: application/problem+json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    conn.write_all(&[head.as_bytes(), body].concat())?;
    Ok("Content-Length")
}

pub fn run(args: &[String]) {
    let host = args.first().map(String::as_str).unwrap_or("0.0.0.0");
    let port = port_arg(args, 1, 8080);
    let srv = TcpListener::bind((host, port)).unwrap_or_else(|e| exit_with(&format!("Không mở được cổng {port}: {e}")));
    println!("[server] Đang lắng nghe tại {host}:{port}");
    for conn in srv.incoming() {
        let Ok(mut conn) = conn else { continue };
        let line = match read_request_head(&mut conn) {
            Ok(Some(line)) => line,
            _ => continue,
        };
        let path = line.split(' ').nth(1).unwrap_or("/");
        match respond(&mut conn, path) {
            Ok(how) => println!("[server] {line} -> định khung: {how}"),
            Err(e) => println!("[server] Lỗi khi trả lời: {e}"),
        }
        // conn được đóng khi ra khỏi vòng lặp (with conn của Python)
    }
}
