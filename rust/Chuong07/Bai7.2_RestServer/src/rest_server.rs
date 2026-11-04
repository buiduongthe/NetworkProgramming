//! Bài 7.2 – Server REST tối giản trên HTTP/1.1 (tokio).
//!
//! Cài đặt Mã giả 7.2 (Mục 7.2.3, Giáo trình Lập trình mạng): HTTP_SESSION, ROUTE, RESPOND.
//! Kế thừa: Mã giả 6.3 (server bất đồng bộ), 7.1 (READ_HEADERS), 3.7 (từ điển DICT nạp từ glossary.json).
//! Bổ sung theo bài tập cuối chương: POST /api/v1/terms (201 + Location) và DELETE /api/v1/terms/{term} (204).
//!
//! Chạy:  cargo run -- server [host] [port]      (mặc định 0.0.0.0 8080)
use crate::{exit_with, port_arg};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io;
use std::sync::{LazyLock, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};

const MAX_BODY: usize = 1048576; // 1 MiB, như MAX_FRAME của Mã giả 3.6

/// DICT nạp từ glossary.json; các tác vụ chạy trên nhiều luồng nên được bảo vệ bằng Mutex.
static DICT: LazyLock<Mutex<Map<String, Value>>> = LazyLock::new(|| {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/glossary.json"); // tệp nằm cạnh mã nguồn
    let text = std::fs::read_to_string(path).expect("Không đọc được glossary.json");
    Mutex::new(serde_json::from_str(&text).expect("glossary.json sai định dạng"))
});

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Content Too Large",
        _ => "Not Implemented", // 501
    }
}

/// PROBLEM(status, title) theo RFC 9457.
fn problem(status: u16, title: &str) -> Value {
    json!({"type": "about:blank", "title": title, "status": status})
}

/// URL_DECODE: %XX → byte, rồi giải mã UTF-8 (ví dụ %20 → dấu cách).
fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let hex = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok());
        match (b[i], hex) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// URL_ENCODE cho đường dẫn: giữ chữ cái, chữ số và "-._~/", còn lại thành %XX (như quote của Python).
fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|c| match c {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => (c as char).to_string(),
            _ => format!("%{c:02X}"),
        })
        .collect()
}

/// READ_LINE_ASYNC: BufReader của tokio đóng vai Reader (có bộ đệm pending riêng).
async fn read_line_async(r: &mut BufReader<tokio::net::tcp::OwnedReadHalf>, max_len: usize) -> io::Result<Option<String>> {
    let mut line = Vec::new();
    (&mut *r).take(max_len as u64 + 1).read_until(b'\n', &mut line).await?;
    if line.is_empty() {
        return Ok(None); // đối tác đã đóng
    }
    if line.len() > max_len {
        return Err(io::Error::other("Dòng quá dài"));
    }
    while line.last() == Some(&b'\n') || line.last() == Some(&b'\r') {
        line.pop(); // bỏ CRLF
    }
    Ok(Some(String::from_utf8_lossy(&line).into_owned()))
}

/// Mã giả 7.1 – READ_HEADERS, phiên bản bất đồng bộ (kết nối đóng giữa chừng là lỗi ConnectionClosed).
async fn read_headers_async(r: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> io::Result<HashMap<String, String>> {
    let mut hdr = HashMap::new();
    loop {
        let Some(line) = read_line_async(r, 8192).await? else {
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

/// Mã giả 7.2, dòng 38–45: Content-Length là số byte sau khi mã hóa UTF-8.
async fn respond(w: &mut OwnedWriteHalf, status: u16, obj: &Value, extra: &[(String, String)]) -> io::Result<()> {
    let mut h = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    for (k, v) in extra {
        h += &format!("{k}: {v}\r\n");
    }
    let mut out;
    if status == 204 {
        // 204: không có thân, không có Content-Length
        out = (h + "\r\n").into_bytes();
    } else {
        let b = serde_json::to_vec(obj).unwrap(); // JSON gọn, không khoảng trắng
        let ct = if status >= 400 { "application/problem+json" } else { "application/json" };
        h += &format!("Content-Type: {ct}; charset=utf-8\r\n");
        h += &format!("Content-Length: {}\r\n\r\n", b.len());
        out = h.into_bytes();
        out.extend_from_slice(&b);
    }
    w.write_all(&out).await // SEND_ALL_ASYNC
}

/// Mã giả 7.2, dòng 24–36 (thêm POST, DELETE). Trả về (mã, thân, tiêu đề bổ sung).
fn route(method: &str, path: &str, body: &[u8]) -> (u16, Value, Vec<(String, String)>) {
    let p: Vec<&str> = path.trim_matches('/').split('/').collect(); // ["api","v1","terms",…]
    if p.len() < 3 || p[0..3] != ["api", "v1", "terms"] || p.len() > 4 {
        return (404, problem(404, "Không có tài nguyên"), vec![]);
    }
    let mut dict = DICT.lock().unwrap();
    if method == "GET" && p.len() == 3 {
        let terms: Vec<&String> = dict.keys().collect();
        return (200, json!({"terms": terms}), vec![]);
    }
    if method == "GET" && p.len() == 4 {
        let t = url_decode(p[3]); // URL_DECODE: %20 -> dấu cách
        return match dict.get(&t) {
            Some(text) => (200, json!({"term": t, "text": text}), vec![]),
            None => (404, problem(404, "Không có thuật ngữ"), vec![]),
        };
    }
    if method == "POST" && p.len() == 3 {
        // mở rộng: thêm thuật ngữ
        let obj: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let (Some(t), Some(text)) = (obj["term"].as_str(), obj["text"].as_str()) else {
            return (400, problem(400, "JSON sai hoặc thiếu trường term, text"), vec![]);
        };
        if dict.contains_key(t) {
            return (409, problem(409, "Thuật ngữ đã tồn tại"), vec![]);
        }
        dict.insert(t.to_string(), json!(text));
        let location = ("Location".to_string(), format!("/api/v1/terms/{}", url_encode(t)));
        return (201, json!({"term": t, "text": text}), vec![location]);
    }
    if method == "DELETE" && p.len() == 4 {
        // mở rộng: xóa thuật ngữ
        let t = url_decode(p[3]);
        return match dict.shift_remove(&t) {
            Some(_) => (204, Value::Null, vec![]),
            None => (404, problem(404, "Không có thuật ngữ"), vec![]),
        };
    }
    (405, problem(405, "Phương thức không hỗ trợ"), vec![])
}

/// Mã giả 7.2, dòng 1–21: phiên keep-alive, nhiều yêu cầu trên một kết nối.
async fn http_session(conn: TcpStream) {
    let addr = conn.peer_addr().map(|a| a.to_string()).unwrap_or_default();
    let (rd, mut w) = conn.into_split();
    let mut r = BufReader::new(rd);
    if let Err(e) = session_loop(&mut r, &mut w, &addr).await {
        println!("[server] Kết thúc phiên {addr}: {e}");
    }
    drop(w); // CLOSE(conn)
    println!("[server] Đóng kết nối {addr}");
}

async fn session_loop(
    r: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    w: &mut OwnedWriteHalf,
    addr: &str,
) -> io::Result<()> {
    loop {
        let Some(line) = read_line_async(r, 8192).await? else {
            return Ok(()); // client đóng kết nối
        };
        let parts: Vec<&str> = line.split(' ').collect();
        if parts.len() != 3 {
            return respond(w, 400, &problem(400, "Dòng yêu cầu sai"), &[]).await;
        }
        let (method, target) = (parts[0], parts[1]);
        let hdr = read_headers_async(r).await?;
        let cl = hdr.get("content-length").map(String::as_str).unwrap_or("0");
        // TO_INT64: số nguyên 64 bit vì giá trị có thể > 4 GB
        let n: i64 = cl.parse().map_err(|_| io::Error::other(format!("Content-Length sai: {cl}")))?;
        if hdr.contains_key("transfer-encoding") {
            // chưa hỗ trợ chunked; độ dài thân không xác định: đóng
            println!("[server] {addr} {method} {target} -> 501");
            return respond(w, 501, &problem(501, "Chưa hỗ trợ"), &[]).await;
        }
        if n < 0 || n > MAX_BODY as i64 {
            // từ chối trước khi đọc: không cấp phát bộ đệm
            println!("[server] {addr} {method} {target} -> 413");
            return respond(w, 413, &problem(413, "Thân quá lớn"), &[]).await;
        }
        let mut body = vec![0u8; n as usize];
        r.read_exact(&mut body).await?; // READ_EXACT_ASYNC
        let path = target.split('?').next().unwrap_or(""); // PATH(target): bỏ query
        let (status, obj, extra) = route(method, path, &body);
        println!("[server] {addr} {method} {target} -> {status}");
        respond(w, status, &obj, &extra).await?;
        if hdr.get("connection").map(|v| v.to_lowercase()) == Some("close".into()) {
            return Ok(());
        }
    }
}

async fn serve(args: &[String]) {
    let host = args.first().map(String::as_str).unwrap_or("0.0.0.0");
    let port = port_arg(args, 1, 8080);
    let srv = TcpListener::bind((host, port))
        .await
        .unwrap_or_else(|e| exit_with(&format!("Không mở được cổng {port}: {e}")));
    println!("[server] Đang lắng nghe tại {host}:{port}");
    loop {
        let Ok((conn, _)) = srv.accept().await else { continue };
        tokio::spawn(http_session(conn)); // mỗi kết nối một tác vụ
    }
}

pub async fn run(args: &[String]) {
    tokio::select! {
        _ = serve(args) => {}
        _ = tokio::signal::ctrl_c() => println!("[server] Dừng server"),
    }
}
