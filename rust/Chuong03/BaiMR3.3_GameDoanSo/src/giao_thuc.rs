//! Giao thức trò chơi đoán số: khung JSON có tiền tố độ dài, bộ sinh số giả ngẫu nhiên.
//!
//! Kế thừa Bài 3.2 (Mã giả 3.3–3.6): khung = 4 byte độ dài (big-endian) + nội dung JSON mã hóa UTF-8.
//! Các thông điệp (mỗi thông điệp là một đối tượng JSON có trường "type"):
//!   client -> server: hello {name}, guess {value}, bye
//!   server -> client: welcome, start, turn, result, timeout, left, error {code, msg}, game_over, bye

use serde_json::Value;
use std::io::{self, Read, Write};
use std::net::TcpStream;

pub const MAX_FRAME: usize = 64 * 1024; // thông điệp trò chơi rất nhỏ: giới hạn 64 KiB chống khung khổng lồ

/// Mã giả 3.4 (RECV_EXACT): nhận đúng n byte; đối tác đóng giữa chừng -> lỗi UnexpectedEof.
pub fn recv_exact(s: &mut TcpStream, n: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; n];
    s.read_exact(&mut buf)?;
    Ok(buf)
}

/// Mã giả 3.6 (SEND_FRAME): 4 byte độ dài + nội dung, gửi hết bằng write_all (SEND_ALL).
pub fn send_frame(mut s: &TcpStream, payload: &[u8]) -> io::Result<()> {
    let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(payload);
    s.write_all(&frame)
}

/// Mã giả 3.6 (RECV_FRAME): đọc tiêu đề, kiểm tra giới hạn, đọc đúng n byte.
pub fn recv_frame(s: &mut TcpStream) -> io::Result<Vec<u8>> {
    let h = recv_exact(s, 4)?;
    let n = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as usize;
    if n > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("Khung quá lớn: {n} byte")));
    }
    recv_exact(s, n)
}

pub fn send_json(s: &TcpStream, obj: &Value) -> io::Result<()> {
    send_frame(s, serde_json::to_string(obj).unwrap().as_bytes())
}

/// Giải mã nội dung khung; trả None nếu không phải đối tượng JSON hợp lệ.
pub fn decode_json(payload: &[u8]) -> Option<Value> {
    let v: Value = serde_json::from_slice(payload).ok()?;
    v.is_object().then_some(v)
}

pub fn recv_json(s: &mut TcpStream) -> io::Result<Option<Value>> {
    Ok(decode_json(&recv_frame(s)?))
}

/// Lấy trường dạng chữ để in: chuỗi in nguyên văn, kiểu khác in dạng JSON, thiếu thì "None".
pub fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

/// Biểu diễn giá trị kiểu repr() của Python, để thông điệp giống bản Python.
pub fn repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'")),
        Value::Array(a) => format!("[{}]", a.iter().map(repr).collect::<Vec<_>>().join(", ")),
        Value::Object(o) => {
            let items: Vec<String> = o.iter().map(|(k, v)| format!("'{k}': {}", repr(v))).collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

/// Bộ sinh số giả ngẫu nhiên đồng dư tuyến tính (LCG) rất đơn giản.
/// Cùng công thức với bản Python và Go nên với cùng hạt giống (seed) cho cùng dãy số.
/// Không dùng cho mục đích bảo mật.
pub struct Lcg {
    x: u64,
}

impl Lcg {
    pub fn new(seed: i64) -> Lcg {
        Lcg { x: (seed as u64) & 0x7FFF_FFFF }
    }
    /// Số nguyên ngẫu nhiên trong [lo, hi].
    pub fn next_int(&mut self, lo: i64, hi: i64) -> i64 {
        self.x = (self.x * 1103515245 + 12345) & 0x7FFF_FFFF;
        lo + (self.x >> 8) as i64 % (hi - lo + 1)
    }
}
