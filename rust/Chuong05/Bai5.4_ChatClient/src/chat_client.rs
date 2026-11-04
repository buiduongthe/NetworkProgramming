//! Lớp dịch vụ ChatClient bất đồng bộ (tokio).
//!
//! Cài đặt Mã giả 5.5 (Mục 5.3.3, Giáo trình Lập trình mạng).
//! Kế thừa: Mã giả 5.1 (khung JSON bất đồng bộ); SAFE_CONNECT_ASYNC = Mã giả 2.2 + AWAIT.
//! Lớp không tham chiếu tới điều khiển giao diện nào: tầng giao diện (ở đây là chat_cli.rs)
//! chỉ đăng ký hai sự kiện on_message, on_status.
//! Ánh xạ: cts ↔ CancelToken; send_lock ↔ tokio::sync::Mutex (khóa giữ luôn nửa ghi của socket);
//! RUN_BACKGROUND ↔ tokio::spawn.

use crate::khung_json_async::{recv_json_async, safe_connect_async, send_json_async, CancelToken, FrameError};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;

/// Kiểu của hai sự kiện cho tầng giao diện.
pub type OnMessage = Arc<dyn Fn(Value) + Send + Sync>;
pub type OnStatus = Arc<dyn Fn(String) + Send + Sync>;

/// STRUCT ChatClient { sock, cts, send_lock ← ASYNC_LOCK(), on_message, on_status } (dòng 1–2).
/// Socket được tách làm hai nửa: nửa đọc thuộc vòng nhận chạy nền, nửa ghi nằm trong khóa gửi,
/// nên muốn gửi bắt buộc phải lấy khóa trước (trình biên dịch Rust bảo đảm điều này).
pub struct ChatClient {
    writer: Arc<Mutex<Option<OwnedWriteHalf>>>, // send_lock + nửa ghi của sock; None = đã đóng
    cts: CancelToken,
    pub on_message: OnMessage,
    pub on_status: OnStatus,
}

impl ChatClient {
    pub fn new() -> ChatClient {
        ChatClient {
            writer: Arc::new(Mutex::new(None)),
            cts: CancelToken::new(),
            on_message: Arc::new(|_| {}),
            on_status: Arc::new(|_| {}),
        }
    }

    /// Mã giả 5.5, dòng 3–14: CONNECT(c, host, port, name).
    pub async fn connect(&mut self, host: &str, port: u16, name: &str) -> Result<(), String> {
        let sock = safe_connect_async(host, port, 3000).await.map_err(|e| e.to_string())?;
        self.cts = CancelToken::new(); // c.cts ← NEW_CANCELLATION()
        let (reader, mut writer) = sock.into_split();
        let hello = json!({"type": "hello", "name": name});
        if let Err(e) = send_json_async(&mut writer, &hello, &self.cts).await {
            // CLOSE(c.sock); RAISE e – không để rò socket: hai nửa bị hủy (drop) khi ra khỏi hàm
            return Err(e.to_string());
        }
        *self.writer.lock().await = Some(writer);
        // RUN_BACKGROUND(RECEIVE_LOOP(c)): không .await, vòng nhận chạy nền
        tokio::spawn(receive_loop(reader, self.cts.clone(), self.writer.clone(), self.on_message.clone(), self.on_status.clone()));
        (self.on_status)("connected".to_string()); // RAISE_EVENT(c.on_status, "connected")
        Ok(())
    }

    /// Mã giả 5.5, dòng 28–36: SEND_MESSAGE(c, room, text).
    pub async fn send_message(&self, room: &str, text: &str) -> Result<(), FrameError> {
        let mut guard = self.writer.lock().await; // ACQUIRE_ASYNC(c.send_lock): mỗi lúc gửi một khung
        let writer = guard.as_mut().ok_or(FrameError::Protocol("Chưa kết nối".to_string()))?;
        let m = json!({"type": "msg", "room": room, "text": text});
        send_json_async(writer, &m, &self.cts).await
        // RELEASE(c.send_lock): khóa tự nhả khi `guard` ra khỏi phạm vi, kể cả khi gửi lỗi (FINALLY)
    }

    /// Mã giả 5.5, dòng 37–39: DISCONNECT(c) – hủy trước, đóng socket sau.
    pub async fn disconnect(&self) {
        self.cts.cancel(); // CANCEL(c.cts): vòng nhận kết thúc ở nhánh Cancelled
        *self.writer.lock().await = None; // CLOSE(c.sock): bỏ nửa ghi; nửa đọc được vòng nhận bỏ khi thoát
    }
}

/// Mã giả 5.5, dòng 15–27: RECEIVE_LOOP(c).
async fn receive_loop(
    mut reader: OwnedReadHalf,
    ct: CancelToken,
    writer: Arc<Mutex<Option<OwnedWriteHalf>>>,
    on_message: OnMessage,
    on_status: OnStatus,
) {
    loop {
        match recv_json_async(&mut reader, &ct).await {
            Ok(msg) => on_message(msg), // RAISE_EVENT(c.on_message, msg)
            Err(FrameError::Cancelled) => return, // CATCH Cancelled: ngắt kết nối chủ động
            Err(e) => {
                // CATCH NetError, ConnectionClosed, ProtocolError, JsonError
                *writer.lock().await = None; // CLOSE(c.sock): giải phóng socket
                on_status(format!("lost: {e}"));
                return;
            }
        }
    }
}
