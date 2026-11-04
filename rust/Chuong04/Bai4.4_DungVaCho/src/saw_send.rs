//! Bên gửi của giao thức dừng và chờ (stop-and-wait ARQ) trên UDP.
//!
//! Cài đặt Mã giả 4.6 SAW_SEND (Mục 4.4.1) với RTO ước lượng theo RFC 6298 (Mục 4.4.2):
//! SRTT, RTTVAR, RTO = SRTT + max(G, K·RTTVAR); nhân đôi RTO khi hết thời gian chờ; thuật toán Karn.
//! Kế thừa: Mã giả 4.4 (vòng chờ phản hồi đúng địa chỉ, đúng mã định danh trong phần thời gian còn lại).

use crate::endpoint::parse_endpoint;
use crate::saw_common::*;
use std::fs;
use std::io::{self, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::time::{Duration, Instant};

/// RTO() / UPDATE_RTT() / BACKOFF_RTO() theo RFC 6298 (Mục 4.4.2).
struct RttEstimator {
    srtt: Option<f64>,
    rttvar: f64,
    rto: f64,
}

impl RttEstimator {
    const ALPHA: f64 = 1.0 / 8.0;
    const BETA: f64 = 1.0 / 4.0;
    const K: f64 = 4.0;
    const G: f64 = 1.0; // độ phân giải đồng hồ 1 ms

    fn new() -> Self {
        RttEstimator { srtt: None, rttvar: 0.0, rto: RTO_INIT }
    }

    /// UPDATE_RTT(R')
    fn update(&mut self, r: f64) {
        let srtt = match self.srtt {
            None => {
                self.rttvar = r / 2.0; // mẫu đầu tiên
                r
            }
            Some(old) => {
                // RTTVAR tính trước, với SRTT cũ
                self.rttvar = (1.0 - Self::BETA) * self.rttvar + Self::BETA * (old - r).abs();
                (1.0 - Self::ALPHA) * old + Self::ALPHA * r
            }
        };
        self.srtt = Some(srtt);
        self.rto = (srtt + Self::G.max(Self::K * self.rttvar)).clamp(RTO_MIN, MAX_RTO);
    }

    /// BACKOFF_RTO(): RTO ← MIN(2 × RTO, MAX_RTO)
    fn backoff(&mut self) {
        self.rto = (2.0 * self.rto).min(MAX_RTO);
    }
}

/// WAIT_ACK: chờ ACK mang đúng seq từ đúng peer trong rto_ms; bỏ ACK cũ, ACK lạ.
/// Trả Ok(true) khi nhận được ACK, Ok(false) khi hết thời gian chờ.
fn wait_ack(s: &UdpSocket, peer: SocketAddr, seq: u32, rto_ms: f64) -> io::Result<bool> {
    let deadline = Instant::now() + Duration::from_secs_f64(rto_ms / 1000.0);
    let mut buf = vec![0u8; 65535];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        s.set_read_timeout(Some(remaining.max(Duration::from_millis(1))))?;
        match s.recv_from(&mut buf) {
            Ok((n, addr)) => {
                let d = &buf[..n];
                if addr == peer && n >= 5 && d[4] == ACK && u32::from_be_bytes([d[0], d[1], d[2], d[3]]) == seq {
                    return Ok(true);
                }
            }
            // Timeout, hoặc ConnectionReset (Windows: ICMP port unreachable) – xử lý như hết thời gian chờ
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::ConnectionReset) => {
                return Ok(false)
            }
            Err(e) => return Err(e),
        }
    }
}

/// Mã giả 4.6 – SAW_SEND; trả về (số datagram đã gửi, số lần gửi lại).
fn saw_send(s: &UdpSocket, peer: SocketAddr, chunks: &[&[u8]], est: &mut RttEstimator, loss: &mut LossSim) -> Result<(u32, u32), String> {
    let (mut seq, mut sent, mut retrans) = (0u32, 0u32, 0u32); // dòng 3
    // dòng 4: APPEND(chunks, FIN_MARK) – None đánh dấu FIN sau khối cuối
    let items: Vec<Option<&[u8]>> = chunks.iter().map(|c| Some(*c)).chain(std::iter::once(None)).collect();
    for c in items {
        let (typ, body): (u8, &[u8]) = match c {
            Some(b) => (DATA, b),
            None => (FIN, &[]), // dòng 5
        };
        let mut tries = 0; // dòng 6
        loop {
            // dòng 7: LOOP
            let mut dgram = header(seq, typ); // UINT32_BE(seq) + BYTE(type) + c
            dgram.extend_from_slice(body);
            let dropped = loss.drop_next();
            if !dropped {
                s.send_to(&dgram, peer).map_err(|e| e.to_string())?; // dòng 8
            }
            sent += 1;
            let kind = if typ == FIN { "FIN" } else { "DATA" };
            let note = if dropped { " – mô phỏng mất gói" } else { "" };
            println!("[gửi] seq={seq} {kind} {} byte, RTO {:.0} ms{note}", dgram.len(), est.rto);
            let t0 = Instant::now(); // dòng 9
            if wait_ack(s, peer, seq, est.rto).map_err(|e| e.to_string())? {
                // dòng 11
                if tries == 0 {
                    est.update(t0.elapsed().as_secs_f64() * 1000.0); // dòng 12: Karn – chỉ đo khi chưa gửi lại
                }
                break; // dòng 13
            }
            // dòng 14: CATCH Timeout
            tries += 1;
            retrans += 1;
            println!("[gửi] Hết {:.0} ms chờ ACK {seq}, gửi lại (lần {tries})", est.rto);
            est.backoff(); // dòng 16
            if tries > MAX_TRIES {
                // dòng 17
                return Err(format!("Không nhận được ACK {seq} sau {MAX_TRIES} lần gửi lại"));
            }
        }
        seq += 1; // dòng 20
    }
    Ok((sent, retrans))
}

pub fn run(args: &[String]) -> i32 {
    let path = &args[0];
    let rest = &args[1..];
    let (host, port) = parse_endpoint(&rest[..rest.len().min(2)], "127.0.0.1", 9000);
    let mut loss = LossSim::new(rest.get(2).map(|s| s.as_str()).unwrap_or(""));
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            println!("[gửi] Không đọc được tệp {path}: {e}");
            return 1;
        }
    };
    let chunks: Vec<&[u8]> = data.chunks(CHUNK).collect(); // các khối ≤ 1195 byte
    let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
    println!("[gửi] Tệp {name}: {} byte, {} khối; SHA-256:", data.len(), chunks.len());
    println!("[gửi]   {}", sha256_file(path).unwrap_or_default());
    println!("[gửi] Gửi tới {host}:{port}; mô phỏng mất DATA/FIN: {}", loss.describe());
    let peer = match (host.as_str(), port).to_socket_addrs().map(|mut it| it.next()) {
        Ok(Some(a)) => a,
        _ => {
            println!("[gửi] Không phân giải được {host}");
            return 1;
        }
    };
    let s = match UdpSocket::bind(if peer.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }) {
        Ok(s) => s,
        Err(e) => {
            println!("[gửi] Không tạo được socket: {e}");
            return 1;
        }
    };
    let t0 = Instant::now();
    let mut est = RttEstimator::new();
    match saw_send(&s, peer, &chunks, &mut est, &mut loss) {
        Ok((sent, retrans)) => {
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            println!("[gửi] Hoàn tất: {sent} datagram đã gửi ({retrans} lần gửi lại), {ms:.0} ms");
            0
        }
        Err(e) => {
            println!("[gửi] Lỗi: {e}");
            1
        }
    }
}
