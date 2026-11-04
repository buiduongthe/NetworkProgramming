//! Client tra cứu và tải tệp lên server.
//!
//! Cài đặt Mã giả 3.10 (PUT_FILE, Mục 3.5.2) trên nền client tra cứu của Mã giả 3.8:
//! cùng một kết nối dùng cho cả tra cứu và tải tệp. Mỗi dòng nhập là một lệnh:
//!     put <đường dẫn tệp>     – tải tệp lên (đọc và gửi từng khối 64 KiB)
//!     <thuật ngữ>             – tra cứu thuật ngữ

use crate::framing::{json_encode, recv_json, send_all, send_json, show, FrameError};
use crate::netutil::{parse_endpoint, safe_connect};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, BufRead, ErrorKind, Read};
use std::net::TcpStream;
use std::path::Path;
use std::process;
use std::time::Duration;

/// HEX(SHA256_OF_FILE(path)): đọc tệp theo từng khối 64 KiB.
fn sha256_of_file(path: &str) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// Mã giả 3.10: khung tiêu đề -> chờ 100 -> gửi đúng size byte -> nhận kết quả.
fn put_file(s: &mut TcpStream, path: &str) -> Result<(), FrameError> {
    let digest = sha256_of_file(path)?; // HEX(SHA256_OF_FILE(path))
    let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(); // BASENAME
    let size = fs::metadata(path)?.len();
    println!("[client] Tải lên {name}: {size} byte, SHA-256 {}…", &digest[..16]);
    send_json(s, &json!({"op": "put", "name": name, "size": size, "sha256": digest}))?;
    let resp = recv_json(s)?;
    if resp["status"] != 100 {
        return Err(FrameError::Protocol(show(&resp["error"])));
    }
    let mut f = File::open(path)?; // OPEN_FILE(path, READ)
    let mut buf = vec![0u8; 65536];
    loop {
        let n = f.read(&mut buf)?; // READ(f, 65536)
        if n == 0 {
            break;
        }
        send_all(s, &buf[..n])?;
    }
    let resp = recv_json(s)?;
    println!("[client] Kết quả: {} {}", resp["status"], json_encode(&resp));
    Ok(())
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "127.0.0.1", 9100);
    let mut s = match safe_connect(&host, port, 3000) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[client] Không kết nối được: {e}");
            process::exit(1);
        }
    };
    s.set_read_timeout(Some(Duration::from_millis(5000))).ok(); // RECV_TIMEOUT 5000 ms
    println!("[client] Đã kết nối {host}:{port}. Nhập 'put <tệp>' hoặc thuật ngữ (Ctrl+D để kết thúc).");
    if let Err(e) = session(&mut s) {
        println!("[client] Lỗi: {e}");
    }
    drop(s);
    println!("[client] Đã đóng kết nối");
}

fn session(s: &mut TcpStream) -> Result<(), FrameError> {
    for line in io::stdin().lock().lines() {
        // READ_CONSOLE()
        let line = line?;
        if let Some(rest) = line.strip_prefix("put ") {
            let path = rest.trim();
            match put_file(s, path) {
                Ok(()) => {}
                // tệp không tồn tại: báo lỗi, phiên vẫn tiếp tục
                Err(FrameError::Io(e)) if e.kind() == ErrorKind::NotFound => {
                    println!("[client] Lỗi: không tìm thấy tệp '{path}'")
                }
                Err(FrameError::Protocol(m)) => println!("[client] Lỗi: {m}"), // server không trả 100
                Err(e) => return Err(e),
            }
            continue;
        }
        send_json(s, &json!({"op": "lookup", "term": line}))?;
        let resp = recv_json(s)?;
        if resp["status"] == 200 {
            println!("[client] {}: {}", show(&resp["term"]), show(&resp["text"]));
        } else {
            println!("[client] Lỗi {}: {} ('{line}')", resp["status"], show(&resp["error"]));
        }
    }
    send_json(s, &json!({"op": "bye"}))?;
    Ok(())
}
