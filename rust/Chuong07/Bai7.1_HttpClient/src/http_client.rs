//! Bài 7.1 – Client HTTP/1.1 tối giản trên socket TCP.
//!
//! Cài đặt Mã giả 7.1 (Mục 7.2.1, Giáo trình Lập trình mạng): HTTP_REQUEST, READ_HEADERS, READ_CHUNKED.
//! Kế thừa: Mã giả 2.2 (SAFE_CONNECT), 3.4 (RECV_EXACT), 3.5 (READ_LINE, cấu trúc Reader).
//! Client gửi GET tới từng đường dẫn và in mã trạng thái, cách định khung thân và nội dung thân.
//!
//! Chạy:  cargo run -- client [host] [port] [đường_dẫn ...]   (mặc định 127.0.0.1 8080 /)
use crate::port_arg;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Mã giả 2.2 (rút gọn): thử lần lượt các địa chỉ, có thời gian chờ kết nối.
fn safe_connect(host: &str, port: u16, timeout_ms: u64) -> io::Result<TcpStream> {
    let mut last_error = String::from("không phân giải được tên");
    for addr in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)) {
            Ok(s) => return Ok(s),
            Err(e) => last_error = e.to_string(),
        }
    }
    Err(io::Error::other(format!("Không kết nối được {host}:{port}: {last_error}")))
}

/// Mã giả 3.5 – STRUCT Reader { sock, pending }: giữ phần byte nhận dư giữa các lần đọc.
struct Reader {
    sock: TcpStream,
    pending: Vec<u8>,
}

/// Mã giả 3.5 – READ_LINE: trả về dòng (bỏ CRLF) hoặc None khi đối tác đã đóng.
fn read_line(r: &mut Reader, max_len: usize) -> io::Result<Option<String>> {
    loop {
        if let Some(i) = r.pending.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = r.pending.drain(..=i).collect();
            line.pop(); // bỏ '\n'
            if line.last() == Some(&b'\r') {
                line.pop(); // bỏ '\r'
            }
            return Ok(Some(String::from_utf8_lossy(&line).into_owned()));
        }
        if r.pending.len() > max_len {
            return Err(io::Error::other("Dòng quá dài"));
        }
        let mut chunk = [0u8; 4096];
        let n = r.sock.read(&mut chunk)?; // RECV
        if n == 0 {
            return Ok(None);
        }
        r.pending.extend_from_slice(&chunk[..n]);
    }
}

/// READ_EXACT: lấy phần dư r.pending trước, thiếu thì RECV_EXACT (Mã giả 3.4).
fn read_exact(r: &mut Reader, n: usize) -> io::Result<Vec<u8>> {
    let take = n.min(r.pending.len());
    let mut data: Vec<u8> = r.pending.drain(..take).collect();
    while data.len() < n {
        let mut chunk = vec![0u8; n - data.len()];
        let k = r.sock.read(&mut chunk)?;
        if k == 0 {
            return Err(io::Error::other(format!("Kết nối đóng khi mới nhận {}/{n} byte", data.len())));
        }
        data.extend_from_slice(&chunk[..k]);
    }
    Ok(data)
}

/// READ_UNTIL_CLOSE: đọc đến khi server đóng kết nối.
fn read_until_close(r: &mut Reader) -> io::Result<Vec<u8>> {
    let mut data = std::mem::take(&mut r.pending);
    r.sock.read_to_end(&mut data)?;
    Ok(data)
}

/// Mã giả 7.1, dòng 25–34: đọc các trường tiêu đề đến dòng trống.
fn read_headers(r: &mut Reader) -> io::Result<HashMap<String, String>> {
    let mut hdr = HashMap::new();
    loop {
        let Some(line) = read_line(r, 8192)? else {
            // IF line = NULL THEN RAISE ConnectionClosed(): server đóng giữa phần tiêu đề
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Kết nối đóng giữa phần tiêu đề"));
        };
        if line.is_empty() {
            break; // dòng trống: hết
        }
        let (k, v) = line.split_once(':').unwrap_or((&line, "")); // SPLIT_FIRST tại dấu ':' đầu tiên
        hdr.insert(k.trim().to_lowercase(), v.trim().to_string()); // tên trường không phân biệt hoa/thường
    }
    Ok(hdr)
}

/// Mã giả 7.1, dòng 33–43: giải mã thân Transfer-Encoding: chunked.
fn read_chunked(r: &mut Reader) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    loop {
        let line = read_line(r, 1024)?.ok_or(io::Error::other("Kết nối đóng giữa thân chunked"))?;
        let size = line.split(';').next().unwrap_or("").trim(); // bỏ phần mở rộng sau ';'
        let n = usize::from_str_radix(size, 16).map_err(|_| io::Error::other(format!("Kích thước khối sai: {size}")))?;
        if n == 0 {
            break;
        }
        data.extend(read_exact(r, n)?);
        read_line(r, 2)?; // CRLF sau mỗi khối
    }
    read_headers(r)?; // bỏ phần trailer
    Ok(data)
}

/// Mã giả 7.1, dòng 1–23: gửi một yêu cầu HTTP/1.1 và đọc phản hồi.
fn http_request(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> io::Result<(u16, HashMap<String, String>, Vec<u8>)> {
    let s = safe_connect(host, port, 3000)?;
    s.set_read_timeout(Some(Duration::from_millis(5000)))?;
    let mut r = Reader { sock: s, pending: Vec::new() };
    let mut h = format!("{method} {path} HTTP/1.1\r\n");
    h += &format!("Host: {host}:{port}\r\n"); // kèm cổng
    for (k, v) in headers {
        h += &format!("{k}: {v}\r\n");
    }
    if let Some(b) = body {
        h += &format!("Content-Length: {}\r\n", b.len());
    }
    h += "Connection: close\r\n\r\n"; // dòng trống kết thúc tiêu đề
    let mut out = h.into_bytes();
    out.extend_from_slice(body.unwrap_or(b""));
    r.sock.write_all(&out)?; // SEND_ALL
    let line = read_line(&mut r, 8192)?.ok_or(io::Error::other("Server đóng kết nối trước khi gửi phản hồi"))?;
    let status: u16 = line
        .split(' ')
        .nth(1)
        .and_then(|x| x.parse().ok())
        .ok_or(io::Error::other(format!("Dòng trạng thái sai: {line}")))?;
    let hdr = read_headers(&mut r)?;
    let te = hdr.get("transfer-encoding").map(|v| v.to_lowercase()).unwrap_or_default();
    let data = if te.ends_with("chunked") {
        // vd. "gzip, chunked"
        read_chunked(&mut r)?
    } else if let Some(cl) = hdr.get("content-length") {
        let n: usize = cl.parse().map_err(|_| io::Error::other(format!("Content-Length sai: {cl}")))?;
        read_exact(&mut r, n)?
    } else {
        read_until_close(&mut r)?
    };
    Ok((status, hdr, data)) // CLOSE(s) khi r ra khỏi phạm vi
}

pub fn run(args: &[String]) {
    let host = args.first().map(String::as_str).unwrap_or("127.0.0.1");
    let port = port_arg(args, 1, 8080);
    let paths: Vec<&str> = if args.len() > 2 { args[2..].iter().map(String::as_str).collect() } else { vec!["/"] };
    for path in paths {
        let (status, hdr, data) = match http_request(host, port, "GET", path, &[("Accept", "application/json")], None) {
            Ok(x) => x,
            Err(e) => {
                println!("[client] Lỗi: {e}");
                std::process::exit(1);
            }
        };
        println!("[client] GET {path} -> {status}");
        let te = hdr.get("transfer-encoding").map(|v| v.to_lowercase()).unwrap_or_default();
        let how = if te.ends_with("chunked") {
            "Transfer-Encoding = chunked".to_string()
        } else if let Some(cl) = hdr.get("content-length") {
            format!("Content-Length = {cl}")
        } else {
            "đọc đến khi server đóng kết nối".to_string()
        };
        println!("[client] Định khung thân: {how}");
        println!("[client] Thân ({} byte): {}", data.len(), String::from_utf8_lossy(&data));
    }
}
