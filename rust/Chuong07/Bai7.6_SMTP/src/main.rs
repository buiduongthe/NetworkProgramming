//! Bài 7.6 – Phiên gửi thư SMTP thủ công trên socket, có STARTTLS và AUTH PLAIN.
//!
//! Cài đặt Mã giả 7.7 (Mục 7.5.1, Giáo trình Lập trình mạng): SMTP_REPLY, CMD, SEND_MAIL, DOT_STUFF.
//! Kế thừa: Mã giả 2.2 (SAFE_CONNECT), 3.5 (READ_LINE). TLS_WRAP dùng crate rustls (Mục 8.4).
//! Tài khoản lấy từ biến môi trường SMTP_USER, SMTP_PASS (không ghi mật khẩu trong mã nguồn).
//!
//! Chạy:  cargo run -- [host] [port] [tệp_CA]      (mặc định localhost 587 ca.pem)
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Lỗi của phiên: mã phản hồi SMTP không như mong đợi, lỗi TLS hoặc lỗi mạng.
enum SmtpError {
    Reply(u16, Vec<String>),
    Protocol(String),
    Tls(String),
    Io(io::Error),
}

impl From<io::Error> for SmtpError {
    fn from(e: io::Error) -> Self {
        SmtpError::Io(e)
    }
}

impl fmt::Display for SmtpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SmtpError::Reply(code, text) => write!(f, "{code} {}", text.join(" ")),
            SmtpError::Protocol(m) | SmtpError::Tls(m) => write!(f, "{m}"),
            SmtpError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Mã giả 3.5 – Reader { sock, pending } và READ_LINE; sock là socket thường hoặc luồng TLS.
struct Reader<S: Read + Write> {
    sock: S,
    pending: Vec<u8>,
}

impl<S: Read + Write> Reader<S> {
    fn new(sock: S) -> Self {
        Reader { sock, pending: Vec::new() }
    }

    fn read_line(&mut self, max_len: usize) -> io::Result<String> {
        loop {
            if let Some(i) = self.pending.iter().position(|&b| b == b'\n') {
                let mut line: Vec<u8> = self.pending.drain(..=i).collect();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(String::from_utf8_lossy(&line).into_owned());
            }
            if self.pending.len() > max_len {
                return Err(io::Error::other("Dòng quá dài"));
            }
            let mut chunk = [0u8; 4096];
            let n = self.sock.read(&mut chunk)?;
            if n == 0 {
                return Err(io::Error::other("Server đóng kết nối"));
            }
            self.pending.extend_from_slice(&chunk[..n]);
        }
    }
}

/// Mã giả 7.7, dòng 1–8: đọc phản hồi nhiều dòng ("250-..." ... "250 ...").
fn smtp_reply<S: Read + Write>(r: &mut Reader<S>) -> Result<(u16, Vec<String>), SmtpError> {
    let mut text = Vec::new();
    loop {
        let l = r.read_line(1024)?; // NULL (server đóng kết nối) → lỗi trong read_line
        println!("[client] S: {l}");
        let code: Option<u16> = l.get(0..3).and_then(|c| c.parse().ok());
        let Some(code) = code else {
            // LENGTH(l) < 3 (hoặc không phải ba chữ số): RAISE ProtocolError()
            return Err(SmtpError::Protocol(format!("Phản hồi sai: '{l}'")));
        };
        text.push(l.get(4..).unwrap_or("").to_string()); // rỗng nếu l chỉ có 3 chữ số
        if l.len() == 3 || l.as_bytes()[3] == b' ' {
            return Ok((code, text)); // dòng cuối
        }
    }
}

/// Mã giả 7.7, dòng 10–14: gửi lệnh, kiểm tra chữ số đầu của mã phản hồi.
fn cmd<S: Read + Write>(r: &mut Reader<S>, line: &str, expect: u16, shown: Option<&str>) -> Result<(), SmtpError> {
    println!("[client] C: {}", shown.unwrap_or(line));
    r.sock.write_all(format!("{line}\r\n").as_bytes())?;
    let (code, text) = smtp_reply(r)?;
    if code / 100 != expect {
        return Err(SmtpError::Reply(code, text));
    }
    Ok(())
}

/// Thêm '.' vào đầu dòng bắt đầu bằng '.', chuẩn hóa xuống dòng về CRLF (Mục 3.4.2).
fn dot_stuff(msg: &str) -> String {
    let lines: Vec<String> = msg.split('\n').map(|l| if l.starts_with('.') { format!(".{l}") } else { l.to_string() }).collect();
    lines.join("\r\n")
}

/// Cấu hình TLS phía client: chỉ tin cậy các chứng chỉ CA trong tệp cafile (định dạng PEM).
fn tls_config(cafile: &str) -> Result<Arc<ClientConfig>, SmtpError> {
    let mut roots = RootCertStore::empty();
    let certs = CertificateDer::pem_file_iter(cafile).map_err(|e| SmtpError::Tls(format!("Không đọc được {cafile}: {e}")))?;
    for cert in certs {
        let cert = cert.map_err(|e| SmtpError::Tls(format!("{cafile} sai định dạng: {e}")))?;
        roots.add(cert).map_err(|e| SmtpError::Tls(e.to_string()))?;
    }
    Ok(Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth()))
}

/// SAFE_CONNECT (rút gọn): thử lần lượt các địa chỉ, chờ kết nối 5 giây.
fn safe_connect(host: &str, port: u16) -> io::Result<TcpStream> {
    let mut last = io::Error::other("không phân giải được tên");
    for addr in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
            Ok(s) => {
                s.set_read_timeout(Some(Duration::from_secs(5)))?;
                return Ok(s);
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Mã giả 7.7, dòng 16–30.
#[allow(clippy::too_many_arguments)]
fn send_mail(host: &str, port: u16, cafile: &str, user: &str, secret: &str, sender: &str, to: &str, msg: &str) -> Result<(), SmtpError> {
    let s = safe_connect(host, port)?; // SAFE_CONNECT(host, 587, 5000)
    let mut r = Reader::new(s);
    let (code, _) = smtp_reply(&mut r)?; // 220: lời chào của server
    if code != 220 {
        return Err(SmtpError::Reply(code, vec!["Không phải lời chào 220".into()]));
    }
    cmd(&mut r, "EHLO client.example", 2, None)?;
    cmd(&mut r, "STARTTLS", 2, None)?; // 220: sẵn sàng TLS
    // TLS_WRAP(s, CLIENT, host): bỏ Reader cũ (cùng mọi byte nhận trước khi bắt tay), dùng Reader mới
    let mut sock = r.sock;
    let name = ServerName::try_from(host.to_string()).map_err(|e| SmtpError::Tls(e.to_string()))?;
    let mut conn = ClientConnection::new(tls_config(cafile)?, name).map_err(|e| SmtpError::Tls(e.to_string()))?;
    while conn.is_handshaking() {
        conn.complete_io(&mut sock).map_err(|e| SmtpError::Tls(e.to_string()))?; // bắt tay TLS
    }
    let version = match conn.protocol_version() {
        Some(rustls::ProtocolVersion::TLSv1_3) => "TLSv1.3",
        Some(rustls::ProtocolVersion::TLSv1_2) => "TLSv1.2",
        _ => "TLS",
    };
    println!("[client] Đã thiết lập {version}, chứng chỉ server hợp lệ cho '{host}'");
    let mut r = Reader::new(StreamOwned::new(conn, sock));
    cmd(&mut r, "EHLO client.example", 2, None)?; // chào lại sau khi có TLS
    let token = B64.encode(format!("\0{user}\0{secret}"));
    cmd(&mut r, &format!("AUTH PLAIN {token}"), 2, Some("AUTH PLAIN ****"))?; // không in thông tin xác thực
    cmd(&mut r, &format!("MAIL FROM:<{sender}>"), 2, None)?;
    cmd(&mut r, &format!("RCPT TO:<{to}>"), 2, None)?;
    cmd(&mut r, "DATA", 3, None)?; // 354: bắt đầu nội dung
    let body = dot_stuff(msg);
    let shown = format!("<nội dung thư {} byte> + dòng '.'", body.len());
    cmd(&mut r, &format!("{body}\r\n."), 2, Some(&shown))?;
    cmd(&mut r, "QUIT", 2, None)?;
    Ok(()) // CLOSE: socket đóng khi r ra khỏi phạm vi
}

/// Ngày giờ hiện tại theo RFC 5322, giờ UTC (ví dụ "Wed, 04 Nov 2026 14:32:33 +0000").
fn rfc5322_date() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Đổi số ngày kể từ 1970-01-01 sang năm-tháng-ngày (thuật toán "civil from days" của H. Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    let wd = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][days.rem_euclid(7) as usize]; // 1970-01-01 là thứ Năm
    let mon = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(month - 1) as usize];
    format!("{wd}, {day:02} {mon} {year} {:02}:{:02}:{:02} +0000", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Thư Internet tối giản: tiêu đề (Subject mã hóa MIME UTF-8), dòng trống, thân văn bản UTF-8.
fn build_message(sender: &str, to: &str) -> String {
    let subject = format!("=?utf-8?B?{}?=", B64.encode("Thư thử nghiệm Bài 7.6"));
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    [
        format!("From: <{sender}>"),
        format!("To: <{to}>"),
        format!("Subject: {subject}"),
        format!("Date: {}", rfc5322_date()),
        format!("Message-ID: <{now}@client.example>"),
        "MIME-Version: 1.0".into(),
        "Content-Type: text/plain; charset=utf-8".into(),
        "Content-Transfer-Encoding: 8bit".into(),
        "".into(),
        "Xin chào, đây là thư gửi bằng phiên SMTP tự cài đặt.".into(),
        ".Dòng này bắt đầu bằng dấu chấm (được DOT_STUFF thêm một dấu chấm).".into(),
        "Hết thư.".into(),
    ]
    .join("\n")
}

fn exit_with(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let host = args.first().map(String::as_str).unwrap_or("localhost");
    let port: i64 = match args.get(1) {
        Some(s) => s.parse().unwrap_or_else(|_| exit_with(&format!("Cổng phải là số nguyên: {s}"))),
        None => 587,
    };
    if !(1..=65535).contains(&port) {
        exit_with(&format!("Cổng không hợp lệ: {port}"));
    }
    let cafile = args.get(2).map(String::as_str).unwrap_or("ca.pem");
    let user = std::env::var("SMTP_USER").unwrap_or_default();
    let secret = std::env::var("SMTP_PASS").unwrap_or_default();
    let (sender, to) = ("sinhvien@example.com", "giangvien@example.com");
    match send_mail(host, port as u16, cafile, &user, &secret, sender, to, &build_message(sender, to)) {
        Ok(()) => println!("[client] Gửi thư thành công"),
        Err(SmtpError::Tls(e)) => {
            // chứng chỉ không tin cậy hoặc sai tên máy
            println!("[client] Lỗi TLS: {e}");
            std::process::exit(1);
        }
        Err(e) => {
            println!("[client] Lỗi SMTP: {e}");
            std::process::exit(1);
        }
    }
}
