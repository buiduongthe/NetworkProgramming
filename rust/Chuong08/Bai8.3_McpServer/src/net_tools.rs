//! Bài 8.3 – Thân ba công cụ mạng và lớp kiểm soát đích của MCP server.
//!
//! - Mã giả 1.1 RESOLVE_HOST, 1.2 PORT_CHECK, 7.8 PING: chép gọn, PRINT đổi thành LOG ra stderr
//!   (mọi dòng in ra stdout sẽ chen vào luồng thông điệp MCP – Mục 8.3.5).
//! - Mã giả 8.7 VALIDATE_NAME, VALIDATE_TARGET, VALIDATE_PORT (Mục 8.5.4): đích phải nằm trong ALLOWED_NETS.
//! Công cụ dùng socket API, không gọi shell.
use serde_json::Value;
use socket2::{Domain, Protocol, Socket, Type};
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// Lỗi khi thực thi công cụ; cả bốn loại đều được CALL_TOOL trả về mô hình với isError = true.
pub enum ToolError {
    Net(String),     // NetError: máy không phản hồi, không mở được socket…
    Policy(String),  // PolicyError: đầu vào bị chính sách từ chối
    Resolve(String), // ResolveError: không phân giải được tên
    Timeout(String), // Timeout: hết thời gian chờ
}

impl ToolError {
    pub fn kind(&self) -> &'static str {
        match self {
            ToolError::Net(_) => "NetError",
            ToolError::Policy(_) => "PolicyError",
            ToolError::Resolve(_) => "ResolveError",
            ToolError::Timeout(_) => "Timeout",
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ToolError::Net(m) | ToolError::Policy(m) | ToolError::Resolve(m) | ToolError::Timeout(m) => write!(f, "{m}"),
        }
    }
}

/// LOG: nhật ký chỉ ghi ra STDERR.
pub fn log(msg: &str) {
    eprintln!("[mcp] {msg}");
}

pub fn family_name(a: &IpAddr) -> &'static str {
    if a.is_ipv4() {
        "INET"
    } else {
        "INET6"
    }
}

/// Mã giả 1.1 – RESOLVE_HOST(name), PRINT -> LOG(stderr); lỗi phân giải -> danh sách rỗng.
pub fn resolve_host(name: &str) -> Vec<IpAddr> {
    match (name, 0).to_socket_addrs() {
        Ok(addrs) => {
            let addrs: Vec<IpAddr> = addrs.map(|a| a.ip()).collect();
            for a in &addrs {
                log(&format!("  resolve: {} {a}", family_name(a)));
            }
            addrs
        }
        Err(e) => {
            log(&format!("  resolve: Không phân giải được {name} ({e})"));
            Vec::new()
        }
    }
}

/// Mã giả 1.2 – PORT_CHECK(host, port, timeout_ms) -> OPEN | CLOSED | FILTERED | UNRESOLVED.
pub fn port_check(host: &str, port: u16, timeout_ms: u64) -> &'static str {
    let addrs = resolve_host(host);
    if addrs.is_empty() {
        return "UNRESOLVED";
    }
    let mut state = "FILTERED";
    for a in addrs {
        // CONNECT có CONNECT_TIMEOUT; socket đóng ngay khi ra khỏi phạm vi
        match TcpStream::connect_timeout(&SocketAddr::new(a, port), Duration::from_millis(timeout_ms)) {
            Ok(_) => return "OPEN",
            Err(e) if e.kind() == ErrorKind::ConnectionRefused => state = "CLOSED", // nhận gói RST
            Err(_) => continue, // Timeout, NetworkUnreachable, HostUnreachable
        }
    }
    state
}

/// Mã giả 7.8 – INET_CHECKSUM (RFC 1071).
fn inet_checksum(data: &[u8]) -> u16 {
    let mut b = data.to_vec();
    if b.len() % 2 == 1 {
        b.push(0);
    }
    let mut sum: u32 = b.chunks(2).map(|w| u16::from_be_bytes([w[0], w[1]]) as u32).sum();
    while sum > 0xFFFF {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Mã giả 7.8 – PING(host, seq, timeout_ms) bằng socket RAW (cần quyền quản trị); hạn chót chung.
/// Trả về (địa chỉ, RTT ms, TTL).
pub fn ping(dst: IpAddr, seq: u16, timeout_ms: u64) -> Result<(IpAddr, f64, u8), ToolError> {
    let IpAddr::V4(_) = dst else {
        return Err(ToolError::Net("PING chỉ hỗ trợ IPv4".into())); // FIRST_IPV4
    };
    let raw = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))
        .map_err(|_| ToolError::Net("Không mở được socket RAW: cần quyền quản trị".into()))?;
    let s: UdpSocket = raw.into(); // UdpSocket chỉ bọc sendto/recvfrom nên dùng được cho socket RAW
    let id = (std::process::id() & 0xFFFF) as u16;
    let mut p = vec![8, 0, 0, 0];
    p.extend_from_slice(&id.to_be_bytes());
    p.extend_from_slice(&seq.to_be_bytes());
    while p.len() < 8 + 32 {
        // RANDOM_BYTES(32): khóa băm ngẫu nhiên của RandomState
        let r = std::collections::hash_map::RandomState::new().build_hasher().finish();
        p.extend_from_slice(&r.to_le_bytes());
    }
    let c = inet_checksum(&p).to_be_bytes();
    p[2..4].copy_from_slice(&c);
    let net = |e: std::io::Error| ToolError::Net(e.to_string());
    let t0 = Instant::now();
    s.send_to(&p, SocketAddr::new(dst, 0)).map_err(net)?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms); // hạn chót chung
    loop {
        // bỏ qua gói ICMP khác
        let left = deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
        s.set_read_timeout(Some(left)).map_err(net)?;
        let mut d = [0u8; 1500];
        let (n, addr) = match s.recv_from(&mut d) {
            Ok(x) => x,
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                return Err(ToolError::Timeout(format!("Không nhận được Echo Reply trong {timeout_ms} ms")));
            }
            Err(e) => return Err(net(e)),
        };
        let ihl = ((d[0] & 0x0F) * 4) as usize;
        if n < ihl + 8 {
            continue;
        }
        let m = &d[ihl..n];
        if m[0] == 0 && u16::from_be_bytes([m[4], m[5]]) == id && u16::from_be_bytes([m[6], m[7]]) == seq {
            return Ok((addr.ip(), t0.elapsed().as_secs_f64() * 1000.0, d[8]));
        }
    }
}

// ---------------- Mã giả 8.7 – Kiểm soát đích của công cụ mạng ----------------

/// Một mạng CIDR, ví dụ 10.10.0.0/16 hoặc ::1/128.
pub struct Cidr {
    net: IpAddr,
    prefix: u32,
}

impl Cidr {
    fn parse(text: &str) -> Option<Cidr> {
        let (ip, prefix) = text.split_once('/').unwrap_or((text, ""));
        let net: IpAddr = ip.trim().parse().ok()?;
        let max = if net.is_ipv4() { 32 } else { 128 };
        let prefix = if prefix.is_empty() { max } else { prefix.trim().parse().ok()? };
        (prefix <= max).then_some(Cidr { net, prefix })
    }

    /// a IN net: so sánh `prefix` bit đầu của hai địa chỉ cùng họ.
    fn contains(&self, a: &IpAddr) -> bool {
        let (x, y, bits) = match (self.net, a) {
            (IpAddr::V4(n), IpAddr::V4(a)) => (u32::from(n) as u128, u32::from(*a) as u128, 32),
            (IpAddr::V6(n), IpAddr::V6(a)) => (u128::from(n), u128::from(*a), 128),
            _ => return false,
        };
        if self.prefix == 0 {
            return true;
        }
        let shift = bits - self.prefix;
        (x >> shift) == (y >> shift)
    }
}

impl fmt::Display for Cidr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.net, self.prefix)
    }
}

/// CONST ALLOWED_NETS ← PARSE_CIDRS(ENV("ALLOWED_NETS")); biến vắng → danh sách rỗng (chặn mọi đích).
pub static ALLOWED_NETS: LazyLock<Vec<Cidr>> = LazyLock::new(|| {
    let text = std::env::var("ALLOWED_NETS").unwrap_or_default();
    text.split(',').map(str::trim).filter(|x| !x.is_empty()).filter_map(Cidr::parse).collect()
});

/// VALIDATE_NAME(host): địa chỉ IP dạng số (kể cả IPv6 "::1") được nhận ngay; tên miền phải gồm
/// tối đa 253 ký tự [A-Za-z0-9.-].
pub fn validate_name(host: &Value) -> Result<String, ToolError> {
    let bad = || ToolError::Policy("Tên máy không hợp lệ".into());
    let host = host.as_str().ok_or_else(bad)?;
    if host.parse::<IpAddr>().is_ok() {
        return Ok(host.to_string()); // IS_IP_ADDRESS(host)
    }
    let ok_chars = !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if host.len() > 253 || !ok_chars {
        return Err(bad());
    }
    Ok(host.to_string())
}

/// VALIDATE_TARGET(host): mọi địa chỉ phân giải được đều phải nằm trong ALLOWED_NETS;
/// trả về đúng địa chỉ đã kiểm tra (chống DNS rebinding).
pub fn validate_target(host: &Value) -> Result<IpAddr, ToolError> {
    let name = validate_name(host)?;
    let addrs: Vec<IpAddr> = (name.as_str(), 0) // RESOLVE
        .to_socket_addrs()
        .map_err(|_| ToolError::Resolve(format!("Không phân giải được {name}")))?
        .map(|a| a.ip())
        .collect();
    if addrs.is_empty() {
        return Err(ToolError::Resolve(format!("Không phân giải được {name}")));
    }
    for a in &addrs {
        if !ALLOWED_NETS.iter().any(|net| net.contains(a)) {
            // IN_ANY
            return Err(ToolError::Policy(format!("Ngoài phạm vi cho phép: {a}")));
        }
    }
    Ok(addrs[0]) // FIRST(addrs)
}

/// VALIDATE_PORT(p): số nguyên 1–65535.
pub fn validate_port(p: &Value) -> Result<u16, ToolError> {
    match p.as_u64() {
        Some(p) if (1..=65535).contains(&p) => Ok(p as u16),
        _ => Err(ToolError::Policy("Cổng không hợp lệ".into())),
    }
}
