//! Bài 5.4 – Các hàm gửi, nhận khung JSON bất đồng bộ (tokio), chép từ Bài 5.1.
//!
//! Cài đặt Mã giả 5.1 (Mục 5.2.4, Giáo trình Lập trình mạng).
//! Kế thừa: Mã giả 3.3 (SEND_ALL), 3.4 (RECV_EXACT), 3.6 (khung JSON), 2.1 (PARSE_ENDPOINT),
//! 2.2 (SAFE_CONNECT) – giữ nguyên logic, thay lời gọi vào/ra bằng phiên bản có `.await`.
//! Tín hiệu hủy `ct` được cài bằng kiểu `CancelToken` nhỏ bên dưới: mỗi điểm `.await` chờ mạng
//! đồng thời chờ tín hiệu hủy (`tokio::select!`), bị hủy thì kết thúc bằng `FrameError::Cancelled`.
//!
//! Tệp này được các chương trình trong cùng gói dùng chung (`mod khung_json_async;`).
#![allow(dead_code)] // thư viện dùng chung: một số hàm không dùng ở mọi bài

use serde_json::Value;
use std::env;
use std::fmt;
use std::io;
use std::process;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio::time::{timeout, Duration};

pub const MAX_FRAME: u32 = 1_048_576; // 1 MiB (Mã giả 3.6)

/// Tín hiệu hủy `ct` (cancellation token) tối giản.
/// `cancel()` = CANCEL(cts); `cancelled()` là tác vụ hoàn tất khi tín hiệu đã được phát.
/// Sao chép (`clone`) token để nhiều tác vụ cùng theo dõi một tín hiệu.
#[derive(Clone)]
pub struct CancelToken {
    tx: Arc<watch::Sender<bool>>,
}

impl CancelToken {
    pub fn new() -> CancelToken {
        let (tx, _rx) = watch::channel(false);
        CancelToken { tx: Arc::new(tx) }
    }

    /// CANCEL(cts): phát tín hiệu hủy cho mọi nơi đang theo dõi.
    pub fn cancel(&self) {
        self.tx.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.tx.borrow()
    }

    /// Chờ đến khi tín hiệu hủy được phát (trả về ngay nếu đã hủy từ trước).
    pub async fn cancelled(&self) {
        let mut rx = self.tx.subscribe();
        let _ = rx.wait_for(|c| *c).await;
    }
}

/// Các lỗi của khung JSON bất đồng bộ.
#[derive(Debug)]
pub enum FrameError {
    /// Đối tác đóng kết nối; số kèm theo = số byte của khung đã nhận được.
    ConnectionClosed(usize),
    /// Vi phạm giao thức, ví dụ khung quá lớn.
    Protocol(String),
    /// Nội dung khung không phải JSON hợp lệ mã hóa UTF-8.
    Json(String),
    /// Tín hiệu hủy `ct` được phát trong lúc chờ.
    Cancelled,
    /// Lỗi vào/ra của hệ điều hành.
    Io(io::Error),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FrameError::ConnectionClosed(n) => write!(f, "Kết nối đã đóng (đã nhận {n} byte)"),
            FrameError::Protocol(m) | FrameError::Json(m) => write!(f, "{m}"),
            FrameError::Cancelled => write!(f, "Cancelled"),
            FrameError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for FrameError {
    fn from(e: io::Error) -> Self {
        FrameError::Io(e)
    }
}

/// Lỗi mạng đã phân loại (Mã giả 2.2): code, retry.
#[derive(Debug)]
pub struct NetError {
    pub code: &'static str,
    pub retry: bool,
    pub cause: String,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.cause.is_empty() {
            write!(f, "{}", self.code)
        } else {
            write!(f, "{}: {}", self.code, self.cause)
        }
    }
}

/// ENCODE(JSON_ENCODE(obj), "UTF-8"): giữ nguyên chữ có dấu, không thêm khoảng trắng.
pub fn json_bytes(obj: &Value) -> Vec<u8> {
    obj.to_string().into_bytes()
}

/// UINT32_BE(LENGTH(p)) + p.
pub fn make_frame(obj: &Value) -> Vec<u8> {
    let p = json_bytes(obj);
    let mut frame = (p.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&p);
    frame
}

/// Mã giả 5.1, dòng 1–10: RECV_EXACT_ASYNC(s, n, ct).
pub async fn recv_exact_async(s: &mut (impl AsyncRead + Unpin), n: usize, ct: &CancelToken) -> Result<Vec<u8>, FrameError> {
    let mut buf = vec![0u8; n];
    let mut got = 0;
    while got < n {
        // AWAIT RECV_ASYNC(s, n − LENGTH(buf), ct): chờ dữ liệu hoặc tín hiệu hủy, cái nào đến trước
        let k = tokio::select! {
            _ = ct.cancelled() => return Err(FrameError::Cancelled),
            r = s.read(&mut buf[got..]) => r?,
        };
        if k == 0 {
            return Err(FrameError::ConnectionClosed(got)); // đối tác đã đóng kết nối
        }
        got += k;
    }
    Ok(buf)
}

/// Mã giả 5.1, dòng 12–20: RECV_JSON_ASYNC(s, ct).
pub async fn recv_json_async(s: &mut (impl AsyncRead + Unpin), ct: &CancelToken) -> Result<Value, FrameError> {
    let head = recv_exact_async(s, 4, ct).await?;
    let n = u32::from_be_bytes([head[0], head[1], head[2], head[3]]); // FROM_UINT32_BE
    if n > MAX_FRAME {
        return Err(FrameError::Protocol("Khung quá lớn".to_string()));
    }
    let data = match recv_exact_async(s, n as usize, ct).await {
        Ok(d) => d,
        Err(FrameError::ConnectionClosed(k)) => return Err(FrameError::ConnectionClosed(4 + k)), // đã nhận xong tiêu đề
        Err(e) => return Err(e),
    };
    let text = std::str::from_utf8(&data).map_err(|e| FrameError::Json(e.to_string()))?;
    serde_json::from_str(text).map_err(|e| FrameError::Json(e.to_string()))
}

/// Mã giả 5.1, dòng 22–24: SEND_JSON_ASYNC(s, obj, ct).
pub async fn send_json_async(s: &mut (impl AsyncWrite + Unpin), obj: &Value, ct: &CancelToken) -> Result<(), FrameError> {
    let frame = make_frame(obj);
    // AWAIT SEND_ALL_ASYNC: write_all lặp đến khi gửi hết, nhường luồng khi bộ đệm gửi đầy
    tokio::select! {
        _ = ct.cancelled() => Err(FrameError::Cancelled),
        r = s.write_all(&frame) => Ok(r?),
    }
}

/// Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_PORT > giá trị mặc định.
pub fn parse_endpoint(args: &[String], default_host: &str, default_port: u16) -> (String, u16) {
    let mut host = default_host.to_string();
    let mut port = env::var("APP_PORT").unwrap_or_else(|_| default_port.to_string());
    if args.len() >= 1 {
        host = args[0].clone();
    }
    if args.len() >= 2 {
        port = args[1].clone();
    }
    let p: i64 = match port.trim().parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Cổng phải là số nguyên: {port}");
            process::exit(1);
        }
    };
    if !(1..=65535).contains(&p) {
        eprintln!("Cổng không hợp lệ: {p}");
        process::exit(1);
    }
    (host, p as u16)
}

/// SAFE_CONNECT_ASYNC = Mã giả 2.2 + AWAIT.
pub async fn safe_connect_async(host: &str, port: u16, timeout_ms: u64) -> Result<TcpStream, NetError> {
    let addrs: Vec<_> = match tokio::net::lookup_host((host, port)).await {
        // RESOLVE
        Ok(it) => it.collect(),
        Err(e) => return Err(NetError { code: "NOT_FOUND", retry: false, cause: e.to_string() }),
    };
    let mut refused = 0;
    let mut last_error = String::new();
    for addr in &addrs {
        match timeout(Duration::from_millis(timeout_ms), TcpStream::connect(addr)).await {
            Ok(Ok(s)) => return Ok(s),
            Ok(Err(e)) => {
                if e.kind() == io::ErrorKind::ConnectionRefused {
                    refused += 1;
                }
                last_error = e.to_string();
            }
            Err(_) => last_error = format!("hết thời gian chờ {timeout_ms} ms"), // hết giờ
        }
    }
    if refused > 0 && refused == addrs.len() {
        return Err(NetError { code: "REFUSED", retry: false, cause: last_error });
    }
    Err(NetError { code: "UNREACHABLE", retry: true, cause: last_error })
}
