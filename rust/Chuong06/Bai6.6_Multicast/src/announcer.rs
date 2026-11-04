//! Bài 6.6 – Phát thông báo dịch vụ qua multicast (ANNOUNCER).
//!
//! Cài đặt Mã giả 6.8, thủ tục ANNOUNCER (Mục 6.4.3, Giáo trình Lập trình mạng). Kế thừa Mã giả 6.7:
//! thay địa chỉ broadcast bằng địa chỉ nhóm 239.255.42.99 (dải quản trị cục bộ 239.0.0.0/8).
//! Tham số count (bổ sung để minh họa): dừng sau count lần phát; 0 = phát mãi.
//!
//! Chạy:  cargo run -- announcer [service_port] [tên] [period_ms] [count] [mport]
//!        (mặc định 9200 server-chat 5000 0 9998)
use crate::{int_arg, port_arg, random_u64, GROUP, MPORT};
use serde_json::json;
use std::net::UdpSocket;
use std::thread;
use std::time::Duration;

fn announcer(service_port: u16, name: &str, period_ms: u64, count: u64, mport: u16) -> std::io::Result<()> {
    let s = UdpSocket::bind("0.0.0.0:0")?; // SOCKET(INET, DGRAM), cổng tạm
    s.set_multicast_ttl_v4(1)?; // SETOPT(MULTICAST_TTL, 1): chỉ trong mạng con
    s.set_multicast_loop_v4(true)?; // SETOPT(MULTICAST_LOOP, TRUE): nhận cả trên máy gửi
    println!("[announcer] Phát tới nhóm {GROUP}:{mport}, chu kỳ {period_ms} ms");
    let mut n = 0;
    while count == 0 || n < count {
        let a = json!({"type": "announce", "name": name, "port": service_port, "ttl_ms": 3 * period_ms});
        s.send_to(a.to_string().as_bytes(), (GROUP, mport))?; // SENDTO(s, …, (GROUP, MPORT))
        n += 1;
        println!("[announcer] Lần {n}: announce {name}, cổng {service_port}, ttl_ms={}", a["ttl_ms"]);
        let jitter = random_u64() % (period_ms / 10 + 1); // RANDOM(0, period_ms / 10)
        thread::sleep(Duration::from_millis(period_ms + jitter)); // SLEEP: có nhiễu
    }
    println!("[announcer] Dừng phát sau {n} lần");
    Ok(()) // CLOSE(s) khi s ra khỏi phạm vi
}

pub fn run(args: &[String]) {
    let service_port = port_arg(args, 0, 9200);
    let name = args.get(1).map(String::as_str).unwrap_or("server-chat");
    let period_ms = int_arg(args, 2, 5000);
    let count = int_arg(args, 3, 0);
    let mport = port_arg(args, 4, MPORT);
    if let Err(e) = announcer(service_port, name, period_ms, count, mport) {
        println!("[announcer] Lỗi: {e}");
    }
}
