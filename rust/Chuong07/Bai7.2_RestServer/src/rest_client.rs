//! Bài 7.2 – Client thử server REST: gửi nhiều yêu cầu trên MỘT kết nối giữ lại (keep-alive).
//!
//! Dùng lại Reader, READ_LINE, READ_HEADERS, READ_EXACT của Mã giả 7.1 (Mục 7.2.1).
//! Mỗi dòng nhập có dạng:  PHƯƠNG_THỨC ĐƯỜNG_DẪN [THÂN_JSON]
//! ví dụ  GET /api/v1/terms/TCP   hoặc   POST /api/v1/terms {"term":"QUIC","text":"..."}
//!
//! Chạy:  cargo run -- client [host] [port]      (mặc định 127.0.0.1 8080)
use crate::port_arg;
use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Mã giả 3.5 – Reader { sock, pending }.
struct Reader {
    sock: TcpStream,
    pending: Vec<u8>,
}

impl Reader {
    fn read_line(&mut self, max_len: usize) -> io::Result<Option<String>> {
        loop {
            if let Some(i) = self.pending.iter().position(|&b| b == b'\n') {
                let mut line: Vec<u8> = self.pending.drain(..=i).collect();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(Some(String::from_utf8_lossy(&line).into_owned()));
            }
            if self.pending.len() > max_len {
                return Err(io::Error::other("Dòng quá dài"));
            }
            let mut chunk = [0u8; 4096];
            let n = self.sock.read(&mut chunk)?;
            if n == 0 {
                return Ok(None);
            }
            self.pending.extend_from_slice(&chunk[..n]);
        }
    }

    fn read_exact(&mut self, n: usize) -> io::Result<Vec<u8>> {
        let take = n.min(self.pending.len());
        let mut data: Vec<u8> = self.pending.drain(..take).collect();
        while data.len() < n {
            let mut chunk = vec![0u8; n - data.len()];
            let k = self.sock.read(&mut chunk)?;
            if k == 0 {
                return Err(io::Error::other("Kết nối đóng giữa chừng"));
            }
            data.extend_from_slice(&chunk[..k]);
        }
        Ok(data)
    }
}

/// Mã giả 7.1 – READ_HEADERS: đọc đến dòng trống; kết nối đóng giữa chừng là lỗi (ConnectionClosed).
fn read_headers(r: &mut Reader) -> io::Result<HashMap<String, String>> {
    let mut hdr = HashMap::new();
    loop {
        let Some(line) = r.read_line(8192)? else {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Kết nối đóng giữa phần tiêu đề"));
        };
        if line.is_empty() {
            break;
        }
        let (k, v) = line.split_once(':').unwrap_or((&line, ""));
        hdr.insert(k.trim().to_lowercase(), v.trim().to_string());
    }
    Ok(hdr)
}

fn session(host: &str, port: u16) -> io::Result<()> {
    // SAFE_CONNECT (rút gọn): địa chỉ đầu tiên, chờ kết nối 3 giây
    let addr = (host, port).to_socket_addrs()?.next().ok_or(io::Error::other("không phân giải được tên"))?;
    let s = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut r = Reader { sock: s, pending: Vec::new() };
    println!("[client] Đã kết nối {host}:{port} (một kết nối cho mọi yêu cầu)");
    for line in io::stdin().lock().lines() {
        let line = line?;
        let parts: Vec<&str> = line.trim().splitn(3, ' ').collect();
        if parts.len() < 2 {
            continue;
        }
        let (method, path) = (parts[0], parts[1]);
        let body = parts.get(2).map(|b| b.as_bytes()).unwrap_or(b"");
        let mut h = format!("{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\n");
        if !body.is_empty() {
            h += &format!("Content-Type: application/json\r\nContent-Length: {}\r\n", body.len());
        }
        h += "\r\n";
        r.sock.write_all(&[h.as_bytes(), body].concat())?;
        let Some(status_line) = r.read_line(8192)? else {
            println!("[client] Server đã đóng kết nối");
            break;
        };
        let hdr = read_headers(&mut r)?;
        let n: usize = hdr.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
        let data = r.read_exact(n)?;
        println!("[client] {method} {path} -> {status_line}");
        if let Some(loc) = hdr.get("location") {
            println!("[client]   Location: {loc}");
        }
        if !data.is_empty() {
            println!("[client]   {}", String::from_utf8_lossy(&data));
        }
    }
    Ok(()) // CLOSE(s) khi r ra khỏi phạm vi
}

pub fn run(args: &[String]) {
    let host = args.first().map(String::as_str).unwrap_or("127.0.0.1");
    let port = port_arg(args, 1, 8080);
    if let Err(e) = session(host, port) {
        println!("[client] Lỗi: {e}");
    }
    println!("[client] Đã đóng kết nối");
}
