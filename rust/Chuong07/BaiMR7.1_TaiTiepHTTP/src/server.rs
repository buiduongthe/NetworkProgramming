//! Server HTTP phục vụ tệp có hỗ trợ tải từng phần (Range, RFC 9110).
//!
//! Phục vụ một tệp nhị phân sinh sẵn tại /tep.bin:
//!   - HEAD: trả Content-Length, Accept-Ranges: bytes, ETag, X-Checksum-SHA256.
//!   - GET có "Range: bytes=a-" hoặc "bytes=a-b": trả 206 Partial Content + Content-Range;
//!     dải không thỏa mãn được: 416 + "Content-Range: bytes */<kích thước>".
//!   - If-Range: <ETag>: nếu ETag không khớp (tệp đã đổi) thì bỏ qua Range, trả 200 toàn bộ.
//!   - Giả lập sự cố mạng: --so-lan-cat N yêu cầu GET đầu tiên bị ngắt kết nối sau --cat-sau byte.

use crate::http::read_head;
use crate::{sha256_hex, split_args};
use std::io::{BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

/// Nội dung tệp cố định (giống bản Python, Go) để các bản cho cùng SHA-256.
fn make_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| ((i * 31 + i / 7) % 256) as u8).collect()
}

fn log(text: &str) {
    println!("[server] {text}");
}

struct Tep {
    data: Vec<u8>,
    etag: String,
    sha: String,
    cat_sau: usize,
    cuts_left: AtomicI64, // số yêu cầu GET còn phải "cắt"
    counter: AtomicUsize,
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        206 => "Partial Content",
        404 => "Not Found",
        416 => "Range Not Satisfiable",
        _ => "Not Implemented",
    }
}

fn common_headers(w: &mut TcpStream, t: &Tep, status: u16, length: usize, extra: &[(&str, String)]) -> std::io::Result<()> {
    let mut h = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/octet-stream\r\nContent-Length: {length}\r\nAccept-Ranges: bytes\r\nETag: {}\r\nConnection: close\r\n",
        reason(status),
        t.etag
    );
    for (k, v) in extra {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str("\r\n");
    w.write_all(h.as_bytes())
}

/// Phân tích "bytes=a-" hoặc "bytes=a-b" (một dải); None nếu sai cú pháp.
fn parse_range(v: &str) -> Option<(usize, Option<usize>)> {
    let (a, b) = v.trim().strip_prefix("bytes=")?.split_once('-')?;
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if !digits(a) || !(b.is_empty() || digits(b)) {
        return None;
    }
    Some((a.parse().ok()?, if b.is_empty() { None } else { b.parse().ok() }))
}

fn handle(mut conn: TcpStream, t: Arc<Tep>) -> std::io::Result<()> {
    let head = read_head(&mut BufReader::new(conn.try_clone()?))?;
    let mut parts = head.first.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let k = t.counter.fetch_add(1, Ordering::SeqCst);
    if method != "GET" && method != "HEAD" {
        return common_headers(&mut conn, &t, 501, 0, &[]);
    }
    if path != "/tep.bin" {
        return common_headers(&mut conn, &t, 404, 0, &[]);
    }
    let size = t.data.len();
    if method == "HEAD" {
        common_headers(&mut conn, &t, 200, size, &[("X-Checksum-SHA256", t.sha.clone())])?;
        log(&format!("#{k} HEAD -> 200, {size} byte"));
        return Ok(());
    }
    let rng = head.get("Range").map(String::from);
    let if_range = head.get("If-Range");
    let (mut start, mut end, mut status) = (0, size.saturating_sub(1), 200);
    if let Some(r) = rng.as_deref().filter(|_| if_range.is_none_or(|e| e == t.etag)) {
        // If-Range khớp (hoặc không có) mới dùng Range
        if let Some((a, b)) = parse_range(r) {
            start = a;
            end = b.map_or(size.saturating_sub(1), |b| b.min(size.saturating_sub(1)));
            if start >= size || start > end {
                common_headers(&mut conn, &t, 416, 0, &[("Content-Range", format!("bytes */{size}"))])?;
                log(&format!("#{k} GET Range={r} -> 416"));
                return Ok(());
            }
            status = 206;
        }
    }
    let extra = if status == 206 { vec![("Content-Range", format!("bytes {start}-{end}/{size}"))] } else { vec![] };
    let body = if size == 0 { &t.data[..] } else { &t.data[start..=end] };
    common_headers(&mut conn, &t, status, body.len(), &extra)?;
    let rng_txt = rng.as_deref().unwrap_or("-");
    if t.cuts_left.fetch_sub(1, Ordering::SeqCst) > 0 {
        // giả lập đứt kết nối giữa chừng
        let n = t.cat_sau.min(body.len());
        conn.write_all(&body[..n])?;
        conn.shutdown(Shutdown::Both)?;
        log(&format!("#{k} GET Range={rng_txt} -> {status}, NGẮT sau {n} byte"));
        return Ok(());
    }
    conn.write_all(body)?;
    log(&format!("#{k} GET Range={rng_txt} -> {status}, gửi {} byte", body.len()));
    Ok(())
}

pub fn run(args: &[String]) {
    let (pos, opt) = split_args(args);
    let host = pos.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let port: u16 = pos.get(1).and_then(|p| p.parse().ok()).unwrap_or(8000);
    let num = |k: &str, d: i64| opt.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(d);
    let data = make_data(num("kich-thuoc", 300000).max(0) as usize);
    let sha = sha256_hex(&data);
    let etag = format!("\"{}\"", &sha[..16]);
    let t = Arc::new(Tep {
        etag,
        sha,
        cat_sau: num("cat-sau", 100000).max(0) as usize,
        cuts_left: AtomicI64::new(num("so-lan-cat", 0)),
        counter: AtomicUsize::new(1),
        data,
    });
    let srv = TcpListener::bind((host.as_str(), port)).unwrap_or_else(|e| {
        eprintln!("[server] Không mở được {host}:{port}: {e}");
        std::process::exit(1)
    });
    ctrlc::set_handler(|| {
        log("Dừng server");
        std::process::exit(0);
    })
    .expect("Không cài được hàm bắt Ctrl+C");
    log(&format!("http://{host}:{port}/tep.bin, {} byte, ETag {}", t.data.len(), t.etag));
    for conn in srv.incoming().flatten() {
        let t = Arc::clone(&t);
        thread::spawn(move || {
            let _ = handle(conn, t);
        });
    }
}
