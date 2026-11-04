//! Server chat tối giản để chạy thử lớp ChatClient.
//!
//! Server chỉ cài đặt phần giao thức chat (Mục 6.3.1) mà Mã giả 5.5 dùng tới: hello → welcome
//! (lỗi 409 nếu trùng tên), msg → chuyển tiếp tới mọi người trong một phòng chung, ping → pong,
//! thông báo sys khi có người vào, ra. Bản đầy đủ (nhiều phòng, tin riêng, hộp thư đi cho mỗi
//! client) là Mã giả 6.x ở Chương 6. Mỗi client có một khóa gửi để các khung không xen kẽ.

use crate::khung_json_async::{parse_endpoint, recv_json_async, send_json_async, CancelToken, FrameError};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

/// Nửa ghi của một client, nằm trong khóa gửi riêng của client đó.
type Writer = Arc<Mutex<OwnedWriteHalf>>;
/// Danh sách người dùng theo thứ tự vào phòng: (tên, nửa ghi).
type Users = Arc<StdMutex<Vec<(String, Writer)>>>;

/// Giờ:phút:giây hiện tại theo UTC (thư viện chuẩn Rust không có múi giờ địa phương).
fn now_hms() -> String {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) % 86400;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

async fn send_to(w: &Writer, obj: &Value) -> Result<(), FrameError> {
    let mut guard = w.lock().await; // mỗi socket mỗi lúc chỉ ghi một khung
    send_json_async(&mut *guard, obj, &CancelToken::new()).await
}

async fn broadcast(users: &Users, obj: &Value) {
    // sao chép danh sách rồi mới gửi (không giữ khóa danh sách trong lúc chờ mạng)
    let targets: Vec<Writer> = users.lock().unwrap().iter().map(|(_, w)| w.clone()).collect();
    for w in targets {
        let _ = send_to(&w, obj).await; // client vừa rời: bỏ qua
    }
}

/// Nhận một khung JSON với thời gian chờ tính bằng giây.
async fn recv_with_timeout(reader: &mut OwnedReadHalf, secs: u64) -> Result<Value, FrameError> {
    match timeout(Duration::from_secs(secs), recv_json_async(reader, &CancelToken::new())).await {
        Ok(r) => r,
        Err(_) => Err(FrameError::Protocol(format!("Quá {secs} s không nhận được thông điệp"))),
    }
}

/// Thân một phiên chat; tên người dùng (nếu đã vào phòng) được ghi vào `joined`.
async fn session(reader: &mut OwnedReadHalf, writer: &Writer, users: &Users, joined: &mut Option<String>) -> Result<(), FrameError> {
    let h = recv_with_timeout(reader, 10).await?; // hello trong 10 giây
    let name = h["name"].as_str().unwrap_or("").to_string();
    if h["type"] != "hello" || name.is_empty() {
        return send_to(writer, &json!({"type": "error", "code": 400, "text": "Cần hello"})).await;
    }
    let taken = users.lock().unwrap().iter().any(|(n, _)| *n == name);
    if taken {
        return send_to(writer, &json!({"type": "error", "code": 409, "text": "Tên đã được dùng"})).await;
    }
    users.lock().unwrap().push((name.clone(), writer.clone()));
    *joined = Some(name.clone());
    println!("[server] {name} đã kết nối");
    let online: Vec<String> = users.lock().unwrap().iter().map(|(n, _)| n.clone()).collect();
    send_to(writer, &json!({"type": "welcome", "users": online})).await?;
    broadcast(users, &json!({"type": "sys", "text": format!("{name} đã vào phòng")})).await;
    loop {
        let m = recv_with_timeout(reader, 90).await?;
        match m["type"].as_str() {
            Some("msg") => {
                println!("[server] {name}: {}", m["text"].as_str().unwrap_or(""));
                let room = m["room"].as_str().unwrap_or("chung"); // server tối giản: mọi phòng dùng chung một danh sách
                let out = json!({"type": "msg", "from": name, "room": room, "text": m["text"], "ts": now_hms()});
                broadcast(users, &out).await;
            }
            Some("ping") => send_to(writer, &json!({"type": "pong"})).await?,
            _ => send_to(writer, &json!({"type": "error", "code": 400, "text": "Sai loại thông điệp"})).await?,
        }
    }
}

async fn chat_session(conn: TcpStream, users: Users) {
    let (mut reader, writer) = conn.into_split();
    let writer: Writer = Arc::new(Mutex::new(writer));
    let mut joined = None;
    // kết thúc phiên bình thường hoặc do lỗi (đóng kết nối, hết giờ, JSON sai): không cần in lỗi
    let _ = session(&mut reader, &writer, &users, &mut joined).await;
    if let Some(name) = joined {
        users.lock().unwrap().retain(|(n, _)| *n != name);
        println!("[server] {name} đã rời phòng");
        broadcast(&users, &json!({"type": "sys", "text": format!("{name} đã rời phòng")})).await;
    }
    // ra khỏi hàm: hai nửa socket bị hủy, kết nối đóng
}

pub async fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9200);
    let srv = match TcpListener::bind((host.as_str(), port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[server] Không mở được {host}:{port}: {e}");
            std::process::exit(1);
        }
    };
    println!("[server] Đang lắng nghe tại {host}:{port}");
    let users: Users = Arc::new(StdMutex::new(Vec::new()));
    loop {
        tokio::select! {
            r = srv.accept() => if let Ok((conn, _)) = r {
                tokio::spawn(chat_session(conn, users.clone()));
            },
            _ = tokio::signal::ctrl_c() => {
                println!("[server] Dừng server");
                return;
            }
        }
    }
}
