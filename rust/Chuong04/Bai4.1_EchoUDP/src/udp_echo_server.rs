//! UDP Echo Server – Mã giả 4.1 (Mục 4.2.1, Giáo trình Lập trình mạng).
//! So với TCP Echo Server (Mã giả 3.1): không có LISTEN/ACCEPT, một socket phục vụ mọi client;
//! mỗi lần RECVFROM nhận đúng một datagram nguyên vẹn kèm địa chỉ người gửi.

use crate::endpoint::parse_endpoint;
use std::io::ErrorKind;
use std::net::UdpSocket;
use std::process;

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9000); // Mã giả 4.1, dòng 2
    // dòng 3–4: SOCKET(INET, DGRAM) + BIND – UdpSocket::bind làm cả hai bước
    let s = match UdpSocket::bind((host.as_str(), port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[server] Không mở được {host}:{port}: {e}");
            process::exit(1);
        }
    };
    println!("[server] Đang lắng nghe UDP tại {host}:{port}");
    let mut buf = vec![0u8; 65535];
    loop {
        // dòng 5: LOOP
        let (n, addr) = match s.recv_from(&mut buf) {
            // dòng 6: datagram nguyên vẹn
            Ok(v) => v,
            Err(e) if e.kind() == ErrorKind::ConnectionReset => continue, // Windows: ICMP port unreachable (Mục 4.2.2)
            Err(e) => {
                println!("[server] Lỗi nhận: {e}");
                continue;
            }
        };
        println!("[server] Nhận {n} byte từ {addr}"); // dòng 7
        if let Err(e) = s.send_to(&buf[..n], addr) {
            // dòng 8: trả lời đúng người gửi
            println!("[server] Lỗi gửi tới {addr}: {e}");
        }
    }
}
