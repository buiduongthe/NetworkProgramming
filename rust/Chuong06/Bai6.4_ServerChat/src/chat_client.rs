//! Bài 6.4 – Client chat dòng lệnh tối giản dùng lớp ChatClient.
//!
//! Kiểu ChatClient chép gọn từ Mã giả 5.5 (Mục 5.3.3): CONNECT gửi hello rồi chạy nền RECEIVE_LOOP,
//! SEND dùng send_lock để mỗi lúc chỉ gửi một khung, DISCONNECT hủy vòng nhận rồi đóng socket.
//! Lệnh (Mục 6.3.3): /join phòng, /leave phòng, /pm tên nội dung, /ping, /quit; dòng khác là tin msg
//! gửi tới phòng hiện tại (phòng của lệnh /join gần nhất). Bộ hẹn giờ gửi ping mỗi 30 giây.
//! Khi hết dữ liệu nhập (Ctrl+D), client chờ 1 giây để nhận nốt thông điệp rồi ngắt kết nối.
//!
//! Chạy:  cargo run -- client [host] [port] [tên]      (mặc định 127.0.0.1 9200 user)
use crate::async_frame::{parse_endpoint, recv_json_async, send_json_async, NetError};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Hàm xử lý sự kiện trạng thái ("connected", "lost: …").
type StatusFn = Arc<dyn Fn(&str) + Send + Sync>;

/// STRUCT ChatClient { sock, send_lock, on_message, on_status } (Mã giả 5.5).
/// Nửa ghi của socket nằm trong send_lock (ASYNC_LOCK) nên mỗi lúc chỉ một khung được gửi.
pub struct ChatClient {
    send_lock: Arc<Mutex<OwnedWriteHalf>>,
    recv_task: JoinHandle<()>,
}

impl ChatClient {
    /// CONNECT: SAFE_CONNECT_ASYNC (3000 ms), gửi hello, báo "connected" rồi chạy nền RECEIVE_LOOP.
    pub async fn connect(
        host: &str,
        port: u16,
        name: &str,
        on_message: fn(&Value),
        on_status: StatusFn,
    ) -> Result<ChatClient, String> {
        let conn = match tokio::time::timeout(Duration::from_secs(3), TcpStream::connect((host, port))).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => return Err(e.to_string()),
            Err(_) => return Err("quá thời gian chờ kết nối".into()),
        };
        let (reader, mut writer) = conn.into_split();
        // nếu gửi lỗi, hai nửa socket được giải phóng khi trả lỗi (không rò socket)
        send_json_async(&mut writer, &json!({"type": "hello", "name": name}))
            .await
            .map_err(|e| e.to_string())?;
        on_status("connected"); // báo trước khi vòng nhận chạy nền
        let recv_task = tokio::spawn(receive_loop(reader, on_message, on_status)); // RUN_BACKGROUND
        Ok(ChatClient { send_lock: Arc::new(Mutex::new(writer)), recv_task })
    }

    /// SEND: mỗi lúc gửi một khung.
    pub async fn send(&self, obj: Value) -> Result<(), NetError> {
        let mut w = self.send_lock.lock().await; // AWAIT send_lock.acquire()
        send_json_async(&mut *w, &obj).await
    }

    /// DISCONNECT: hủy vòng nhận trước, đóng socket sau.
    pub async fn disconnect(&self) {
        self.recv_task.abort();
        let mut w = self.send_lock.lock().await;
        let _ = w.shutdown().await; // CLOSE(sock): gửi FIN; socket được giải phóng khi ChatClient bị hủy
    }
}

/// RECEIVE_LOOP: nhận từng thông điệp và chuyển cho on_message đến khi kết nối hỏng.
async fn receive_loop(mut reader: OwnedReadHalf, on_message: fn(&Value), on_status: StatusFn) {
    loop {
        match recv_json_async(&mut reader).await {
            Ok(m) => on_message(&m),
            Err(e) => {
                let reason = match e {
                    NetError::ConnectionClosed(0) => "server đã đóng kết nối".to_string(),
                    e => e.to_string(),
                };
                on_status(&format!("lost: {reason}"));
                return; // nửa đọc được đóng khi ra khỏi hàm
            }
        }
    }
}

/// on_message: hiển thị một thông điệp từ server.
fn show(m: &Value) {
    let s = |k: &str| m[k].as_str().unwrap_or("").to_string();
    let ts = s("ts");
    let hhmmss = ts.get(11..19).unwrap_or("");
    match m["type"].as_str().unwrap_or("") {
        "welcome" => {
            let join = |k: &str| {
                let v: Vec<&str> = m[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
                v.join(", ")
            };
            let rooms = join("rooms");
            let rooms = if rooms.is_empty() { "(chưa có)".to_string() } else { rooms };
            println!("[client] Chào mừng! Trực tuyến: {}; phòng: {rooms}", join("users"));
        }
        "msg" => println!("[client] {hhmmss} [{}] {}: {}", s("room"), s("from"), s("text")),
        "pm" => println!("[client] {hhmmss} [riêng] {}: {}", s("from"), s("text")),
        "sys" => println!("[client] * {}", s("text")),
        "error" => println!("[client] Lỗi {}: {}", m["code"], s("text")),
        "pong" => println!("[client] pong"),
        _ => println!("[client] {m}"),
    }
}

pub async fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "127.0.0.1", 9200);
    let name = args.get(2).cloned().unwrap_or("user".into());
    let connected = format!("[client] Đã kết nối {host}:{port}, gửi hello với tên {name}");
    let on_status: StatusFn = Arc::new(move |s: &str| match s.strip_prefix("lost: ") {
        Some(reason) => println!("[client] Mất kết nối: {reason}"),
        None => println!("{connected}"),
    });
    let c = match ChatClient::connect(&host, port, &name, show, on_status).await {
        Ok(c) => Arc::new(c),
        Err(e) => {
            println!("[client] Không kết nối được {host}:{port}: {e}");
            return;
        }
    };
    // bộ hẹn giờ nhịp tim 30 giây
    let pc = Arc::clone(&c);
    let pinger = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let _ = pc.send(json!({"type": "ping"})).await;
        }
    });
    // READ_CONSOLE trong luồng phụ, chuyển từng dòng vào vòng lặp bất đồng bộ qua kênh
    let (tx, mut lines) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
        // hết dữ liệu nhập: tx bị hủy, lines.recv() trả về None
    });
    let mut room: Option<String> = None; // phòng hiện tại
    let mut eof = true;
    while let Some(line) = lines.recv().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        println!("[client] > {line}");
        let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
        let result = match cmd {
            "/join" if !rest.is_empty() => {
                room = Some(rest.to_string());
                c.send(json!({"type": "join", "room": rest})).await
            }
            "/leave" if !rest.is_empty() => c.send(json!({"type": "leave", "room": rest})).await,
            "/pm" if rest.contains(' ') => {
                let (to, text) = rest.split_once(' ').unwrap();
                c.send(json!({"type": "pm", "to": to, "text": text})).await
            }
            "/ping" => c.send(json!({"type": "ping"})).await,
            "/quit" => {
                eof = false;
                break;
            }
            _ if line.starts_with('/') => {
                println!("[client] Lệnh không hợp lệ");
                Ok(())
            }
            _ => match &room {
                None => {
                    println!("[client] Chưa chọn phòng: dùng /join tên_phòng");
                    Ok(())
                }
                Some(r) => c.send(json!({"type": "msg", "room": r, "text": line})).await,
            },
        };
        if let Err(e) = result {
            println!("[client] Gửi lỗi: {e}");
            eof = false;
            break;
        }
    }
    if eof {
        tokio::time::sleep(Duration::from_secs(1)).await; // hết dữ liệu nhập: chờ nhận nốt
    }
    pinger.abort();
    c.disconnect().await;
    println!("[client] Đã ngắt kết nối");
}
