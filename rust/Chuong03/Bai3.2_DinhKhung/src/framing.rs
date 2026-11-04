//! Bài 3.2 – Thư viện định khung thông điệp trên dòng byte TCP.
//!
//! Cài đặt Mã giả 3.3 (SEND_ALL, Mục 3.2.4), Mã giả 3.4 (RECV_EXACT), Mã giả 3.5 (READ_LINE)
//! và Mã giả 3.6 (SEND_FRAME, RECV_FRAME, SEND_JSON, RECV_JSON) – Mục 3.4.3 Giáo trình Lập trình mạng.
//! Khung = 4 byte độ dài (big-endian, không dấu) + nội dung; thông điệp JSON mã hóa UTF-8.

use serde_json::Value;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;

pub const MAX_FRAME: u32 = 1_048_576; // 1 MiB

/// Các lỗi của thư viện định khung (tương ứng các ngoại lệ trong mã giả).
#[derive(Debug)]
pub enum FrameError {
    /// Đối tác đóng kết nối; `received` = số byte đã nhận của khung (hoặc tệp) dở dang.
    ConnectionClosed { received: usize, message: String },
    /// Đối tác vi phạm định dạng giao thức (khung quá lớn, dòng quá dài...).
    Protocol(String),
    /// Nội dung khung không phải JSON hợp lệ mã hóa UTF-8.
    Json(String),
    /// Lỗi vào/ra của hệ điều hành (kết nối bị đặt lại, hết thời gian chờ...).
    Io(io::Error),
}

impl FrameError {
    /// ConnectionClosed(received) với thông báo mặc định.
    pub fn closed(received: usize) -> FrameError {
        FrameError::ConnectionClosed { received, message: format!("Kết nối đóng giữa chừng (đã nhận {received} byte)") }
    }
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FrameError::ConnectionClosed { message, .. } => write!(f, "{message}"),
            FrameError::Protocol(m) | FrameError::Json(m) => write!(f, "{m}"),
            FrameError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Cho phép dùng toán tử `?` với lỗi io::Error bên trong các hàm trả FrameError.
impl From<io::Error> for FrameError {
    fn from(e: io::Error) -> Self {
        FrameError::Io(e)
    }
}

/// Mã giả 3.3: SEND có thể gửi ít hơn yêu cầu nên lặp đến khi hết (tương đương `write_all`).
pub fn send_all(s: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let mut sent = 0;
    while sent < data.len() {
        let n = s.write(&data[sent..])?; // có thể gửi ít hơn yêu cầu
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "SEND trả về 0 byte"));
        }
        sent += n;
    }
    Ok(())
}

/// Mã giả 3.4: nhận đúng n byte; báo ConnectionClosed kèm số byte đã nhận nếu đóng giữa chừng.
pub fn recv_exact(s: &mut TcpStream, n: usize) -> Result<Vec<u8>, FrameError> {
    let mut buf = vec![0u8; n];
    let mut got = 0;
    while got < n {
        let k = s.read(&mut buf[got..])?; // RECV(s, n - LENGTH(buf))
        if k == 0 {
            return Err(FrameError::closed(got));
        }
        got += k;
    }
    Ok(buf)
}

/// Mã giả 3.5: STRUCT Reader { sock, pending } – bộ đệm tồn tại giữa các lần READ_LINE.
pub struct Reader {
    pub sock: TcpStream,
    pub pending: Vec<u8>,
}

impl Reader {
    pub fn new(sock: TcpStream) -> Reader {
        Reader { sock, pending: Vec::new() } // pending ← EMPTY_BYTES
    }
}

/// Mã giả 3.5: đọc một dòng kết thúc bằng LF hoặc CRLF; trả `None` khi đối tác đã đóng.
pub fn read_line(r: &mut Reader, max_len: usize) -> Result<Option<String>, FrameError> {
    loop {
        let found = r.pending.iter().position(|&b| b == b'\n'); // i ← FIND(r.pending, '\n')
        if let Some(i) = found {
            if i > max_len {
                return Err(FrameError::Protocol("Dòng quá dài".to_string())); // đã thấy LF nhưng dòng quá dài
            }
            let mut line: Vec<u8> = r.pending.drain(..=i).collect(); // lấy dòng kèm '\n' ra khỏi pending
            line.pop(); // bỏ '\n'
            if line.last() == Some(&b'\r') {
                line.pop(); // STRIP_CR
            }
            return match String::from_utf8(line) {
                Ok(text) => Ok(Some(text)),
                Err(_) => Err(FrameError::Protocol("Dòng không phải UTF-8 hợp lệ".to_string())),
            };
        }
        if r.pending.len() > max_len {
            return Err(FrameError::Protocol("Dòng quá dài".to_string()));
        }
        let mut chunk = [0u8; 4096];
        let n = r.sock.read(&mut chunk)?;
        if n == 0 {
            return Ok(None); // đối tác đã đóng
        }
        r.pending.extend_from_slice(&chunk[..n]);
    }
}

/// Mã giả 3.6: ghép 4 byte độ dài với nội dung rồi gửi một lần.
pub fn send_frame(s: &mut TcpStream, payload: &[u8]) -> io::Result<()> {
    let mut frame = (payload.len() as u32).to_be_bytes().to_vec(); // UINT32_BE(LENGTH(payload))
    frame.extend_from_slice(payload);
    send_all(s, &frame)
}

/// Mã giả 3.6: nhận đủ tiêu đề, kiểm tra MAX_FRAME, nhận đúng n byte nội dung.
pub fn recv_frame(s: &mut TcpStream) -> Result<Vec<u8>, FrameError> {
    let head = recv_exact(s, 4)?;
    let n = u32::from_be_bytes([head[0], head[1], head[2], head[3]]); // FROM_UINT32_BE
    if n > MAX_FRAME {
        return Err(FrameError::Protocol(format!("Khung quá lớn: {n}")));
    }
    match recv_exact(s, n as usize) {
        Ok(data) => Ok(data),
        // đã nhận xong tiêu đề: cộng thêm 4 byte
        Err(FrameError::ConnectionClosed { received, .. }) => Err(FrameError::closed(4 + received)),
        Err(e) => Err(e),
    }
}

/// JSON_ENCODE: dạng gọn, không thêm khoảng trắng, giữ nguyên chữ tiếng Việt (không thoát \uXXXX).
pub fn json_encode(obj: &Value) -> String {
    obj.to_string()
}

/// DECODE(…, "UTF-8") rồi JSON_DECODE; mọi lỗi giải mã đều quy về FrameError::Json.
pub fn json_decode(payload: &[u8]) -> Result<Value, FrameError> {
    let text = std::str::from_utf8(payload).map_err(|e| FrameError::Json(e.to_string()))?;
    serde_json::from_str(text).map_err(|e| FrameError::Json(e.to_string()))
}

/// Mã giả 3.6 – SEND_JSON.
pub fn send_json(s: &mut TcpStream, obj: &Value) -> io::Result<()> {
    send_frame(s, json_encode(obj).as_bytes())
}

/// Mã giả 3.6 – RECV_JSON.
pub fn recv_json(s: &mut TcpStream) -> Result<Value, FrameError> {
    json_decode(&recv_frame(s)?)
}

/// In một giá trị JSON để đọc: chuỗi in không kèm dấu nháy, giá trị khác in dạng JSON.
pub fn show(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
