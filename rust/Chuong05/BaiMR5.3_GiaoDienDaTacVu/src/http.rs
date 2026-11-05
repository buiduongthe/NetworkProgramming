//! HTTP/1.1 tối giản trên tokio (đủ cho bài: không dùng crate máy chủ web như hyper, axum).
//! Mỗi kết nối chỉ phục vụ một yêu cầu rồi đóng ("Connection: close") cho đơn giản; riêng luồng SSE
//! giữ kết nối mở và gửi dần từng sự kiện.

use std::collections::HashMap;
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// Một yêu cầu đã phân tích: phương thức, đường dẫn (không gồm ?…), các tham số truy vấn.
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
}

impl Request {
    pub fn get(&self, k: &str) -> &str {
        self.query.get(k).map(String::as_str).unwrap_or("")
    }
    pub fn num(&self, k: &str) -> i64 {
        self.get(k).parse().unwrap_or(0)
    }
}

/// Giải mã %XX (và '+' = dấu cách) trong chuỗi truy vấn, ví dụ encodeURIComponent của JavaScript.
pub fn percent_decode(s: &str) -> String {
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 3;
                    continue;
                }
                _ => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Đọc dòng yêu cầu và các tiêu đề (bỏ qua phần thân – API của bài chỉ dùng tham số truy vấn).
pub async fn read_request<R: AsyncRead + Unpin>(r: &mut BufReader<R>) -> io::Result<Request> {
    let mut line = String::new();
    if r.read_line(&mut line).await? == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut body_len = 0usize;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).await? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                body_len = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; body_len.min(64 * 1024)];
    r.read_exact(&mut body).await?; // đọc bỏ phần thân (nếu có)
    let (path, qs) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let query = qs
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect();
    Ok(Request { method, path: path.to_string(), query })
}

/// Gửi một phản hồi đầy đủ rồi kết thúc (Connection: close).
pub async fn respond<W: AsyncWrite + Unpin>(w: &mut W, status: &str, ctype: &str, body: &[u8]) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        body.len()
    );
    w.write_all(head.as_bytes()).await?;
    w.write_all(body).await?;
    w.flush().await
}

/// Client HTTP tối giản: gửi GET và trả về (luồng đọc đã qua phần tiêu đề, Content-Length).
pub async fn get(addr: &str, path: &str) -> io::Result<(BufReader<TcpStream>, u64)> {
    let mut s = TcpStream::connect(addr).await?;
    s.write_all(format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").as_bytes()).await?;
    let mut r = BufReader::new(s);
    let mut status = String::new();
    r.read_line(&mut status).await?;
    if !status.contains(" 200 ") {
        return Err(io::Error::other(format!("máy chủ trả {}", status.trim())));
    }
    let mut len = 0;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).await? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
    }
    Ok((r, len))
}

#[cfg(test)]
mod tests {
    #[test]
    fn giai_ma_phan_tram() {
        assert_eq!(super::percent_decode("Trang%20b%E1%BB%8B%20ch%E1%BA%B7n"), "Trang bị chặn");
        assert_eq!(super::percent_decode("a+b%2"), "a b%2");
    }
}
