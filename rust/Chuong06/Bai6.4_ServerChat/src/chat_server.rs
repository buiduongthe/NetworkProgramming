//! Bài 6.4 – Server chat nhiều phòng: vòng chấp nhận kết nối và tác vụ phiên.
//!
//! Cài đặt Mã giả 6.5 (Mục 6.3.3, Giáo trình Lập trình mạng): kế thừa Mã giả 6.3 (server bất đồng bộ),
//! thay SERVE_ASYNC bằng CHAT_SESSION. Các thủ tục phát, gửi riêng, tác vụ ghi nằm ở chat_broadcast.rs
//! (Mã giả 6.6). Giao thức: Bảng đặc tả giao thức chat (Mục 6.3.1), cổng mặc định 9200.
//!
//! Chạy:  cargo run -- server [host] [port]      (mặc định 0.0.0.0 9200)
use crate::async_frame::{exit_with, parse_endpoint, recv_json_async, send_json_async, NetError};
use crate::chat_broadcast::{self as cb, Client};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::{TcpListener, TcpSocket, TcpStream};

const RATE_LIMIT: u32 = 10; // tối đa 10 tin msg/pm mỗi giây (Mục 6.3.4)

/// Tên người dùng, tên phòng hợp lệ: 1–32 ký tự chữ, số, '_' hoặc '-' (tương đương ^[\w-]{1,32}$).
fn valid(v: &Value) -> bool {
    match v.as_str() {
        Some(s) => {
            let n = s.chars().count();
            (1..=32).contains(&n) && s.chars().all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
        }
        None => false,
    }
}

/// Đếm tin trong cửa sổ 1 giây; trả false khi vượt RATE_LIMIT (lỗi 429).
struct RateWindow {
    start: Instant,
    count: u32,
}

impl RateWindow {
    fn ok(&mut self) -> bool {
        if self.start.elapsed() >= Duration::from_secs(1) {
            self.start = Instant::now();
            self.count = 0;
        }
        self.count += 1;
        self.count <= RATE_LIMIT
    }
}

/// Kết quả của tác vụ phiên trước khi đăng ký tên: HelloError(code) hoặc lỗi kết nối.
enum HelloOutcome {
    Error(u16),
    Ended,
}

/// CHAT_SESSION (Mã giả 6.5, dòng 4–33).
async fn chat_session(conn: TcpStream, addr: String) {
    let (mut reader, mut sock) = conn.into_split(); // nửa đọc cho phiên, nửa ghi cho tác vụ ghi
    // h ← AWAIT WITH_TIMEOUT(10000, RECV_JSON_ASYNC(conn)): 10 giây cho hello
    let hello = match tokio::time::timeout(Duration::from_secs(10), recv_json_async(&mut reader)).await {
        Ok(Ok(h)) if h["type"] == "hello" && valid(&h["name"]) => Ok(h),
        Ok(Ok(_)) => Err(HelloOutcome::Error(400)),
        _ => Err(HelloOutcome::Ended), // hết giờ hoặc lỗi kết nối
    };
    let registered = match hello {
        Ok(h) => match cb::register(h["name"].as_str().unwrap()) {
            Some(x) => Ok(x),
            None => Err(HelloOutcome::Error(409)), // trùng tên
        },
        Err(e) => Err(e),
    };
    let (c, outbox_rx) = match registered {
        Ok(x) => x,
        Err(HelloOutcome::Error(code)) => {
            // CATCH HelloError: lời chào không hợp lệ
            cb::log(&format!("[server] Từ chối {addr}: lỗi {code}"));
            let _ = send_json_async(&mut sock, &cb::error(code)).await;
            return; // CLOSE(conn): hai nửa socket được giải phóng
        }
        Err(HelloOutcome::Ended) => return,
    };
    cb::log(&format!("[server] {} đăng ký từ {addr}", c.name));
    tokio::spawn(cb::writer(Arc::clone(&c), outbox_rx, sock)); // RUN_BACKGROUND(WRITER(c))
    let snapshot = {
        let t = cb::LOCK.lock().unwrap();
        let mut users: Vec<&String> = t.users.keys().collect();
        let mut rooms: Vec<&String> = t.rooms.keys().collect();
        users.sort();
        rooms.sort();
        json!({"type": "welcome", "users": users, "rooms": rooms})
    };
    cb::enqueue(&c, snapshot);
    session_loop(&c, &mut reader).await; // kết thúc phiên bình thường hoặc do lỗi
    // FINALLY
    cb::unregister(&c);
    c.close_outbox();
    cb::log(&format!("[server] {} rời server", c.name));
    // CLOSE(conn): nửa đọc được giải phóng khi ra khỏi hàm, nửa ghi khi tác vụ ghi kết thúc
}

/// Vòng lặp chính của CHAT_SESSION: đọc từng thông điệp rồi xử lý theo trường type.
async fn session_loop(c: &Client, reader: &mut OwnedReadHalf) {
    let mut win = RateWindow { start: Instant::now(), count: 0 };
    let mut kill = c.kill.subscribe();
    loop {
        let m = tokio::select! {
            // 90 giây = 3 chu kỳ nhịp tim
            r = tokio::time::timeout(Duration::from_secs(90), recv_json_async(reader)) => r,
            _ = kill.wait_for(|&k| k) => return, // socket bị đóng (client chậm, lỗi ghi)
        };
        let m: Value = match m {
            Ok(Ok(m)) => m,
            Ok(Err(NetError::ConnectionClosed(_) | NetError::Io(_) | NetError::Protocol(_))) => return,
            Err(_) => return, // Timeout
        };
        let t = m["type"].as_str().unwrap_or("");
        if (t == "msg" || t == "pm") && !win.ok() {
            cb::enqueue(c, cb::error(429));
        } else if t == "join" && valid(&m["room"]) {
            cb::join_room(c, m["room"].as_str().unwrap());
        } else if t == "leave" && m["room"].is_string() {
            cb::leave_room(c, m["room"].as_str().unwrap());
        } else if t == "msg" && m["room"].is_string() && m["text"].is_string() {
            let msg = json!({"type": "msg", "from": c.name, "room": m["room"], "text": m["text"], "ts": cb::now_ts()});
            cb::broadcast(m["room"].as_str().unwrap(), msg, Some(c));
        } else if t == "pm" && m["to"].is_string() && m["text"].is_string() {
            let msg = json!({"type": "pm", "from": c.name, "text": m["text"], "ts": cb::now_ts()});
            cb::send_to(m["to"].as_str().unwrap(), msg, Some(c));
        } else if t == "ping" {
            cb::enqueue(c, json!({"type": "pong"}));
        } else {
            cb::enqueue(c, cb::error(400)); // OTHERWISE
        }
    }
}

/// SOCKET, SETOPT(REUSEADDR), BIND, LISTEN(128) như Mã giả 6.3.
async fn open_listener(host: &str, port: u16) -> std::io::Result<TcpListener> {
    let addr = tokio::net::lookup_host((host, port))
        .await?
        .find(|a| a.is_ipv4())
        .ok_or(std::io::Error::other("không có địa chỉ IPv4"))?;
    let srv = TcpSocket::new_v4()?;
    srv.set_reuseaddr(true)?;
    srv.bind(addr)?;
    srv.listen(128)
}

async fn chat_server(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9200);
    let srv = open_listener(&host, port)
        .await
        .unwrap_or_else(|e| exit_with(&format!("Không mở được cổng {port}: {e}")));
    cb::log(&format!("[server] Server chat đang lắng nghe tại {host}:{port}"));
    loop {
        let Ok((conn, a)) = srv.accept().await else { continue }; // AWAIT ACCEPT_ASYNC
        tokio::spawn(chat_session(conn, a.to_string())); // RUN_BACKGROUND(CHAT_SESSION(conn, addr))
    }
}

pub async fn run(args: &[String]) {
    tokio::select! {
        _ = chat_server(args) => {}
        _ = tokio::signal::ctrl_c() => println!("[server] Dừng server"),
    }
}
