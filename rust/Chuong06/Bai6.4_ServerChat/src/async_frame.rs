//! Bài 6.4 – Các hàm gửi, nhận khung JSON bất đồng bộ (Mã giả 5.1, Mục 5.2.4) và PARSE_ENDPOINT (Mã giả 2.1).
//!
//! Các hàm nhận một nửa đọc/ghi bất kỳ (AsyncRead/AsyncWrite) vì server và client tách socket
//! thành hai nửa: tác vụ phiên giữ nửa đọc, tác vụ ghi giữ nửa ghi.
use serde_json::Value;
use std::fmt;
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_FRAME: u32 = 1048576; // Mã giả 3.6: 1 MiB

/// Các loại lỗi khi nhận/gửi khung.
#[derive(Debug)]
pub enum NetError {
    /// Đối tác đóng kết nối; giá trị = số byte của khung đang nhận dở.
    ConnectionClosed(usize),
    /// Vi phạm định dạng khung (khung quá lớn, JSON sai).
    Protocol(String),
    /// Lỗi mạng của hệ điều hành.
    Io(io::Error),
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetError::ConnectionClosed(n) => write!(f, "Kết nối bị đóng sau {n} byte"),
            NetError::Protocol(m) => write!(f, "{m}"),
            NetError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for NetError {
    fn from(e: io::Error) -> Self {
        NetError::Io(e)
    }
}

/// RECV_EXACT_ASYNC: nhận đủ n byte; đóng giữa chừng → ConnectionClosed(số byte đã nhận).
pub async fn recv_exact_async<R: AsyncRead + Unpin>(r: &mut R, n: usize) -> Result<Vec<u8>, NetError> {
    let mut buf = vec![0u8; n];
    let mut got = 0;
    while got < n {
        let k = r.read(&mut buf[got..]).await?;
        if k == 0 {
            return Err(NetError::ConnectionClosed(got));
        }
        got += k;
    }
    Ok(buf)
}

/// RECV_JSON_ASYNC (Mã giả 5.1, dòng 12–20).
pub async fn recv_json_async<R: AsyncRead + Unpin>(r: &mut R) -> Result<Value, NetError> {
    let h = recv_exact_async(r, 4).await?;
    let n = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    if n > MAX_FRAME {
        return Err(NetError::Protocol("Khung quá lớn".into()));
    }
    let data = match recv_exact_async(r, n as usize).await {
        Ok(d) => d,
        Err(NetError::ConnectionClosed(k)) => return Err(NetError::ConnectionClosed(4 + k)), // đã nhận xong tiêu đề
        Err(e) => return Err(e),
    };
    let obj: Value = serde_json::from_slice(&data).map_err(|e| NetError::Protocol(e.to_string()))?;
    if !obj.is_object() {
        return Err(NetError::Protocol("Thông điệp không phải đối tượng JSON".into()));
    }
    Ok(obj)
}

/// SEND_JSON_ASYNC (Mã giả 5.1, dòng 22–24): ghép 4 byte độ dài rồi gửi cả khung.
pub async fn send_json_async<W: AsyncWrite + Unpin>(w: &mut W, obj: &Value) -> Result<(), NetError> {
    let p = serde_json::to_vec(obj).expect("JSON hợp lệ");
    let mut frame = (p.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&p);
    w.write_all(&frame).await?; // SEND_ALL_ASYNC
    Ok(())
}

/// In thông báo lỗi rồi thoát chương trình (giống sys.exit của Python).
pub fn exit_with(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

/// Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > mặc định.
pub fn parse_endpoint(args: &[String], default_host: &str, default_port: u16) -> (String, u16) {
    let mut host = std::env::var("APP_HOST").unwrap_or(default_host.to_string());
    let mut port = std::env::var("APP_PORT").unwrap_or(default_port.to_string());
    if args.len() >= 1 {
        host = args[0].clone();
    }
    if args.len() >= 2 {
        port = args[1].clone();
    }
    let p: i64 = match port.trim().parse() {
        Ok(p) => p,
        Err(_) => exit_with(&format!("Cổng phải là số nguyên: {port}")),
    };
    if !(1..=65535).contains(&p) {
        exit_with(&format!("Cổng không hợp lệ: {p}"));
    }
    (host, p as u16)
}
