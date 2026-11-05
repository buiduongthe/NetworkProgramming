//! Bài mở rộng MR8.4 – Thân công cụ fetch_url: bản NGÂY THƠ và bản AN TOÀN (chống SSRF).
//!
//! Dùng bởi main.rs. SSRF (Server-Side Request Forgery) là lỗ hổng khi server nhận một URL từ bên
//! ngoài rồi tự đi tải URL đó: kẻ tấn công ép server truy cập dịch vụ nội bộ (máy chủ siêu dữ liệu
//! đám mây 169.254.169.254, dịch vụ chỉ nghe loopback, mạng riêng 10/8…) mà client ngoài không với tới.
//!
//! Hai bảng được "tiêm" (inject) qua biến môi trường để demo chạy hoàn toàn offline, tất định:
//!   SSRF_RESOLVER = {"tên miền": ["ip1", "ip2", ...]} – bảng phân giải tên giả lập; nhiều IP thì mỗi
//!                   lần gọi trả IP kế tiếp (mô phỏng DNS rebinding / TOCTOU).
//!   SSRF_DIAL     = {"ip": cổng}                       – nối tới 127.0.0.1:cổng thay cho ip thật.
//! Hai bảng này CHỈ cho bản demo offline; thực tế dùng DNS thật và nối thẳng tới IP đã kiểm tra.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

const MAX_REDIRECT: usize = 5;
const MAX_BYTES: usize = 64 * 1024; // giới hạn kích thước phản hồi
const HAN_GIAY: Duration = Duration::from_secs(5); // giới hạn tổng thời gian một lần fetch

/// Dải địa chỉ BỊ CHẶN (liệt kê tường minh, KHÔNG dựa vào "is private" vì dải tài liệu cũng private).
const BLOCKED: [&str; 10] = [
    "0.0.0.0/8",     // địa chỉ "không xác định" / mạng này
    "127.0.0.0/8",   // loopback
    "10.0.0.0/8",    // RFC 1918
    "172.16.0.0/12", // RFC 1918
    "192.168.0.0/16",// RFC 1918
    "169.254.0.0/16",// link-local, gồm 169.254.169.254 (siêu dữ liệu đám mây)
    "::1/128",       // loopback IPv6
    "::/128",        // không xác định IPv6
    "fc00::/7",      // IPv6 unique-local
    "fe80::/10",     // IPv6 link-local
];

/// Lỗi fetch: Ssrf = URL bị chính sách an toàn từ chối; Fetch = lỗi khi thực thi.
#[derive(Debug)]
pub enum Loi {
    Ssrf(String),
    Fetch(String),
}

impl Loi {
    pub fn ten(&self) -> &'static str {
        match self {
            Loi::Ssrf(_) => "SsrfError",
            Loi::Fetch(_) => "FetchError",
        }
    }
    pub fn thong_bao(&self) -> &str {
        match self {
            Loi::Ssrf(s) | Loi::Fetch(s) => s,
        }
    }
}

pub struct Fetcher {
    resolver: HashMap<String, VecDeque<String>>, // bảng phân giải giả lập (có thể "tiến" từng IP)
    dial: HashMap<String, u16>,                   // ip -> cổng cục bộ (ánh xạ về 127.0.0.1)
}

impl Fetcher {
    pub fn tu_moi_truong() -> Fetcher {
        let resolver = std::env::var("SSRF_RESOLVER").ok().and_then(|s| serde_json::from_str::<HashMap<String, Vec<String>>>(&s).ok()).unwrap_or_default();
        let resolver = resolver.into_iter().map(|(k, v)| (k, v.into())).collect();
        let dial = std::env::var("SSRF_DIAL").ok().and_then(|s| serde_json::from_str::<HashMap<String, serde_json::Value>>(&s).ok()).unwrap_or_default();
        let dial = dial.into_iter().filter_map(|(k, v)| v.as_u64().map(|p| (k, p as u16))).collect();
        Fetcher { resolver, dial }
    }

    /// Phân giải tên -> một địa chỉ IP (chuỗi). Nếu host đã là IP thì trả nguyên.
    /// Bảng giả lập: mỗi lần gọi trả IP kế tiếp (IP cuối "dính") -> mô phỏng DNS rebinding.
    fn resolve_once(&mut self, host: &str) -> Result<String, Loi> {
        if host.parse::<IpAddr>().is_ok() {
            return Ok(host.to_string()); // host vốn là địa chỉ IP dạng số
        }
        if let Some(lst) = self.resolver.get_mut(host) {
            let ip = lst.front().cloned().unwrap_or_default();
            if lst.len() > 1 {
                lst.pop_front(); // advance: lần sau trả IP kế tiếp
            }
            return Ok(ip);
        }
        // Không có trong bảng giả lập -> dùng DNS thật (ngoài demo).
        match (host, 0u16).to_socket_addrs() {
            Ok(mut a) => a.next().map(|s| s.ip().to_string()).ok_or_else(|| Loi::Fetch(format!("Không phân giải được tên {host}"))),
            Err(_) => Err(Loi::Fetch(format!("Không phân giải được tên {host}"))),
        }
    }

    /// Chặn nếu IP thuộc dải nội bộ/loopback/link-local… (dùng cho bản AN TOÀN).
    fn validate_ip(&self, ip: &str) -> Result<(), Loi> {
        let addr: IpAddr = ip.parse().map_err(|_| Loi::Ssrf(format!("Chặn SSRF: địa chỉ {ip} không hợp lệ")))?;
        for net in BLOCKED {
            if trong_dai(addr, net) {
                return Err(Loi::Ssrf(format!("Chặn SSRF: địa chỉ {ip} thuộc dải cấm {net}")));
            }
        }
        Ok(())
    }

    /// Mở socket tới (ip, port). Bảng dial (chỉ demo) cho phép ánh xạ ip -> 127.0.0.1:cổng_cục_bộ.
    fn dial(&self, ip: &str, port: u16, timeout: Duration) -> Result<TcpStream, Loi> {
        let dich = match self.dial.get(ip) {
            Some(&p) => format!("127.0.0.1:{p}"),
            None => format!("{ip}:{port}"),
        };
        let addr = dich.to_socket_addrs().ok().and_then(|mut a| a.next()).ok_or_else(|| Loi::Fetch(format!("Không nối được {dich}")))?;
        TcpStream::connect_timeout(&addr, timeout).map_err(|e| Loi::Fetch(format!("Không nối được {dich}: {}", bo_os_error(&e))))
    }

    /// Gửi GET HTTP/1.0 (Connection: close) tới IP đã chọn, đọc phản hồi (giới hạn kích thước).
    fn http_get(&self, ip: &str, host: &str, port: u16, path: &str, timeout: Duration, limit: usize) -> Result<(u16, HashMap<String, String>, Vec<u8>), Loi> {
        let mut s = self.dial(ip, port, timeout)?;
        let _ = s.set_read_timeout(Some(timeout));
        let _ = s.set_write_timeout(Some(timeout));
        let req = format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nAccept: */*\r\nConnection: close\r\n\r\n");
        s.write_all(req.as_bytes()).map_err(|e| Loi::Fetch(bo_os_error(&e)))?;
        let mut data = Vec::new();
        let mut buf = [0u8; 4096];
        while data.len() < limit {
            match s.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => data.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        let cat = data.windows(4).position(|w| w == b"\r\n\r\n");
        let (head, body) = match cat {
            Some(i) => (&data[..i], data[i + 4..].to_vec()),
            None => (&data[..], Vec::new()),
        };
        let head = String::from_utf8_lossy(head);
        let mut lines = head.split("\r\n");
        let dong_dau = lines.next().unwrap_or("");
        let status = dong_dau.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0u16);
        let mut headers = HashMap::new();
        for ln in lines {
            if let Some((k, v)) = ln.split_once(':') {
                headers.insert(k.trim().to_lowercase(), v.trim().to_string());
            }
        }
        Ok((status, headers, body))
    }

    /// Bản NGÂY THƠ: KHÔNG kiểm tra gì. Phân giải tên khi "kiểm tra" rồi phân giải LẠI khi nối
    /// (lỗi TOCTOU), theo mọi chuyển hướng, không giới hạn. Ví dụ dễ bị SSRF – không dùng thật.
    pub fn fetch_naive(&mut self, url_vao: &str) -> Result<(u16, Vec<u8>, Vec<String>), Loi> {
        let mut url = url_vao.to_string();
        let mut log = Vec::new();
        for _ in 0..=MAX_REDIRECT {
            let (_, _, host, port, path) = split_url(&url)?;
            let _ = self.resolve_once(&host)?; // "kiểm tra" hình thức rồi vứt đi (vô dụng)
            let ip = self.resolve_once(&host)?; // nối bằng lần phân giải MỚI -> rebinding lọt
            log.push(format!("nối tới {host} ({ip}):{port}{path}"));
            let (status, headers, body) = self.http_get(&ip, &host, port, &path, HAN_GIAY, usize::MAX)?;
            if la_chuyen_huong(status) {
                if let Some(loc) = headers.get("location") {
                    url = noi_tiep(&url, loc);
                    log.push(format!("theo chuyển hướng -> {url}"));
                    continue;
                }
            }
            return Ok((status, body, log));
        }
        Err(Loi::Fetch("Quá nhiều chuyển hướng".into()))
    }

    /// Bản AN TOÀN: lần lượt áp các lớp phòng thủ SSRF.
    pub fn fetch_safe(&mut self, url_vao: &str) -> Result<(u16, Vec<u8>, Vec<String>), Loi> {
        let mut url = url_vao.to_string();
        let mut log = Vec::new();
        let deadline = Instant::now() + HAN_GIAY;
        for hop in 0..=MAX_REDIRECT {
            let (scheme, userinfo, host, port, path) = split_url(&url)?;
            if scheme != "http" && scheme != "https" {
                return Err(Loi::Ssrf(format!("Chặn SSRF: giao thức '{scheme}' không được phép"))); // (1)
            }
            if userinfo {
                log.push("gỡ bỏ thông tin đăng nhập nhúng trong URL".into()); // (2)
            }
            if host.is_empty() {
                return Err(Loi::Ssrf("Chặn SSRF: URL thiếu tên máy".into()));
            }
            let ip = self.resolve_once(&host)?; // (3) phân giải MỘT lần
            self.validate_ip(&ip)?; // (4) chặn dải nội bộ/loopback/link-local
            let con_lai = deadline.saturating_duration_since(Instant::now());
            if con_lai.is_zero() {
                return Err(Loi::Fetch("Hết thời gian cho phép".into())); // (5)
            }
            log.push(format!("cho phép {host} ({ip}):{port}{path}; nối tới ĐÚNG IP đã kiểm tra"));
            // (6) nối tới chính IP đã kiểm tra (chống rebinding/TOCTOU), (7) giới hạn kích thước
            let (status, headers, body) = self.http_get(&ip, &host, port, &path, con_lai.min(Duration::from_secs(2)), MAX_BYTES)?;
            if la_chuyen_huong(status) {
                if let Some(loc) = headers.get("location") {
                    if hop >= MAX_REDIRECT {
                        return Err(Loi::Ssrf("Chặn SSRF: vượt số lần chuyển hướng cho phép".into())); // (8)
                    }
                    url = noi_tiep(&url, loc);
                    log.push(format!("kiểm tra lại chặng chuyển hướng -> {url}"));
                    continue; // (9) kiểm tra lại MỌI chặng
                }
            }
            return Ok((status, body, log));
        }
        Err(Loi::Fetch("Quá nhiều chuyển hướng".into()))
    }
}

fn la_chuyen_huong(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Tách URL -> (scheme, có_thông_tin_đăng_nhập, host, port, path+query). Theo urllib.parse.urlsplit.
fn split_url(raw: &str) -> Result<(String, bool, String, u16, String), Loi> {
    let (scheme, rest) = match raw.split_once("://") {
        Some((s, r)) => (s.to_lowercase(), r),
        None => {
            // dạng "scheme:path" (vd file:///…): scheme trước dấu ':' đầu
            if let Some((s, r)) = raw.split_once(':') {
                return Ok((s.to_lowercase(), false, String::new(), 0, if r.is_empty() { "/".into() } else { r.to_string() }));
            }
            (String::new(), raw)
        }
    };
    // authority tới dấu '/','?','#' đầu tiên
    let cat = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, sau) = rest.split_at(cat);
    let (userinfo, hostport) = match authority.rsplit_once('@') {
        Some((u, h)) => (!u.is_empty(), h),
        None => (false, authority),
    };
    let (host, port) = tach_host_port(hostport, &scheme);
    let mut path = if sau.is_empty() || sau.starts_with(['?', '#']) { "/".to_string() } else { String::new() };
    // giữ path + query (bỏ fragment #…), giống urlsplit + ghép query
    let khong_fragment = sau.split('#').next().unwrap_or("");
    if khong_fragment.starts_with('/') {
        path = khong_fragment.to_string();
    } else if !khong_fragment.is_empty() {
        path.push_str(khong_fragment);
    }
    if path.is_empty() {
        path = "/".into();
    }
    Ok((scheme, userinfo, host, port, path))
}

fn tach_host_port(hostport: &str, scheme: &str) -> (String, u16) {
    let mac_dinh = if scheme == "https" { 443 } else { 80 };
    if let Some(h) = hostport.strip_prefix('[') {
        // IPv6 literal [::1]:port
        if let Some((host, sau)) = h.split_once(']') {
            let port = sau.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(mac_dinh);
            return (host.to_string(), port);
        }
    }
    match hostport.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => (h.to_lowercase(), p.parse().unwrap_or(mac_dinh)),
        _ => (hostport.to_lowercase(), mac_dinh),
    }
}

/// Ghép URL chuyển hướng tương đối vào URL hiện tại (tương đương urllib.parse.urljoin cho các ca ở đây).
fn noi_tiep(cur: &str, loc: &str) -> String {
    if loc.contains("://") {
        return loc.to_string(); // Location tuyệt đối
    }
    let (scheme, _, host, port, _) = match split_url(cur) {
        Ok(x) => x,
        Err(_) => return loc.to_string(),
    };
    let cong = if (scheme == "http" && port == 80) || (scheme == "https" && port == 443) { String::new() } else { format!(":{port}") };
    if let Some(rest) = loc.strip_prefix("//") {
        return format!("{scheme}://{rest}");
    }
    if loc.starts_with('/') {
        return format!("{scheme}://{host}{cong}{loc}");
    }
    format!("{scheme}://{host}{cong}/{loc}")
}

/// IP có thuộc dải "base/prefix" (IPv4 hoặc IPv6) không.
fn trong_dai(ip: IpAddr, cidr: &str) -> bool {
    let (base, plen) = cidr.split_once('/').unwrap_or((cidr, ""));
    let plen: u32 = plen.parse().unwrap_or(0);
    match (ip, base.parse::<IpAddr>()) {
        (IpAddr::V4(a), Ok(IpAddr::V4(b))) => cung_tien_to_v4(a, b, plen),
        (IpAddr::V6(a), Ok(IpAddr::V6(b))) => cung_tien_to_v6(a, b, plen),
        _ => false,
    }
}

fn cung_tien_to_v4(a: Ipv4Addr, b: Ipv4Addr, plen: u32) -> bool {
    let mask = if plen == 0 { 0 } else { u32::MAX << (32 - plen) };
    u32::from(a) & mask == u32::from(b) & mask
}

fn cung_tien_to_v6(a: Ipv6Addr, b: Ipv6Addr, plen: u32) -> bool {
    let mask = if plen == 0 { 0 } else { u128::MAX << (128 - plen) };
    u128::from(a) & mask == u128::from(b) & mask
}

/// Bỏ đuôi " (os error N)" để thông báo lỗi giống bản Python/Go.
fn bo_os_error(e: &std::io::Error) -> String {
    let s = e.to_string();
    s.split(" (os error").next().unwrap_or(&s).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chan_dai_noi_bo() {
        assert!(trong_dai("169.254.169.254".parse().unwrap(), "169.254.0.0/16"));
        assert!(trong_dai("127.0.0.1".parse().unwrap(), "127.0.0.0/8"));
        assert!(!trong_dai("203.0.113.10".parse().unwrap(), "10.0.0.0/8"));
        assert!(trong_dai("::1".parse().unwrap(), "::1/128"));
        assert!(trong_dai("fe80::1".parse().unwrap(), "fe80::/10"));
    }

    #[test]
    fn tach_url() {
        let (s, u, h, p, path) = split_url("http://admin:secret@203.0.113.10/").unwrap();
        assert_eq!((s.as_str(), u, h.as_str(), p, path.as_str()), ("http", true, "203.0.113.10", 80, "/"));
        let (s, _, h, _, _) = split_url("file:///etc/passwd").unwrap();
        assert_eq!((s.as_str(), h.as_str()), ("file", ""));
        let (_, _, _, _, path) = split_url("http://h/a/b?x=1").unwrap();
        assert_eq!(path, "/a/b?x=1");
    }
}
