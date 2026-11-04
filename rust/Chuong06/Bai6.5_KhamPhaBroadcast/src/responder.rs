//! Bài 6.5 – Khám phá server chat trong LAN bằng broadcast: phía server (RESPONDER).
//!
//! Cài đặt Mã giả 6.7, thủ tục RESPONDER (Mục 6.4.2, Giáo trình Lập trình mạng): nhận yêu cầu discover,
//! chờ ngẫu nhiên 0–200 ms (giãn phản hồi) rồi trả lời bằng unicast kèm cổng dịch vụ.
//! Trong server thật, RESPONDER chạy như một tác vụ nền (hoặc luồng riêng) bên cạnh vòng chấp nhận kết nối.
//!
//! Chạy:  cargo run -- responder [service_port] [tên] [disc_port]   (mặc định 9200 server-chat 9999)
use crate::{port_arg, random_u64, DISC_PORT};
use serde_json::{json, Value};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::Duration;

fn responder(service_port: u16, name: &str, disc_port: u16) -> std::io::Result<()> {
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?; // SOCKET(INET, DGRAM)
    s.set_reuse_address(true)?; // cho nhiều responder trên một máy
    s.bind(&SocketAddr::from(([0, 0, 0, 0], disc_port)).into())?; // BIND(s, "0.0.0.0", DISC_PORT)
    let s: UdpSocket = s.into(); // dùng tiếp bằng UdpSocket của thư viện chuẩn
    println!("[responder] {name}, cổng dịch vụ {service_port}, chờ discover tại cổng {disc_port}");
    loop {
        let mut buf = [0u8; 2048];
        let (n, addr) = s.recv_from(&mut buf)?; // RECVFROM(s, 2048)
        let Ok(q) = serde_json::from_slice::<Value>(&buf[..n]) else { continue }; // JsonError: sai định dạng, bỏ
        if q["type"] == "discover" && q["service"] == "chat" {
            let delay = random_u64() % 201; // RANDOM(0, 200)
            println!("[responder] discover id={} từ {addr}, trả lời sau {delay} ms", q["id"]);
            thread::sleep(Duration::from_millis(delay)); // DELAY: giãn phản hồi
            let r = json!({"id": q["id"], "port": service_port, "name": name});
            s.send_to(r.to_string().as_bytes(), addr)?; // SENDTO: unicast về địa chỉ người hỏi
        }
    }
}

pub fn run(args: &[String]) {
    let service_port = port_arg(args, 0, 9200);
    let name = args.get(1).map(String::as_str).unwrap_or("server-chat");
    let disc_port = port_arg(args, 2, DISC_PORT);
    if let Err(e) = responder(service_port, name, disc_port) {
        println!("[responder] Lỗi: {e}");
    }
}
