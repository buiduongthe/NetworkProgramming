//! Bài 7.7 – Gửi ICMP Echo Request và đo thời gian khứ hồi (socket RAW).
//!
//! Cài đặt Mã giả 7.8 (Mục 7.5.3, Giáo trình Lập trình mạng): INET_CHECKSUM (RFC 1071) và PING.
//! Kế thừa: Mã giả 4.4 – cùng mẫu gửi rồi chờ phản hồi có hạn giờ, bỏ qua gói không khớp.
//! Socket RAW cần quyền quản trị: Linux/macOS chạy bằng sudo, Windows mở cửa sổ lệnh "Run as administrator".
//!
//! Chạy:  sudo cargo run -- [host] [số_lần] [thời_gian_chờ_ms]   (mặc định 127.0.0.1 4 1000)
use socket2::{Domain, Protocol, Socket, Type};
use std::hash::{BuildHasher, Hasher};
use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

/// Mã giả 7.8, dòng 1–11: tổng kiểm tra Internet (RFC 1071).
fn inet_checksum(data: &[u8]) -> u16 {
    let mut b = data.to_vec();
    if b.len() % 2 == 1 {
        b.push(0);
    }
    let mut sum: u32 = 0;
    for i in (0..b.len()).step_by(2) {
        sum += u16::from_be_bytes([b[i], b[i + 1]]) as u32; // FROM_UINT16_BE(b[i..i+1])
    }
    while sum > 0xFFFF {
        sum = (sum & 0xFFFF) + (sum >> 16); // cộng phần tràn
    }
    !(sum as u16) // (NOT sum) AND 0xFFFF
}

/// RANDOM_BYTES(n) chỉ dùng thư viện chuẩn: mỗi RandomState mang khóa băm ngẫu nhiên.
fn random_bytes(n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while out.len() < n {
        let r = std::collections::hash_map::RandomState::new().build_hasher().finish();
        out.extend_from_slice(&r.to_le_bytes());
    }
    out.truncate(n);
    out
}

/// Kết quả một lần ping thất bại.
enum PingError {
    Timeout,
    Io(io::Error),
}

impl From<io::Error> for PingError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            ErrorKind::WouldBlock | ErrorKind::TimedOut => PingError::Timeout,
            _ => PingError::Io(e),
        }
    }
}

/// Mã giả 7.8, dòng 13–35: trả về (địa chỉ, RTT, TTL).
fn ping(host: &str, seq: u16, timeout_ms: u64) -> Result<(IpAddr, Duration, u8), PingError> {
    // FIRST_IPV4(RESOLVE(host))
    let dst: Ipv4Addr = (host, 0)
        .to_socket_addrs()?
        .find_map(|a| match a.ip() {
            IpAddr::V4(ip) => Some(ip),
            _ => None,
        })
        .ok_or(PingError::Io(io::Error::other(format!("Không có địa chỉ IPv4 cho {host}"))))?;
    let raw = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?; // SOCKET(INET, RAW, ICMP): cần quyền quản trị
    // UdpSocket của thư viện chuẩn chỉ bọc các lời gọi sendto/recvfrom nên dùng được cho socket RAW
    let s: UdpSocket = raw.into();
    let id = (std::process::id() & 0xFFFF) as u16; // PROCESS_ID() AND 0xFFFF
    let mut p = vec![8, 0, 0, 0]; // kiểu 8, mã 0, checksum 0
    p.extend_from_slice(&id.to_be_bytes());
    p.extend_from_slice(&seq.to_be_bytes());
    p.extend(random_bytes(32)); // phần dữ liệu
    let c = inet_checksum(&p).to_be_bytes();
    p[2..4].copy_from_slice(&c); // điền tổng kiểm tra
    if seq == 1 {
        let hex: Vec<String> = p[..8].iter().map(|b| format!("{b:02X}")).collect();
        println!("[ping] Tiêu đề ICMP gửi đi: {} (kiểu, mã, checksum, id, seq)", hex.join(" "));
    }
    let t0 = Instant::now(); // NOW_PRECISE()
    s.send_to(&p, SocketAddr::new(IpAddr::V4(dst), 0))?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms); // hạn chót chung
    loop {
        // bỏ qua gói ICMP khác; mỗi lần chỉ chờ phần thời gian còn lại (tối thiểu 1 ms)
        let left = deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
        s.set_read_timeout(Some(left))?; // SETOPT(s, RECV_TIMEOUT, MAX(1, deadline - NOW()))
        let mut d = [0u8; 1500];
        let (n, addr) = s.recv_from(&mut d)?; // RAW: có cả tiêu đề IP
        let ihl = ((d[0] & 0x0F) * 4) as usize; // độ dài tiêu đề IPv4
        if n < ihl + 8 {
            continue; // gói quá ngắn
        }
        let m = &d[ihl..n];
        let rid = u16::from_be_bytes([m[4], m[5]]);
        let rseq = u16::from_be_bytes([m[6], m[7]]);
        if m[0] == 0 && rid == id && rseq == seq {
            return Ok((addr.ip(), t0.elapsed(), d[8])); // d[8]: TTL
        }
        println!("[ping]   bỏ qua gói ICMP kiểu {} (không phải Echo Reply của tiến trình này)", m[0]);
    }
    // CLOSE(s): socket đóng khi s ra khỏi phạm vi
}

fn int_arg(args: &[String], i: usize, default: u64) -> u64 {
    match args.get(i) {
        Some(s) => s.parse().unwrap_or_else(|_| {
            eprintln!("Tham số phải là số nguyên: {s}");
            std::process::exit(1)
        }),
        None => default,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let host = args.first().map(String::as_str).unwrap_or("127.0.0.1");
    let count = int_arg(&args, 1, 4).clamp(1, 65535) as u16;
    let timeout_ms = int_arg(&args, 2, 1000);
    println!("[ping] PING {host}: {count} gói Echo Request, dữ liệu 32 byte, chờ tối đa {timeout_ms} ms");
    let mut rtts: Vec<f64> = Vec::new();
    for seq in 1..=count {
        match ping(host, seq, timeout_ms) {
            Ok((addr, rtt, ttl)) => {
                let ms = rtt.as_secs_f64() * 1000.0;
                rtts.push(ms);
                println!("[ping] Trả lời từ {addr}: seq={seq} ttl={ttl} thời gian={ms:.3} ms");
            }
            Err(PingError::Timeout) => println!("[ping] seq={seq}: hết thời gian chờ"),
            Err(PingError::Io(e)) if e.kind() == ErrorKind::PermissionDenied => {
                eprintln!("[ping] Socket RAW cần quyền quản trị (sudo hoặc Run as administrator)");
                std::process::exit(1);
            }
            Err(PingError::Io(e)) => println!("[ping] seq={seq}: lỗi {e}"),
        }
        if seq < count {
            thread::sleep(Duration::from_secs(1));
        }
    }
    let lost = 100 * (count as usize - rtts.len()) / count as usize;
    let avg = if rtts.is_empty() {
        "không có".to_string()
    } else {
        format!("{:.3} ms", rtts.iter().sum::<f64>() / rtts.len() as f64)
    };
    println!("[ping] Đã gửi {count}, nhận {}, mất {lost}%; RTT trung bình {avg}", rtts.len());
}
