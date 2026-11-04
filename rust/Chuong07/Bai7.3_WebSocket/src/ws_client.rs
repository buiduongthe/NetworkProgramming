//! Bài 7.3 – Client WebSocket tự viết bằng thư viện chuẩn (socket chặn).
//!
//! Phía client của Mã giả 7.3 (gửi yêu cầu nâng cấp, kiểm tra Sec-WebSocket-Accept) và Mã giả 7.4
//! (đọc khung với from_client = FALSE). Mọi khung client gửi đều được che bằng khóa ngẫu nhiên 4 byte.
//! Đọc từng dòng từ bàn phím, gửi thành khung văn bản và in khung trả lời; cuối cùng gửi khung đóng 1000.
//!
//! Chạy:  cargo run -- client [host] [port]      (mặc định 127.0.0.1 8080)
use crate::{accept_key, port_arg, random_bytes};
use base64::Engine;
use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Mã giả 3.4 – RECV_EXACT.
pub fn recv_exact(s: &mut TcpStream, n: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; n];
    let mut got = 0;
    while got < n {
        let k = s.read(&mut buf[got..])?;
        if k == 0 {
            return Err(io::Error::other(format!("Kết nối đóng khi mới nhận {got}/{n} byte")));
        }
        got += k;
    }
    Ok(buf)
}

/// Kết nối TCP có thời gian chờ 3 giây, thời gian chờ nhận 5 giây.
pub fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
    let addr = (host, port).to_socket_addrs()?.next().ok_or(io::Error::other("không phân giải được tên"))?;
    let s = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    Ok(s)
}

/// Gửi yêu cầu nâng cấp (Sec-WebSocket-Key = Base64 của 16 byte ngẫu nhiên), kiểm tra phản hồi 101.
pub fn ws_connect(host: &str, port: u16) -> io::Result<(TcpStream, String)> {
    let mut s = connect(host, port)?;
    let key = base64::engine::general_purpose::STANDARD.encode(random_bytes(16));
    let req = format!(
        "GET / HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    s.write_all(req.as_bytes())?;
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.extend(recv_exact(&mut s, 1)?); // đọc từng byte để không lấy lố sang khung đầu tiên
    }
    let text = String::from_utf8_lossy(&head).into_owned();
    let lines: Vec<&str> = text.split("\r\n").collect();
    let hdr: HashMap<String, String> = lines[1..]
        .iter()
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
        .collect();
    let expect = accept_key(&key);
    if !lines[0].starts_with("HTTP/1.1 101") || hdr.get("sec-websocket-accept") != Some(&expect) {
        return Err(io::Error::other(format!("Bắt tay thất bại: {}", lines[0])));
    }
    Ok((s, lines[0].to_string()))
}

/// Gửi một khung FIN = 1; client che dữ liệu bằng khóa 4 byte ngẫu nhiên.
pub fn ws_send(s: &mut TcpStream, opcode: u8, data: &[u8], mask: bool) -> io::Result<()> {
    let n = data.len();
    let b1 = if mask { 0x80 } else { 0 };
    let mut out = vec![0x80 | opcode];
    if n < 126 {
        out.push(b1 | n as u8);
    } else if n < 65536 {
        out.push(b1 | 126);
        out.extend_from_slice(&(n as u16).to_be_bytes());
    } else {
        out.push(b1 | 127);
        out.extend_from_slice(&(n as u64).to_be_bytes());
    }
    if mask {
        let key = random_bytes(4);
        out.extend_from_slice(&key);
        out.extend(data.iter().enumerate().map(|(i, d)| d ^ key[i % 4]));
    } else {
        out.extend_from_slice(data);
    }
    s.write_all(&out)
}

/// Mã giả 7.4 với from_client = FALSE: khung từ server không che.
pub fn ws_read_frame(s: &mut TcpStream) -> io::Result<(bool, u8, Vec<u8>)> {
    let b = recv_exact(s, 2)?;
    let (fin, opcode) = ((b[0] & 0x80) != 0, b[0] & 0x0F);
    let (masked, mut n) = ((b[1] & 0x80) != 0, (b[1] & 0x7F) as u64);
    if n == 126 {
        let e = recv_exact(s, 2)?;
        n = u16::from_be_bytes([e[0], e[1]]) as u64;
    } else if n == 127 {
        let e = recv_exact(s, 8)?;
        n = u64::from_be_bytes(e.try_into().unwrap());
    }
    let mask = if masked { recv_exact(s, 4)? } else { Vec::new() };
    let mut data = recv_exact(s, n as usize)?;
    if masked {
        for (i, d) in data.iter_mut().enumerate() {
            *d ^= mask[i % 4];
        }
    }
    Ok((fin, opcode, data))
}

/// Mã đóng trong khung đóng (1005 nếu khung không có mã).
pub fn close_code(data: &[u8]) -> u16 {
    if data.len() >= 2 {
        u16::from_be_bytes([data[0], data[1]])
    } else {
        1005
    }
}

fn session(s: &mut TcpStream) -> io::Result<()> {
    for line in io::stdin().lock().lines() {
        ws_send(s, 0x1, line?.as_bytes(), true)?;
        let (_, opcode, data) = ws_read_frame(s)?;
        if opcode == 0x8 {
            println!("[client] Server đóng kết nối, mã {}", close_code(&data));
            return Ok(());
        }
        println!("[client] Nhận: '{}'", String::from_utf8_lossy(&data));
    }
    ws_send(s, 0x8, &1000u16.to_be_bytes(), true)?; // khung đóng, mã 1000: đóng bình thường
    let (_, _, data) = ws_read_frame(s)?;
    println!("[client] Server xác nhận đóng, mã {}", close_code(&data));
    Ok(())
}

pub fn run(args: &[String]) {
    let host = args.first().map(String::as_str).unwrap_or("127.0.0.1");
    let port = port_arg(args, 1, 8080);
    let (mut s, status) = match ws_connect(host, port) {
        Ok(x) => x,
        Err(e) => {
            println!("[client] Lỗi: {e}");
            std::process::exit(1);
        }
    };
    println!("[client] Bắt tay thành công: {status}");
    if let Err(e) = session(&mut s) {
        println!("[client] Lỗi: {e}");
    }
    drop(s); // CLOSE(s)
    println!("[client] Đã đóng kết nối");
}
