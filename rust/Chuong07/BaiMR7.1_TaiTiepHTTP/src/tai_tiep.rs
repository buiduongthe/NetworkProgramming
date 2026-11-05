//! Client tải tệp HTTP có tải tiếp (resume) khi kết nối bị đứt.
//!
//! Quy trình:
//!   1. HEAD lấy kích thước, ETag và mã băm SHA-256 của tệp.
//!   2. GET với "Range: bytes=<đã_có>-" và "If-Range: <ETag>"; ghi nối vào tệp cục bộ.
//!      206 -> nối tiếp; 200 -> server gửi lại từ đầu (tệp đã đổi hoặc không hỗ trợ Range).
//!   3. Nếu nhận ít hơn Content-Length (kết nối đứt) -> chờ lùi thời gian rồi thử lại từ chỗ dừng.
//!   4. Kiểm tra kích thước và SHA-256.
//! Lưu ý: TCP báo EOF (read trả 0) khi server đóng sớm – không phải lỗi; phải tự so số byte với
//! Content-Length (cùng bài học "TCP EOF chưa chắc là đủ dữ liệu" ở Chương 3).

use crate::http::{read_head, Head};
use crate::{sha256_hex, split_args};
use std::fs::{self, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process;
use std::thread;
use std::time::Duration;

struct Url {
    host: String,
    port: u16,
    path: String,
}

/// Tách "http://host:port/đường_dẫn" (chỉ hỗ trợ http).
fn parse_url(u: &str) -> Option<Url> {
    let rest = u.strip_prefix("http://")?;
    let (hp, path) = rest.split_once('/').map_or((rest, "/".to_string()), |(h, p)| (h, format!("/{p}")));
    let (host, port) = match hp.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().ok()?),
        None => (hp.to_string(), 80),
    };
    Some(Url { host, port, path })
}

/// Gửi một yêu cầu, trả về (phần đầu phản hồi, bộ đọc phần thân).
fn request(u: &Url, method: &str, extra: &[(&str, &str)]) -> io::Result<(Head, BufReader<TcpStream>)> {
    let addr = (u.host.as_str(), u.port).to_socket_addrs()?.next().ok_or_else(|| io::Error::other("không phân giải được host"))?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut req = format!("{method} {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n", u.path, u.host, u.port);
    for (k, v) in extra {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes())?;
    let mut r = BufReader::new(s);
    let head = read_head(&mut r)?;
    Ok((head, r))
}

fn status_of(h: &Head) -> (u16, String) {
    let mut p = h.first.splitn(3, ' ');
    p.next();
    let code = p.next().and_then(|c| c.parse().ok()).unwrap_or(0);
    (code, p.next().unwrap_or("").to_string())
}

fn head(u: &Url) -> io::Result<(u64, String, String)> {
    let (h, _) = request(u, "HEAD", &[])?;
    let (code, _) = status_of(&h);
    if code != 200 {
        return Err(io::Error::other(format!("HEAD trả {code}")));
    }
    let len = h.get("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    Ok((len, h.get("ETag").unwrap_or("").to_string(), h.get("X-Checksum-SHA256").unwrap_or("").to_string()))
}

/// Một lần GET; trả về số byte có trong tệp cục bộ sau lần này.
fn get_range(u: &Url, path: &str, have: u64, etag: &str) -> io::Result<u64> {
    let range = format!("bytes={have}-");
    let extra: Vec<(&str, &str)> = if have > 0 { vec![("Range", &range), ("If-Range", etag)] } else { vec![] };
    let (h, mut body) = request(u, "GET", &extra)?;
    let (code, reason) = status_of(&h);
    let cr = if code == 206 { format!(", {}", h.get("Content-Range").unwrap_or("")) } else { String::new() };
    println!("[tai] -> {code} {reason}{cr}");
    let (mut f, have) = match code {
        416 => return Ok(have), // dải không thỏa mãn được: giữ nguyên phần đã có
        200 => (fs::File::create(path)?, 0), // server gửi toàn bộ: ghi đè từ đầu
        206 => (OpenOptions::new().append(true).create(true).open(path)?, have),
        _ => return Err(io::Error::other(format!("Mã trạng thái không mong đợi {code}"))),
    };
    let expected: u64 = h.get("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut got = 0u64;
    let mut buf = [0u8; 16384];
    while got < expected {
        let want = buf.len().min((expected - got) as usize);
        let n = body.read(&mut buf[..want])?;
        if n == 0 {
            break; // kết nối đóng sớm: EOF, không phải lỗi
        }
        f.write_all(&buf[..n])?;
        got += n as u64;
    }
    if got < expected {
        println!("[tai] Kết nối đứt: nhận {got}/{expected} byte của lần này");
    }
    Ok(have + got)
}

fn kich_thuoc(path: &str) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

pub fn run(args: &[String]) {
    let (pos, opt) = split_args(args);
    if pos.len() < 2 {
        eprintln!("Cách dùng: tai <url> <tệp_lưu> [--so-lan-thu N]");
        process::exit(2);
    }
    let so_lan_thu: u32 = opt.get("so-lan-thu").and_then(|v| v.parse().ok()).unwrap_or(5);
    let Some(u) = parse_url(&pos[0]) else {
        eprintln!("URL không hợp lệ (cần http://host:port/đường_dẫn): {}", pos[0]);
        process::exit(2)
    };
    let out = &pos[1];
    let (size, etag, sha) = head(&u).unwrap_or_else(|e| {
        eprintln!("[tai] Lỗi HEAD: {e}");
        process::exit(1)
    });
    println!("[tai] HEAD: {size} byte, ETag {etag}");
    let mut have = if Path::new(out).exists() { kich_thuoc(out) } else { 0 };
    for attempt in 1..=so_lan_thu {
        if have >= size {
            break;
        }
        let xin = if have > 0 { format!("Range: bytes={have}-") } else { "toàn bộ".to_string() };
        println!("[tai] Lần {attempt}: đã có {have} byte, xin {xin}");
        match get_range(&u, out, have, &etag) {
            Ok(h) => have = h,
            Err(e) => {
                println!("[tai] Lỗi: {e}");
                have = kich_thuoc(out);
            }
        }
        if have < size {
            let delay = 0.2 * 2f64.powi(attempt as i32 - 1); // lùi thời gian theo cấp số nhân (Bài 5.6)
            println!("[tai] Chờ {delay:.1} s rồi tải tiếp");
            thread::sleep(Duration::from_secs_f64(delay));
        }
    }
    if have < size {
        println!("[tai] THẤT BẠI sau {so_lan_thu} lần thử: có {have}/{size} byte");
        process::exit(1);
    }
    let digest = sha256_hex(&fs::read(out).unwrap_or_default());
    println!("[tai] Đủ {have}/{size} byte; SHA-256 {} ({}...)", if digest == sha { "KHỚP" } else { "SAI" }, &digest[..16]);
}
