//! Echo Client dùng khung JSON: gửi dòng nhập từ bàn phím và một thông điệp lớn.
//!
//! Cài đặt Mã giả 3.2 (Mục 3.2.3) với vòng gửi nhận thô được thay bằng SEND_JSON/RECV_JSON
//! của Mã giả 3.6 (Mục 3.4.3) – lời giải bài tập "Định khung thông điệp" của Chương 3.
//! Tham số thứ ba (tùy chọn) là số ký tự của thông điệp lớn gửi sau khi hết dữ liệu nhập,
//! ví dụ 200000: nhờ RECV_EXACT, phản hồi được nhận đủ dù đến thành nhiều phần.

use crate::framing::{recv_json, send_json, show, FrameError};
use crate::netutil::{parse_endpoint, safe_connect};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::{self, BufRead};
use std::net::{Shutdown, TcpStream};
use std::process;
use std::time::Duration;

const PATTERN: &str = "Mạng máy tính "; // 14 ký tự, 18 byte UTF-8

/// Tạo chuỗi đúng n ký tự, có chữ tiếng Việt (2–3 byte mỗi ký tự trong UTF-8).
fn make_text(n: usize) -> String {
    PATTERN.chars().cycle().take(n).collect()
}

fn sha256_hex(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(&args[..args.len().min(2)], "127.0.0.1", 9000);
    let big: usize = match args.get(2) {
        None => 0,
        Some(t) => t.trim().parse().unwrap_or_else(|_| {
            eprintln!("Số ký tự phải là số nguyên không âm: {t}");
            process::exit(1);
        }),
    };
    let mut s = match safe_connect(&host, port, 3000) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[client] Không kết nối được: {e}");
            process::exit(1);
        }
    };
    s.set_read_timeout(Some(Duration::from_millis(5000))).ok(); // RECV_TIMEOUT 5000 ms
    println!("[client] Đã kết nối {host}:{port}. Nhập dòng cần gửi (Ctrl+D để kết thúc).");
    if let Err(e) = session(&mut s, big) {
        println!("[client] Lỗi: {e}");
    }
    drop(s); // CLOSE(s)
    println!("[client] Đã đóng kết nối");
}

fn session(s: &mut TcpStream, big: usize) -> Result<(), FrameError> {
    for line in io::stdin().lock().lines() {
        // READ_CONSOLE()
        let line = line?;
        send_json(s, &json!({"text": line}))?; // SEND_JSON thay cho SEND_ALL thô
        let resp = recv_json(s)?; // RECV_JSON: luôn nhận đủ một khung
        println!("[client] Gửi: '{line}' -> Nhận: '{}'", show(&resp["text"]));
    }
    if big > 0 {
        let text = make_text(big);
        println!("[client] Gửi thông điệp lớn: {} ký tự, {} byte UTF-8", text.chars().count(), text.len());
        send_json(s, &json!({"text": text}))?;
        let resp = recv_json(s)?;
        let back = resp["text"].as_str().unwrap_or("");
        let same = if back == text { "có" } else { "không" };
        println!("[client] Nhận lại: {} ký tự, {} byte; trùng khớp: {same}", back.chars().count(), back.len());
        println!("[client] SHA-256 gửi:  {}…", &sha256_hex(&text)[..32]);
        println!("[client] SHA-256 nhận: {}…", &sha256_hex(back)[..32]);
    }
    s.shutdown(Shutdown::Write)?; // SHUTDOWN(WRITE)
    Ok(())
}
