//! Bài 7.3 – Kiểm thử server WebSocket bằng các khung tự tạo (kể cả khung vi phạm đặc tả).
//!
//! Năm ca kiểm thử, mỗi ca một kết nối:
//!   1. Yêu cầu HTTP thường (không Upgrade)       -> mong đợi 400 (Mã giả 7.3, dòng 3–7)
//!   2. Ping                                       -> mong đợi pong cùng dữ liệu
//!   3. Văn bản 300 byte (độ dài mở rộng 16 bit)   -> mong đợi echo (Mã giả 7.4, dòng 5)
//!   4. Khung văn bản KHÔNG che                    -> mong đợi khung đóng 1002 (dòng 11–14)
//!   5. Khung khai báo độ dài 2^40 byte            -> mong đợi khung đóng 1009 (dòng 7–10)
//!
//! Chạy:  cargo run -- kiemthu [host] [port]      (mặc định 127.0.0.1 8080)
use crate::port_arg;
use crate::ws_client::{close_code, connect, ws_connect, ws_read_frame, ws_send};
use std::io::{self, Read, Write};

fn all_cases(host: &str, port: u16) -> io::Result<()> {
    {
        // ca 1
        let mut s = connect(host, port)?;
        s.write_all(format!("GET / HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n").as_bytes())?;
        let mut buf = [0u8; 4096];
        let n = s.read(&mut buf)?;
        let text = String::from_utf8_lossy(&buf[..n]);
        let status = text.split("\r\n").next().unwrap_or("");
        println!("[kiểm thử] 1. Yêu cầu HTTP thường (không Upgrade) -> {status}");
    }
    {
        // ca 2
        let (mut s, _) = ws_connect(host, port)?;
        ws_send(&mut s, 0x9, b"nhip-tim", true)?;
        let (_, opcode, data) = ws_read_frame(&mut s)?;
        println!("[kiểm thử] 2. Ping 'nhip-tim' -> opcode 0x{opcode:X}, dữ liệu '{}'", String::from_utf8_lossy(&data));
        ws_send(&mut s, 0x8, &1000u16.to_be_bytes(), true)?;
        ws_read_frame(&mut s)?;
    }
    {
        // ca 3
        let (mut s, _) = ws_connect(host, port)?;
        ws_send(&mut s, 0x1, &[b'A'; 300], true)?;
        let (_, opcode, data) = ws_read_frame(&mut s)?;
        println!("[kiểm thử] 3. Văn bản 300 byte -> nhận lại {} byte (opcode 0x{opcode:X})", data.len());
        ws_send(&mut s, 0x8, &1000u16.to_be_bytes(), true)?;
        ws_read_frame(&mut s)?;
    }
    {
        // ca 4
        let (mut s, _) = ws_connect(host, port)?;
        ws_send(&mut s, 0x1, b"khong che", false)?;
        let (_, _, data) = ws_read_frame(&mut s)?;
        println!("[kiểm thử] 4. Khung không che -> khung đóng, mã {}", close_code(&data));
    }
    {
        // ca 5: chỉ gửi tiêu đề khung
        let (mut s, _) = ws_connect(host, port)?;
        let mut head = vec![0x81, 0x80 | 127];
        head.extend_from_slice(&(1u64 << 40).to_be_bytes());
        s.write_all(&head)?;
        let (_, _, data) = ws_read_frame(&mut s)?;
        println!("[kiểm thử] 5. Khung khai báo 2^40 byte -> khung đóng, mã {}", close_code(&data));
    }
    Ok(())
}

pub fn run(args: &[String]) {
    let host = args.first().map(String::as_str).unwrap_or("127.0.0.1");
    let port = port_arg(args, 1, 8080);
    if let Err(e) = all_cases(host, port) {
        println!("[kiểm thử] Lỗi: {e}");
    }
}
