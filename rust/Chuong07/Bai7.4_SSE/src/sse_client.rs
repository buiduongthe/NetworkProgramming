//! Bài 7.4 – Bộ đọc luồng Server-Sent Events trên socket TCP.
//!
//! Cài đặt Mã giả 7.5 (SSE_READ, Mục 7.3.3, Giáo trình Lập trình mạng).
//! Kế thừa: Mã giả 7.1 (gửi GET, đọc dòng trạng thái và tiêu đề, giải mã chunked), 3.5 (READ_LINE).
//! Khi phản hồi dùng chunked, bộ đọc dòng được đặt phía sau lớp giải mã khối, để các dòng độ dài khối
//! không lẫn vào luồng sự kiện.
//!
//! Chạy:  cargo run -- client [host] [port] [đường_dẫn]   (mặc định 127.0.0.1 8080 /events)
use crate::port_arg;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Mã giả 3.5 – Reader { sock, pending }; `sock` là bất kỳ nguồn nào đọc được (kiểu cài đặt `Read`):
/// socket TCP hoặc lớp giải mã khối ChunkedSource.
pub struct Reader<S: Read> {
    sock: S,
    pending: Vec<u8>,
}

impl<S: Read> Reader<S> {
    pub fn new(sock: S) -> Self {
        Reader { sock, pending: Vec::new() }
    }
}

/// Mã giả 3.5 – READ_LINE (None khi nguồn đã hết).
fn read_line<S: Read>(r: &mut Reader<S>, max_len: usize) -> io::Result<Option<String>> {
    loop {
        if let Some(i) = r.pending.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = r.pending.drain(..=i).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(String::from_utf8_lossy(&line).into_owned()));
        }
        if r.pending.len() > max_len {
            return Err(io::Error::other("Dòng quá dài"));
        }
        let mut chunk = [0u8; 4096];
        let n = r.sock.read(&mut chunk)?;
        if n == 0 {
            return Ok(None);
        }
        r.pending.extend_from_slice(&chunk[..n]);
    }
}

fn read_exact<S: Read>(r: &mut Reader<S>, n: usize) -> io::Result<Vec<u8>> {
    let take = n.min(r.pending.len());
    let mut data: Vec<u8> = r.pending.drain(..take).collect();
    while data.len() < n {
        let mut chunk = vec![0u8; n - data.len()];
        let k = r.sock.read(&mut chunk)?;
        if k == 0 {
            return Err(io::Error::other("Kết nối đóng giữa chừng"));
        }
        data.extend_from_slice(&chunk[..k]);
    }
    Ok(data)
}

/// Mã giả 7.1 – READ_HEADERS: đọc đến dòng trống; kết nối đóng giữa chừng là lỗi (ConnectionClosed).
fn read_headers<S: Read>(r: &mut Reader<S>) -> io::Result<HashMap<String, String>> {
    let mut hdr = HashMap::new();
    loop {
        let Some(line) = read_line(r, 8192)? else {
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

/// Lớp giải mã khối (READ_CHUNKED của Mã giả 7.1 viết dạng luồng): read() trả dữ liệu thân thuần.
struct ChunkedSource {
    r: Reader<TcpStream>,
    left: usize, // số byte còn lại của khối hiện tại
    done: bool,
}

impl Read for ChunkedSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.done {
            return Ok(0);
        }
        if self.left == 0 {
            let line = read_line(&mut self.r, 1024)?.ok_or(io::Error::other("Kết nối đóng giữa thân chunked"))?;
            let size = line.split(';').next().unwrap_or("").trim(); // bỏ phần mở rộng sau ';'
            let size = usize::from_str_radix(size, 16).map_err(|_| io::Error::other("Kích thước khối sai"))?;
            if size == 0 {
                // khối 0: hết thân, bỏ trailer
                read_headers(&mut self.r)?;
                self.done = true;
                return Ok(0);
            }
            self.left = size;
        }
        let data = read_exact(&mut self.r, buf.len().min(self.left))?;
        buf[..data.len()].copy_from_slice(&data);
        self.left -= data.len();
        if self.left == 0 {
            read_line(&mut self.r, 2)?; // CRLF sau mỗi khối
        }
        Ok(data.len())
    }
}

/// Trạng thái dùng khi kết nối lại: id cuối và thời gian chờ kết nối lại.
#[derive(Default)]
struct State {
    last_event_id: Option<String>,
    reconnect_ms: Option<u64>,
}

/// Mã giả 7.5: r là Reader đặt sau phần tiêu đề.
fn sse_read<S: Read>(r: &mut Reader<S>, on_event: impl Fn(&str, &str), state: &mut State) -> io::Result<()> {
    let (mut ev_type, mut ev_data) = ("message".to_string(), Vec::<String>::new());
    while let Some(line) = read_line(r, 65536)? {
        if line.is_empty() {
            // dòng trống: hết một sự kiện
            if !ev_data.is_empty() {
                on_event(&ev_type, &ev_data.join("\n"));
            }
            (ev_type, ev_data) = ("message".to_string(), Vec::new());
        } else if !line.starts_with(':') {
            // ":" mở đầu dòng chú thích
            let (field, value) = line.split_once(':').unwrap_or((&line, ""));
            let value = value.strip_prefix(' ').unwrap_or(value); // STRIP_ONE_SPACE
            match field {
                "event" => ev_type = value.to_string(),
                "data" => ev_data.push(value.to_string()),
                "id" => state.last_event_id = Some(value.to_string()),
                "retry" => state.reconnect_ms = value.parse().ok(),
                _ => {}
            }
        }
    }
    Ok(()) // None: server đóng luồng
}

fn session(host: &str, port: u16, path: &str) -> io::Result<()> {
    // SAFE_CONNECT (rút gọn)
    let addr = (host, port).to_socket_addrs()?.next().ok_or(io::Error::other("không phân giải được tên"))?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    s.set_read_timeout(Some(Duration::from_secs(30)))?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: text/event-stream\r\n\
         Cache-Control: no-cache\r\nConnection: close\r\n\r\n"
    );
    s.write_all(req.as_bytes())?;
    let mut r = Reader::new(s);
    let status = read_line(&mut r, 8192)?.unwrap_or_default();
    let hdr = read_headers(&mut r)?;
    let ct = hdr.get("content-type").map(String::as_str).unwrap_or("");
    println!("[client] {status}; Content-Type: {ct}");
    if status.split(' ').nth(1) != Some("200") {
        return Ok(());
    }
    let mut state = State::default();
    // in '\n' dạng hai ký tự để thấy nhiều dòng data
    let on_event = |t: &str, d: &str| println!("[client] Sự kiện '{t}': '{}'", d.replace('\n', "\\n"));
    let te = hdr.get("transfer-encoding").map(|v| v.to_lowercase()).unwrap_or_default();
    if te.ends_with("chunked") {
        println!("[client] Thân dùng chunked: đặt bộ đọc dòng sau lớp giải mã khối");
        let mut body = Reader::new(ChunkedSource { r, left: 0, done: false });
        sse_read(&mut body, on_event, &mut state)?;
    } else {
        sse_read(&mut r, on_event, &mut state)?;
    }
    let id = state.last_event_id.unwrap_or("(không có)".into());
    let retry = state.reconnect_ms.map(|v| v.to_string()).unwrap_or("(không có)".into());
    println!("[client] Server đóng luồng; id cuối = {id}, retry = {retry} ms");
    Ok(())
}

pub fn run(args: &[String]) {
    let host = args.first().map(String::as_str).unwrap_or("127.0.0.1");
    let port = port_arg(args, 1, 8080);
    let path = args.get(2).map(String::as_str).unwrap_or("/events");
    if let Err(e) = session(host, port, path) {
        println!("[client] Lỗi: {e}");
    }
}
