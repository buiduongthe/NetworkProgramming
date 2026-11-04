//! Bài 4.3 – Tự tạo và phân tích truy vấn DNS qua UDP (RFC 1035).
//!
//! Cài đặt Mã giả 4.5 (Mục 4.3.2, Giáo trình Lập trình mạng): BUILD_QUERY, SKIP_NAME, DNS_QUERY.
//! Kế thừa: client tra cứu DNS của Mục 2.5 – thay RESOLVE của hệ điều hành bằng truy vấn tự tạo.
//! Bổ sung so với mã giả (theo đoạn văn sau Mã giả 4.5): kiểm tra mọi chỉ số pos không vượt độ dài
//! thông điệp và kiểm tra cờ TC trước khi đọc phần trả lời.
//!
//! Chạy:  cargo run -q --release -- <ip_dns_server> <ten_mien> [A|AAAA] [cong]     (mặc định A, cổng 53)
//! Ví dụ: cargo run -q --release -- 8.8.8.8 example.com AAAA

use std::env;
use std::fmt;
use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::process;
use std::time::Duration;

/// Các lỗi của truy vấn DNS.
#[derive(Debug)]
enum DnsError {
    Timeout,          // hết thời gian chờ phản hồi
    Protocol(String), // phản hồi sai định dạng hoặc không khớp truy vấn
    Io(io::Error),    // lỗi socket khác
}

impl fmt::Display for DnsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DnsError::Timeout => write!(f, "hết thời gian chờ"),
            DnsError::Protocol(m) => write!(f, "{m}"),
            DnsError::Io(e) => write!(f, "{e}"),
        }
    }
}

fn type_name(qtype: u16) -> &'static str {
    match qtype {
        1 => "A",
        28 => "AAAA",
        _ => "?",
    }
}

fn rcode_name(rcode: u8) -> &'static str {
    match rcode {
        0 => "NOERROR",
        1 => "FORMERR",
        2 => "SERVFAIL",
        3 => "NXDOMAIN",
        4 => "NOTIMP",
        5 => "REFUSED",
        _ => "?",
    }
}

fn hexdump(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

/// UINT16_BE: số 16 bit theo thứ tự byte mạng (big-endian).
fn u16_be(m: &[u8], pos: usize) -> u16 {
    u16::from_be_bytes([m[pos], m[pos + 1]])
}

/// Mã giả 4.5, dòng 1–9: BUILD_QUERY(id, name, qtype).
fn build_query(qid: u16, name: &str, qtype: u16) -> Result<Vec<u8>, String> {
    let mut q = Vec::new();
    q.extend_from_slice(&qid.to_be_bytes()); // dòng 2: ID
    q.extend_from_slice(&0x0100u16.to_be_bytes()); //         cờ RD = 1
    q.extend_from_slice(&1u16.to_be_bytes()); // dòng 3: QDCOUNT = 1
    q.extend_from_slice(&[0u8; 6]); //                     ANCOUNT = NSCOUNT = ARCOUNT = 0
    for label in name.trim_end_matches('.').split('.') {
        // dòng 4
        if !label.is_ascii() {
            // dòng 5: ENCODE(label, "ASCII") – tên quốc tế cần chuyển sang Punycode trước
            return Err(format!("Nhãn không phải ASCII: '{label}' (hãy nhập dạng Punycode xn--...)"));
        }
        if label.is_empty() || label.len() > 63 {
            return Err(format!("Nhãn không hợp lệ: '{label}'"));
        }
        q.push(label.len() as u8); // dòng 6: BYTE(LENGTH(b)) + b
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0); // dòng 8: kết thúc tên
    q.extend_from_slice(&qtype.to_be_bytes()); // QTYPE
    q.extend_from_slice(&1u16.to_be_bytes()); //  QCLASS = IN
    Ok(q)
}

/// Kiểm tra chỉ số (bổ sung): phản hồi sai định dạng không được làm đọc ngoài thông điệp.
fn need(m: &[u8], end: usize) -> Result<(), DnsError> {
    if end > m.len() {
        return Err(DnsError::Protocol("Phản hồi sai định dạng: vượt quá độ dài thông điệp".to_string()));
    }
    Ok(())
}

/// Mã giả 4.5, dòng 10–17: SKIP_NAME – nhảy qua một tên (không giải mã).
fn skip_name(m: &[u8], mut pos: usize) -> Result<usize, DnsError> {
    loop {
        need(m, pos + 1)?;
        let n = m[pos];
        if n == 0 {
            return Ok(pos + 1); // hết tên
        }
        if n & 0xC0 == 0xC0 {
            need(m, pos + 2)?;
            return Ok(pos + 2); // con trỏ nén 2 byte
        }
        if n & 0xC0 != 0 {
            return Err(DnsError::Protocol("Phản hồi sai định dạng: nhãn kiểu lạ".to_string()));
        }
        pos += 1 + n as usize;
    }
}

/// RANDOM_UINT16(): số ngẫu nhiên an toàn mật mã từ hệ điều hành (crate getrandom).
fn random_u16() -> u16 {
    let mut b = [0u8; 2];
    getrandom::fill(&mut b).expect("không lấy được số ngẫu nhiên từ hệ điều hành");
    u16::from_be_bytes(b)
}

/// Mã giả 4.5, dòng 18–38: DNS_QUERY(server, name, qtype) với server = (ip, cổng); trả về (rcode, answers).
fn dns_query(server: SocketAddr, name: &str, qtype: u16) -> Result<(u8, Vec<String>), DnsError> {
    // SOCKET(FAMILY(server.ip), DGRAM): IPv4 hoặc IPv6, gắn vào cổng tạm bất kỳ
    let local = if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
    let s = UdpSocket::bind(local).map_err(DnsError::Io)?;
    s.set_read_timeout(Some(Duration::from_millis(2000))).ok(); // dòng 19: RECV_TIMEOUT 2000 ms
    let qid = random_u16(); // dòng 20: RANDOM_UINT16()
    let q = build_query(qid, name, qtype).map_err(DnsError::Protocol)?;
    println!("[client] Truy vấn {} cho {name} tới {server} (id=0x{qid:04X}, {} byte)", type_name(qtype), q.len());
    println!("[client] Truy vấn (hex): {}", hexdump(&q));
    s.send_to(&q, server).map_err(DnsError::Io)?; // dòng 21
    let mut buf = vec![0u8; 4096];
    let (n, addr) = match s.recv_from(&mut buf) {
        // dòng 22
        Ok(v) => v,
        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => return Err(DnsError::Timeout),
        Err(e) => return Err(DnsError::Io(e)),
    };
    drop(s); // CLOSE(s)
    let m = &buf[..n];
    if addr != server {
        // dòng 23: IF addr ≠ server
        return Err(DnsError::Protocol(format!("Phản hồi từ địa chỉ lạ {addr}")));
    }
    need(m, 12)?;
    if u16_be(m, 0) != qid {
        // dòng 24
        return Err(DnsError::Protocol("ID của phản hồi không khớp truy vấn".to_string()));
    }
    let rcode = m[3] & 0x0F; // dòng 25: 3 = NXDOMAIN
    let ancount = u16_be(m, 6); // dòng 26
    println!(
        "[client] Phản hồi {} byte từ {addr}, RCODE={rcode} ({}), ANCOUNT={ancount}",
        m.len(),
        rcode_name(rcode)
    );
    println!("[client] Phản hồi (hex): {}", hexdump(m));
    if m[2] & 0x02 != 0 {
        // bổ sung: cờ TC
        return Err(DnsError::Protocol("Phản hồi bị cắt (TC = 1): cần hỏi lại qua TCP cổng 53".to_string()));
    }
    let mut pos = skip_name(m, 12)? + 4; // dòng 27: bỏ QTYPE, QCLASS
    let mut answers = Vec::new();
    for _ in 0..ancount {
        // dòng 29
        pos = skip_name(m, pos)?; // dòng 30
        need(m, pos + 10)?;
        let rtype = u16_be(m, pos); // dòng 31
        let rdlen = u16_be(m, pos + 8) as usize; // dòng 32
        need(m, pos + 10 + rdlen)?;
        let rdata = &m[pos + 10..pos + 10 + rdlen]; // dòng 33
        if rtype == 1 && rdlen == 4 {
            // dòng 34: bản ghi A
            answers.push(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]).to_string());
        }
        if rtype == 28 && rdlen == 16 {
            // dòng 35: bản ghi AAAA
            let mut b = [0u8; 16];
            b.copy_from_slice(rdata);
            answers.push(Ipv6Addr::from(b).to_string());
        }
        pos += 10 + rdlen; // dòng 36
    }
    Ok((rcode, answers))
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 2 {
        println!("Cách dùng: cargo run -q --release -- <ip_dns_server> <ten_mien> [A|AAAA] [cong]");
        process::exit(1);
    }
    let server_ip: IpAddr = match args[0].parse() {
        Ok(ip) => ip,
        Err(_) => {
            println!("Địa chỉ DNS server phải là địa chỉ IPv4/IPv6 dạng số: {}", args[0]);
            process::exit(1);
        }
    };
    let name = &args[1];
    let tname = args.get(2).map(|t| t.to_uppercase()).unwrap_or_else(|| "A".to_string());
    let qtype = match tname.as_str() {
        "A" => 1,
        "AAAA" => 28,
        _ => {
            println!("Kiểu bản ghi chỉ hỗ trợ A hoặc AAAA");
            process::exit(1);
        }
    };
    let port: u16 = match args.get(3) {
        None => 53,
        Some(t) => match t.trim().parse::<i64>() {
            Ok(p) if (1..=65535).contains(&p) => p as u16,
            Ok(p) => {
                println!("Cổng không hợp lệ: {p}");
                process::exit(1);
            }
            Err(_) => {
                println!("Cổng phải là số nguyên: {t}");
                process::exit(1);
            }
        },
    };
    match dns_query(SocketAddr::new(server_ip, port), name, qtype) {
        Ok((_rcode, answers)) => {
            for a in &answers {
                println!("[client] Địa chỉ: {a}");
            }
            if answers.is_empty() {
                println!("[client] Không có bản ghi {tname}");
            }
        }
        Err(DnsError::Timeout) => {
            println!("[client] Lỗi: hết thời gian chờ phản hồi (2000 ms)");
            process::exit(1);
        }
        Err(DnsError::Protocol(m)) => {
            println!("[client] Lỗi giao thức: {m}");
            process::exit(1);
        }
        Err(e) => {
            println!("[client] Lỗi: {e}");
            process::exit(1);
        }
    }
}
