//! UDP Echo Client có thời gian chờ – Mã giả 4.2 (Mục 4.2.1, Giáo trình Lập trình mạng).
//! Đọc từng dòng từ bàn phím, gửi một datagram tới server và chờ phản hồi tối đa 2 giây;
//! hết thời gian chờ thì báo datagram có thể đã mất (không có kết nối nên không có lỗi nào khác).

use crate::endpoint::parse_endpoint;
use std::io::{self, BufRead, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::process;
use std::time::Duration;

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "127.0.0.1", 9000); // Mã giả 4.2, dòng 2
    // dòng 3: server ← FIRST(RESOLVE(host, port))
    let server: SocketAddr = match (host.as_str(), port).to_socket_addrs().map(|mut it| it.next()) {
        Ok(Some(a)) => a,
        _ => {
            eprintln!("[client] Không phân giải được {host}");
            process::exit(1);
        }
    };
    // dòng 4: SOCKET(FAMILY(server), DGRAM) – gắn vào cổng tạm bất kỳ cùng họ địa chỉ
    let local = if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
    let s = UdpSocket::bind(local).unwrap_or_else(|e| {
        eprintln!("[client] Không tạo được socket: {e}");
        process::exit(1);
    });
    s.set_read_timeout(Some(Duration::from_millis(2000))).ok(); // dòng 5: RECV_TIMEOUT 2000 ms
    println!("[client] Gửi tới {host}:{port} qua UDP. Nhập dòng cần gửi (Ctrl+Z/Ctrl+D để kết thúc).");
    let mut buf = vec![0u8; 65535];
    for line in io::stdin().lock().lines() {
        // dòng 6: READ_CONSOLE()
        let Ok(line) = line else { break };
        if let Err(e) = s.send_to(line.as_bytes(), server) {
            // dòng 7: SENDTO
            println!("[client] Lỗi gửi: {e}");
            continue;
        }
        match s.recv_from(&mut buf) {
            // dòng 9: RECVFROM
            Ok((n, _addr)) => {
                let text = String::from_utf8_lossy(&buf[..n]);
                println!("[client] Gửi: '{line}' -> Nhận: '{text}'");
            }
            // dòng 11: CATCH Timeout – trên Linux là WouldBlock, trên Windows là TimedOut;
            // ConnectionReset: Windows báo ICMP port unreachable (Mục 4.2.2)
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::ConnectionReset) => {
                println!("[client] Gửi: '{line}' -> Không có phản hồi: datagram có thể đã mất");
            }
            Err(e) => println!("[client] Lỗi nhận: {e}"),
        }
    }
    drop(s); // dòng 15: CLOSE
    println!("[client] Đã đóng socket");
}
