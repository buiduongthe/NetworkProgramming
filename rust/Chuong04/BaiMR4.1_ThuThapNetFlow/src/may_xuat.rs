//! Máy xuất NetFlow v5 giả lập (exporter) gửi qua UDP.
//!
//! Sinh các bản ghi luồng (flow) giả lập theo một bộ sinh số giả ngẫu nhiên đơn giản (LCG)
//! có hạt giống cố định, nên bản Rust, Python và Go sinh ra đúng cùng một dữ liệu.
//! Mỗi gói UDP chứa tối đa 30 bản ghi. Tùy chọn --bo-goi K cố ý KHÔNG gửi gói thứ K
//! (mô phỏng mất gói trên đường truyền); cuối cùng gửi thêm một datagram rác để thử bộ thu.

use crate::netflow_v5::{Header, Record, MAX_RECORDS};
use crate::{die, split_args};
use std::net::{Ipv4Addr, UdpSocket};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const HOSTS: [&str; 6] = ["10.20.0.11", "10.20.0.12", "10.20.0.13", "10.20.0.14", "10.20.1.5", "10.20.1.6"];
const SERVICES: [(&str, u16, u8); 5] = [
    ("10.20.1.194", 443, 6),
    ("10.20.1.194", 80, 6),
    ("10.20.1.195", 5432, 6),
    ("10.20.1.196", 53, 17),
    ("203.0.113.10", 443, 6),
];

/// x = (x * 1103515245 + 12345) mod 2^31, lấy 15 bit cao (các bit thấp của LCG kém ngẫu nhiên).
/// Đủ để sinh dữ liệu thử, KHÔNG dùng cho bảo mật.
struct Lcg {
    x: u64,
}

impl Lcg {
    fn next(&mut self) -> u64 {
        self.x = (self.x * 1103515245 + 12345) & 0x7FFF_FFFF;
        self.x >> 16
    }
}

/// Một luồng giả lập (các trường cần cho bản ghi).
struct Flow {
    src: &'static str,
    dst: &'static str,
    sport: u16,
    dport: u16,
    proto: u8,
    pkts: u32,
    octets: u32,
    first: u32,
    last: u32,
}

fn make_flows(n: usize, seed: u64) -> Vec<Flow> {
    let mut rng = Lcg { x: seed };
    let mut flows = Vec::new();
    for _ in 0..n {
        let src = HOSTS[(rng.next() % HOSTS.len() as u64) as usize];
        let (dst, dport, proto) = SERVICES[(rng.next() % SERVICES.len() as u64) as usize];
        let pkts = 1 + rng.next() % 200;
        let octets = pkts * (60 + rng.next() % 1400);
        let sport = 32768 + rng.next() % 28000;
        let first = rng.next() % 60000;
        let last = first + rng.next() % 5000;
        flows.push(Flow { src, dst, sport: sport as u16, dport, proto, pkts: pkts as u32, octets: octets as u32, first: first as u32, last: last as u32 });
    }
    flows
}

fn pack_packet(flows: &[Flow], seq: u32, uptime_ms: u32) -> Vec<u8> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let mut out = Vec::new();
    Header {
        version: 5,
        count: flows.len() as u16,
        sys_uptime: uptime_ms,
        unix_secs: now.as_secs() as u32,
        unix_nsecs: now.subsec_nanos(),
        flow_sequence: seq,
        engine_type: 0,
        engine_id: 1,
        sampling: 0,
    }
    .pack(&mut out);
    for f in flows {
        Record {
            src: f.src.parse().unwrap(),
            dst: f.dst.parse().unwrap(),
            nexthop: Ipv4Addr::UNSPECIFIED,
            input: 1,
            output: 2,
            pkts: f.pkts,
            octets: f.octets,
            first: f.first,
            last: f.last,
            sport: f.sport,
            dport: f.dport,
            tcp_flags: if f.proto == 6 { 0x1B } else { 0 },
            proto: f.proto,
            tos: 0,
            src_as: 0,
            dst_as: 0,
            src_mask: 24,
            dst_mask: 27,
        }
        .pack(&mut out);
    }
    out
}

pub fn run(args: &[String]) {
    let (pos, opt) = split_args(args);
    if pos.len() < 2 {
        die("Cách dùng: xuat <host> <port> [--so-luong N] [--bo-goi K]");
    }
    let num = |k: &str, d: usize| opt.iter().find(|(x, _)| x == k).map(|(_, v)| v.parse().unwrap_or_else(|_| die(&format!("--{k} sai: {v}")))).unwrap_or(d);
    let (so_luong, bo_goi) = (num("so-luong", 100), num("bo-goi", 0));
    let port: u16 = pos[1].parse().unwrap_or_else(|_| die(&format!("Cổng sai: {}", pos[1])));
    let target = (pos[0].as_str(), port);
    let flows = make_flows(so_luong, 2026);
    let sock = UdpSocket::bind("0.0.0.0:0").unwrap_or_else(|e| die(&format!("Không tạo được socket: {e}")));
    let mut seq: u32 = 0;
    for (k, chunk) in flows.chunks(MAX_RECORDS).enumerate().map(|(i, c)| (i + 1, c)) {
        let data = pack_packet(chunk, seq, 120000 + k as u32 * 1000);
        if k == bo_goi {
            println!("[xuat] Gói #{k}: {} bản ghi, seq={seq} -> BỎ (giả lập mất gói)", chunk.len());
        } else {
            if let Err(e) = sock.send_to(&data, target) {
                println!("[xuat] Lỗi gửi: {e}");
            }
            println!("[xuat] Gói #{k}: {} bản ghi, seq={seq}, {} byte", chunk.len(), data.len());
        }
        seq += chunk.len() as u32; // flow_sequence đếm tổng số luồng đã xuất
        thread::sleep(Duration::from_millis(50));
    }
    let _ = sock.send_to(b"xin chao, day khong phai NetFlow", target);
    println!("[xuat] Gửi thêm 1 datagram rác để thử bộ thu");
}
