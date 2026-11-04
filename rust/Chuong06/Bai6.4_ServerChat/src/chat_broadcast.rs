//! Bài 6.4 – Server chat: cấu trúc dữ liệu dùng chung, phát tới phòng, gửi riêng và tác vụ ghi.
//!
//! Cài đặt Mã giả 6.6 (Mục 6.3.3, Giáo trình Lập trình mạng) cùng phần cấu trúc dữ liệu của Mã giả 6.5
//! (STRUCT Client, users, rooms, lock) và các thủ tục mà mã giả chỉ nêu tên: REGISTER, UNREGISTER,
//! JOIN_ROOM, LEAVE_ROOM. Dùng SEND_JSON_ASYNC của Mã giả 5.1 (async_frame.rs).
use crate::async_frame::send_json_async;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{mpsc, watch};

pub const OUTBOX_SIZE: usize = 256; // BOUNDED_QUEUE(256)

/// STRUCT Client { name, sock, outbox ← BOUNDED_QUEUE(256) }.
/// - outbox: đầu gửi của hộp thư đi; giá trị None là dấu kết thúc (outbox.close()).
/// - kill: tín hiệu CLOSE(c.sock); tác vụ phiên và tác vụ ghi cùng theo dõi để dừng ngay.
pub struct Client {
    pub name: String,
    pub outbox: mpsc::Sender<Option<Value>>,
    pub kill: watch::Sender<bool>,
}

impl Client {
    /// CLOSE(c.sock): báo cho tác vụ phiên và tác vụ ghi dừng; socket đóng khi hai nửa được giải phóng.
    pub fn close_sock(&self) {
        self.kill.send_replace(true);
    }

    /// outbox.close(): đặt dấu kết thúc None để tác vụ ghi dừng sau khi gửi hết tin đang chờ.
    pub fn close_outbox(&self) {
        if self.outbox.try_send(None).is_err() {
            self.close_sock(); // hộp thư đầy: bỏ tin còn lại vì client đã rời
        }
    }
}

/// users: tên → Client; rooms: tên phòng → tập tên.
#[derive(Default)]
pub struct Tables {
    pub users: HashMap<String, Arc<Client>>,
    pub rooms: HashMap<String, HashSet<String>>,
}

/// LOCK() bảo vệ users, rooms. Không thực hiện vào/ra (await) trong khi giữ khóa.
pub static LOCK: LazyLock<Mutex<Tables>> = LazyLock::new(|| Mutex::new(Tables::default()));

pub fn log(msg: &str) {
    println!("{msg}");
}

pub fn error(code: u16) -> Value {
    let text = match code {
        400 => "Sai cú pháp",
        403 => "Chưa tham gia phòng",
        404 => "Không có người nhận",
        409 => "Tên đã được sử dụng",
        _ => "Gửi quá nhanh", // 429
    };
    json!({"type": "error", "code": code, "text": text})
}

/// NOW(): thời điểm do server gán, dạng ISO 8601 UTC (ví dụ 2026-11-04T14:29:42.123Z).
pub fn now_ts() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let (secs, ms) = (d.as_secs() as i64, d.subsec_millis());
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Đổi số ngày kể từ 1970-01-01 sang năm-tháng-ngày (thuật toán "civil from days" của H. Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// REGISTER: thêm tên vào users nếu chưa có; trả về None nếu trùng tên.
/// Trả kèm đầu nhận của hộp thư và tín hiệu kill cho tác vụ ghi.
pub fn register(name: &str) -> Option<(Arc<Client>, mpsc::Receiver<Option<Value>>)> {
    let mut t = LOCK.lock().unwrap();
    if t.users.contains_key(name) {
        return None;
    }
    let (tx, rx) = mpsc::channel(OUTBOX_SIZE);
    let (kill, _) = watch::channel(false);
    let c = Arc::new(Client { name: name.to_string(), outbox: tx, kill });
    t.users.insert(name.to_string(), Arc::clone(&c));
    Some((c, rx))
}

/// UNREGISTER: xóa client khỏi users và mọi phòng, báo cho các phòng đó.
pub fn unregister(c: &Client) {
    let mut left = Vec::new();
    {
        let mut t = LOCK.lock().unwrap();
        t.users.remove(&c.name);
        for (r, members) in t.rooms.iter_mut() {
            if members.remove(&c.name) {
                left.push(r.clone());
            }
        }
        t.rooms.retain(|_, members| !members.is_empty());
    } // nhả khóa
    left.sort();
    for r in left {
        // phát ngoài khóa
        broadcast(&r, json!({"type": "sys", "text": format!("{} đã rời phòng {r}", c.name)}), None);
    }
}

pub fn join_room(c: &Client, room: &str) {
    LOCK.lock().unwrap().rooms.entry(room.to_string()).or_default().insert(c.name.clone());
    log(&format!("[server] {} vào phòng {room}", c.name));
    broadcast(room, json!({"type": "sys", "text": format!("{} đã vào phòng {room}", c.name)}), None);
}

pub fn leave_room(c: &Client, room: &str) {
    let member = {
        let mut t = LOCK.lock().unwrap();
        let member = t.rooms.get_mut(room).map(|m| m.remove(&c.name)).unwrap_or(false);
        t.rooms.retain(|_, members| !members.is_empty());
        member
    };
    if !member {
        enqueue(c, error(403));
        return;
    }
    log(&format!("[server] {} rời phòng {room}", c.name));
    broadcast(room, json!({"type": "sys", "text": format!("{} đã rời phòng {room}", c.name)}), None);
}

/// BROADCAST (Mã giả 6.6, dòng 1–10); sender None: tin hệ thống.
pub fn broadcast(room: &str, msg: Value, sender: Option<&Client>) {
    let targets: HashSet<String> = {
        // giữ khóa ngắn: chỉ sao chép danh sách thành viên (phòng chưa tồn tại → tập rỗng)
        let t = LOCK.lock().unwrap();
        t.rooms.get(room).cloned().unwrap_or_default() // COPY
    };
    if let Some(s) = sender {
        if !targets.contains(&s.name) {
            enqueue(s, error(403));
            return;
        }
    }
    for name in &targets {
        send_to(name, msg.clone(), None);
    }
}

/// SEND_TO (Mã giả 6.6, dòng 12–18).
pub fn send_to(name: &str, msg: Value, sender: Option<&Client>) {
    let c = LOCK.lock().unwrap().users.get(name).cloned(); // WITH lock DO c ← users[name]
    match (c, sender) {
        (Some(c), _) => enqueue(&c, msg),
        (None, Some(s)) => enqueue(s, error(404)),
        (None, None) => {}
    }
}

/// ENQUEUE (Mã giả 6.6, dòng 20–24): try_put không bao giờ chờ.
pub fn enqueue(c: &Client, msg: Value) {
    if let Err(mpsc::error::TrySendError::Full(_)) = c.outbox.try_send(Some(msg)) {
        // hàng đợi đầy: client chậm
        log(&format!("[server] Ngắt client chậm {}", c.name));
        c.close_sock(); // CLOSE(c.sock)
    }
}

/// WRITER (Mã giả 6.6, dòng 26–34): tác vụ ghi duy nhất của mỗi socket.
pub async fn writer(c: Arc<Client>, mut outbox: mpsc::Receiver<Option<Value>>, mut sock: OwnedWriteHalf) {
    let mut kill = c.kill.subscribe();
    loop {
        let msg = tokio::select! {
            m = outbox.recv() => m,                // AWAIT c.outbox.take_async()
            _ = kill.wait_for(|&k| k) => break,    // CLOSE(c.sock) từ nơi khác
        };
        let Some(Some(msg)) = msg else { break }; // NULL: hộp thư đã đóng
        if send_json_async(&mut sock, &msg).await.is_err() {
            // NetError
            c.close_sock(); // phiên sẽ kết thúc theo
            break;
        }
    }
    // nửa ghi của socket được giải phóng ở đây
}
