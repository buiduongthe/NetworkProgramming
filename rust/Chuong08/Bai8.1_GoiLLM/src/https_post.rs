//! Bài 8.1 – HTTPS_POST tối giản trên socket TCP (kế thừa Mã giả 7.1: dòng yêu cầu, READ_HEADERS,
//! READ_CHUNKED dạng luồng như Bài 7.4) và tín hiệu hủy CancelToken.
//!
//! Thư viện chuẩn Rust không có client HTTP; bài tự viết phần cần thiết: POST một thân JSON,
//! đọc dòng trạng thái và tiêu đề, trả thân phản hồi dưới dạng luồng đọc được (đã giải mã chunked).
//! URL https:// được bọc TLS bằng rustls, tin cậy kho chứng chỉ của hệ điều hành.
use crate::llm_stream::CallError;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::collections::HashMap;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Tín hiệu hủy ct. cancel() đặt cờ và đóng socket đang dùng, nên lời gọi đọc đang chờ trả về ngay.
#[derive(Default)]
pub struct CancelToken {
    cancelled: AtomicBool,
    sock: Mutex<Option<TcpStream>>,
}

impl CancelToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(s) = self.sock.lock().unwrap().take() {
            let _ = s.shutdown(Shutdown::Both);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Ghi nhận socket của lần gọi hiện tại để cancel() đóng được.
    fn register(&self, s: &TcpStream) {
        *self.sock.lock().unwrap() = s.try_clone().ok();
    }

    /// Báo Cancelled nếu đã hủy.
    pub fn check(&self) -> Result<(), CallError> {
        if self.is_cancelled() {
            Err(CallError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Socket thường hoặc luồng TLS: đều đọc, ghi được.
trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send> Stream for T {}

/// Mã giả 3.5 – Reader { sock, pending }.
struct Reader {
    sock: Box<dyn Stream>,
    pending: Vec<u8>,
}

impl Reader {
    /// READ_LINE: dòng (bỏ CRLF) hoặc None khi đối tác đã đóng.
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

    /// Đọc tối đa buf.len() byte: lấy phần dư pending trước.
    fn read_some(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.pending.is_empty() {
            let n = buf.len().min(self.pending.len());
            buf[..n].copy_from_slice(&self.pending[..n]);
            self.pending.drain(..n);
            return Ok(n);
        }
        self.sock.read(buf)
    }

    /// Mã giả 7.1 – READ_HEADERS: đọc đến dòng trống; kết nối đóng giữa chừng là lỗi.
    fn read_headers(&mut self) -> io::Result<HashMap<String, String>> {
        let mut hdr = HashMap::new();
        loop {
            let Some(line) = self.read_line(8192)? else {
                return Err(io::Error::new(ErrorKind::UnexpectedEof, "Kết nối đóng giữa phần tiêu đề"));
            };
            if line.is_empty() {
                return Ok(hdr);
            }
            let (k, v) = line.split_once(':').unwrap_or((&line, ""));
            hdr.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
}

/// Cách định khung thân phản hồi (Mục 7.2.1).
enum Framing {
    Chunked { left: usize, done: bool },
    Length(u64),
    UntilClose,
}

/// Thân phản hồi dạng luồng: read() trả dữ liệu thân thuần (đã bỏ các dòng độ dài khối).
pub struct Body {
    r: Reader,
    framing: Framing,
}

impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.framing {
            Framing::UntilClose => self.r.read_some(buf),
            Framing::Length(left) => {
                if *left == 0 {
                    return Ok(0);
                }
                let max = buf.len().min(*left as usize);
                let n = self.r.read_some(&mut buf[..max])?;
                *left -= n as u64;
                Ok(n)
            }
            Framing::Chunked { left, done } => {
                if *done {
                    return Ok(0);
                }
                if *left == 0 {
                    let line = self.r.read_line(1024)?.ok_or(io::Error::other("Kết nối đóng giữa thân chunked"))?;
                    let size = line.split(';').next().unwrap_or("").trim();
                    let size = usize::from_str_radix(size, 16).map_err(|_| io::Error::other("Kích thước khối sai"))?;
                    if size == 0 {
                        // khối 0: hết thân, bỏ trailer
                        self.r.read_headers()?;
                        *done = true;
                        return Ok(0);
                    }
                    *left = size;
                }
                let max = buf.len().min(*left);
                let n = self.r.read_some(&mut buf[..max])?;
                if n == 0 {
                    return Err(io::Error::other("Kết nối đóng giữa khối"));
                }
                *left -= n;
                if *left == 0 {
                    self.r.read_line(2)?; // CRLF sau mỗi khối
                }
                Ok(n)
            }
        }
    }
}

/// Phản hồi: mã trạng thái, tiêu đề (tên chữ thường), thân dạng luồng.
pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Body,
}

/// Bọc TLS cho socket (TLS_WRAP của Mục 8.4): kiểm tra chứng chỉ theo kho của hệ điều hành và tên máy.
fn tls_wrap(sock: TcpStream, host: &str) -> Result<Box<dyn Stream>, CallError> {
    let mut roots = RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    let config = ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    let name = ServerName::try_from(host.to_string()).map_err(|e| CallError::net(format!("Tên máy sai: {e}")))?;
    let conn = ClientConnection::new(Arc::new(config), name).map_err(|e| CallError::net(format!("TLS: {e}")))?;
    Ok(Box::new(StreamOwned::new(conn, sock)))
}

/// Ánh xạ lỗi kết nối như Mã giả 2.2: bị từ chối → REFUSED (không thử lại); hết giờ → Timeout;
/// không tới được → UNREACHABLE (thử lại).
fn connect_error(e: io::Error) -> CallError {
    match e.kind() {
        ErrorKind::ConnectionRefused => CallError::net("REFUSED"),
        ErrorKind::TimedOut | ErrorKind::WouldBlock => CallError::Timeout("Hết thời gian chờ kết nối".into()),
        _ => CallError::Net { msg: format!("UNREACHABLE ({e})"), retry: true, retry_after: None },
    }
}

/// HTTPS_POST(url, body, hdrs, timeout_ms, ct): gửi POST, trả về (status, headers, thân).
/// timeout_ms là thời gian chờ mỗi lần đọc (thời gian chờ nhàn rỗi giữa hai sự kiện của luồng);
/// thời gian chờ kết nối riêng là 3 000 ms.
pub fn https_post(url: &str, body: &[u8], hdrs: &[(&str, String)], timeout_ms: u64, ct: &CancelToken) -> Result<Response, CallError> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(CallError::net(format!("URL không hợp lệ: {url}")));
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().map_err(|_| CallError::net(format!("Cổng không hợp lệ: {p}")))?),
        None => (authority, if tls { 443 } else { 80 }),
    };
    ct.check()?;
    let addrs = (host, port).to_socket_addrs().map_err(|e| CallError::Net { msg: format!("UNREACHABLE ({e})"), retry: true, retry_after: None })?;
    let mut last = CallError::net("Không phân giải được tên");
    let mut tcp = None;
    for a in addrs {
        match TcpStream::connect_timeout(&a, Duration::from_millis(3000)) {
            Ok(s) => {
                tcp = Some(s);
                break;
            }
            Err(e) => last = connect_error(e),
        }
    }
    let tcp = tcp.ok_or(last)?;
    tcp.set_read_timeout(Some(Duration::from_millis(timeout_ms))).map_err(connect_error)?;
    ct.register(&tcp); // từ đây hủy là đóng socket này
    let sock: Box<dyn Stream> = if tls { tls_wrap(tcp, host)? } else { Box::new(tcp) };
    let mut r = Reader { sock, pending: Vec::new() };
    let mut h = format!("POST {path} HTTP/1.1\r\nHost: {authority}\r\n");
    for (k, v) in hdrs {
        h += &format!("{k}: {v}\r\n");
    }
    h += "Content-Type: application/json\r\nAccept: text/event-stream\r\n";
    h += &format!("Content-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    let read_err = |e: io::Error| {
        if ct.is_cancelled() {
            CallError::Cancelled
        } else if e.kind() == ErrorKind::InvalidData {
            CallError::net(format!("Lỗi TLS: {e}")) // chứng chỉ không tin cậy, sai tên máy: không thử lại
        } else if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut {
            CallError::Timeout("Hết thời gian chờ phản hồi".into())
        } else {
            CallError::Net { msg: format!("Lỗi mạng: {e}"), retry: true, retry_after: None }
        }
    };
    r.sock.write_all(&[h.as_bytes(), body].concat()).map_err(read_err)?; // SEND_ALL (TLS bắt tay ở lần ghi đầu)
    let line = r.read_line(8192).map_err(read_err)?.ok_or(CallError::Net {
        msg: "Server đóng kết nối trước khi gửi phản hồi".into(),
        retry: true,
        retry_after: None,
    })?;
    let status: u16 = line.split(' ').nth(1).and_then(|x| x.parse().ok()).ok_or(CallError::net(format!("Dòng trạng thái sai: {line}")))?;
    let headers = r.read_headers().map_err(read_err)?;
    let te = headers.get("transfer-encoding").map(|v| v.to_lowercase()).unwrap_or_default();
    let framing = if te.ends_with("chunked") {
        Framing::Chunked { left: 0, done: false }
    } else if let Some(n) = headers.get("content-length").and_then(|v| v.parse().ok()) {
        Framing::Length(n)
    } else {
        Framing::UntilClose
    };
    Ok(Response { status, headers, body: Body { r, framing } })
}
