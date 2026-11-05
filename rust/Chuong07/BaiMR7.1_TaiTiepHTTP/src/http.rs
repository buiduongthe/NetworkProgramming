//! Đọc phần đầu (dòng đầu + tiêu đề) của một thông điệp HTTP/1.1 – dùng chung cho server và client.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read};

/// Dòng đầu (request line hoặc status line) và bảng tiêu đề (tên viết thường -> giá trị).
pub struct Head {
    pub first: String,
    pub headers: HashMap<String, String>,
}

impl Head {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_ascii_lowercase()).map(String::as_str)
    }
}

/// Đọc tới dòng trống "\r\n" kết thúc phần tiêu đề.
pub fn read_head<R: Read>(r: &mut BufReader<R>) -> io::Result<Head> {
    let mut first = String::new();
    if r.read_line(&mut first)? == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "kết nối đóng trước khi có dữ liệu"));
    }
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    Ok(Head { first: first.trim_end().to_string(), headers })
}
