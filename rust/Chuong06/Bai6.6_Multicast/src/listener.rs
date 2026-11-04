//! Bài 6.6 – Nhận thông báo dịch vụ qua multicast (LISTENER).
//!
//! Cài đặt Mã giả 6.8, thủ tục LISTENER (Mục 6.4.3, Giáo trình Lập trình mạng): bật REUSEADDR trước
//! BIND để nhiều chương trình trên cùng máy cùng nghe một cổng, tham gia nhóm (IGMP join), cập nhật
//! bộ nhớ đệm cache và xóa dịch vụ hết hạn. Tham số run_ms (bổ sung để minh họa): nghe trong run_ms
//! rồi in bộ nhớ đệm và kết thúc; 0 = nghe mãi.
//!
//! Chạy:  cargo run -- listener [run_ms] [mport]      (mặc định 0 9998)
use crate::{int_arg, port_arg, GROUP, MPORT};
use serde_json::Value;
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// Bộ nhớ đệm: tên dịch vụ → (địa chỉ IP, cổng dịch vụ, thời điểm hết hạn).
type Cache = BTreeMap<String, (String, u64, Instant)>;

/// REMOVE_EXPIRED: xóa dịch vụ đã quá hạn ttl_ms.
fn remove_expired(cache: &mut Cache, now: Instant) {
    let expired: Vec<String> = cache.iter().filter(|(_, (_, _, exp))| *exp < now).map(|(n, _)| n.clone()).collect();
    for name in expired {
        cache.remove(&name);
        println!("[listener] Hết hạn, xóa: {name}");
    }
}

fn listener(cache: &mut Cache, run_ms: u64, mport: u16) -> std::io::Result<()> {
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?; // SOCKET(INET, DGRAM)
    s.set_reuse_address(true)?; // SETOPT(REUSEADDR, TRUE): nhiều tiến trình cùng nghe
    s.bind(&SocketAddr::from(([0, 0, 0, 0], mport)).into())?; // BIND(s, "0.0.0.0", MPORT)
    let s: UdpSocket = s.into();
    s.join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)?; // SETOPT(ADD_MEMBERSHIP, (GROUP, ANY_INTERFACE)): IGMP join
    println!("[listener] Tham gia nhóm {GROUP}, nghe cổng {mport}");
    let end = Instant::now() + Duration::from_millis(run_ms);
    while run_ms == 0 || Instant::now() < end {
        if run_ms > 0 {
            let left = end.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
            s.set_read_timeout(Some(left))?;
        }
        let mut buf = [0u8; 2048];
        let (n, addr) = match s.recv_from(&mut buf) {
            Ok(x) => x, // RECVFROM(s, 2048)
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => break, // hết run_ms
            Err(e) => return Err(e),
        };
        let Ok(a) = serde_json::from_slice::<Value>(&buf[..n]) else { continue }; // JsonError: sai định dạng, bỏ
        if let (Some("announce"), Some(name), Some(port), Some(ttl)) =
            (a["type"].as_str(), a["name"].as_str(), a["port"].as_u64(), a["ttl_ms"].as_u64())
        {
            if !cache.contains_key(name) {
                println!("[listener] Thêm {name} tại {}:{port}", addr.ip());
            }
            // cache[a.name] ← (addr.ip, a.port, NOW() + a.ttl_ms)
            cache.insert(name.to_string(), (addr.ip().to_string(), port, Instant::now() + Duration::from_millis(ttl)));
        }
        remove_expired(cache, Instant::now()); // dịch vụ hết hạn: xóa
    }
    Ok(()) // CLOSE(s) khi s ra khỏi phạm vi
}

pub fn run(args: &[String]) {
    let run_ms = int_arg(args, 0, 0);
    let mport = port_arg(args, 1, MPORT);
    let mut cache = Cache::new();
    if let Err(e) = listener(&mut cache, run_ms, mport) {
        println!("[listener] Lỗi: {e}");
    }
    let items: Vec<String> = cache.iter().map(|(n, (ip, p, _))| format!("{n} tại {ip}:{p}")).collect();
    let items = if items.is_empty() { "(trống)".to_string() } else { items.join(", ") };
    println!("[listener] Bộ nhớ đệm: {items}");
}
