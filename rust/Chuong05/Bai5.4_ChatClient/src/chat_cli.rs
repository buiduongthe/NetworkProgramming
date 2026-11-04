//! Giao diện dòng lệnh dùng lớp ChatClient (Mã giả 5.5).
//!
//! Tầng "giao diện" chỉ đăng ký hai sự kiện on_message, on_status và gọi send_message, disconnect.
//! Mỗi dòng nhập được gửi thành một tin; hai lệnh riêng:
//!   /quit        ngắt kết nối và thoát
//!   /cho <giây>  tạm dừng đọc bàn phím (dùng trong kịch bản chạy minh họa)
//! Bàn phím được đọc trên một luồng phụ và chuyển vào kênh tokio::sync::mpsc, nên vòng nhận
//! vẫn chạy trong lúc chờ người dùng gõ.

use crate::chat_client::ChatClient;
use crate::khung_json_async::parse_endpoint;
use serde_json::Value;
use std::io::{self, BufRead};
use std::process;
use std::sync::Arc;
use std::thread;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

/// Phòng đang chọn (server tối giản của bài chỉ có một phòng chung).
const ROOM: &str = "chung";

/// Luồng phụ đọc bàn phím; None báo hết dữ liệu vào (Ctrl+D).
fn start_console_reader() -> mpsc::UnboundedReceiver<Option<String>> {
    let (tx, rx) = mpsc::unbounded_channel();
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(Some(line)).is_err() {
                return;
            }
        }
        let _ = tx.send(None);
    });
    rx
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Trình xử lý sự kiện on_message.
fn show_message(me: &str, msg: &Value) {
    match msg["type"].as_str() {
        Some("msg") => println!("[{me}] {}: {}", text(&msg["from"]), text(&msg["text"])),
        Some("sys") => println!("[{me}] (hệ thống) {}", text(&msg["text"])),
        Some("welcome") => {
            let users: Vec<String> = msg["users"].as_array().map(|a| a.iter().map(text).collect()).unwrap_or_default();
            println!("[{me}] Server chào mừng; đang trực tuyến: {}", users.join(", "));
        }
        Some("error") => println!("[{me}] Lỗi {}: {}", msg["code"], text(&msg["text"])),
        _ => println!("[{me}] Nhận: {msg}"),
    }
}

pub async fn run(args: &[String]) {
    let Some(me) = args.first().cloned() else {
        eprintln!("Cách dùng: cargo run -q --release -- cli <tên> [host] [port]");
        process::exit(1);
    };
    let (host, port) = parse_endpoint(&args[1..], "127.0.0.1", 9200);
    let mut c = ChatClient::new();
    let (m1, m2) = (me.clone(), me.clone());
    c.on_message = Arc::new(move |msg| show_message(&m1, &msg)); // đăng ký sự kiện
    c.on_status = Arc::new(move |t| println!("[{m2}] Trạng thái: {t}"));
    if let Err(e) = c.connect(&host, port, &me).await {
        eprintln!("[{me}] Không kết nối được: {e}");
        process::exit(1);
    }
    let mut keyboard = start_console_reader();
    loop {
        tokio::select! {
            line = keyboard.recv() => {
                let Some(Some(line)) = line else { break };   // hết dữ liệu vào
                if line == "/quit" {
                    break;
                }
                if let Some(sec) = line.strip_prefix("/cho ") {
                    let sec: f64 = sec.trim().parse().unwrap_or(0.0);
                    sleep(Duration::from_secs_f64(sec.max(0.0))).await;
                } else if !line.is_empty() {
                    if let Err(e) = c.send_message(ROOM, &line).await {   // AWAIT SEND_MESSAGE(c, room, text)
                        println!("[{me}] Lỗi gửi: {e}");
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => break,   // Ctrl+C: ngắt kết nối như /quit
        }
    }
    c.disconnect().await;
    println!("[{me}] Đã ngắt kết nối");
}
