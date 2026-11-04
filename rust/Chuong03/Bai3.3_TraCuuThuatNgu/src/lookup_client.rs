//! Client tra cứu thuật ngữ.
//!
//! Cài đặt Mã giả 3.8 (Mục 3.5.1, Giáo trình Lập trình mạng): giữ phần kết nối của ECHO_CLIENT
//! (Mã giả 3.2), thay vòng gửi nhận thô bằng SEND_JSON/RECV_JSON (Mã giả 3.6).
//! Mỗi dòng nhập là một thuật ngữ cần tra; hết dữ liệu nhập thì gửi {"op":"bye"}.

use crate::framing::{recv_json, send_json, show, FrameError};
use crate::netutil::{parse_endpoint, safe_connect};
use serde_json::json;
use std::io::{self, BufRead};
use std::net::TcpStream;
use std::process;
use std::time::Duration;

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
    println!("[client] Đã kết nối {host}:{port}. Nhập thuật ngữ cần tra (Ctrl+D để kết thúc).");
    if let Err(e) = session(&mut s) {
        println!("[client] Lỗi: {e}");
    }
    drop(s); // CLOSE(s)
    println!("[client] Đã đóng kết nối");
}

fn session(s: &mut TcpStream) -> Result<(), FrameError> {
    for term in io::stdin().lock().lines() {
        // READ_CONSOLE()
        let term = term?;
        send_json(s, &json!({"op": "lookup", "term": term}))?;
        let resp = recv_json(s)?;
        if resp["status"] == 200 {
            println!("[client] {}: {}", show(&resp["term"]), show(&resp["text"]));
        } else {
            println!("[client] Lỗi {}: {} ('{term}')", resp["status"], show(&resp["error"]));
        }
    }
    send_json(s, &json!({"op": "bye"}))?;
    Ok(())
}
