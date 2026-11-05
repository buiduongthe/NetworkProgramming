//! Client thử cho server tắt an toàn.
//! Gửi lần lượt các yêu cầu "LAM <ms>", in phản hồi; sau đó tiếp tục giữ kết nối và in mọi
//! dòng server gửi (BYE, HUY) cho tới khi server đóng kết nối.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

pub fn run(args: &[String]) {
    let (name, host) = (&args[0], &args[1]);
    let port: u16 = args[2].parse().unwrap_or(9000);
    let tag = format!("[khach {name}]");
    let addr = (host.as_str(), port).to_socket_addrs().ok().and_then(|mut a| a.next());
    let s = match addr.map(|a| TcpStream::connect_timeout(&a, Duration::from_secs(3))) {
        Some(Ok(s)) => s,
        Some(Err(e)) => {
            // in gọn như e.strerror của Python: bỏ phần " (os error N)"
            let msg = e.to_string();
            println!("{tag} Không kết nối được: {}", msg.split(" (os error").next().unwrap_or(&msg));
            std::process::exit(1)
        }
        None => {
            println!("{tag} Không phân giải được {host}");
            std::process::exit(1)
        }
    };
    s.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let mut w = s.try_clone().expect("try_clone");
    let mut lines = BufReader::new(s).lines();
    let t0 = Instant::now();
    let t = || t0.elapsed().as_secs_f64();
    for ms in &args[3..] {
        if w.write_all(format!("LAM {ms}\n").as_bytes()).is_err() {
            break;
        }
        println!("{tag} Gửi LAM {ms}");
        match lines.next() {
            Some(Ok(reply)) => println!("{tag} t={:.1}s nhận: {}", t(), reply.trim()),
            _ => {
                println!("{tag} Server đóng kết nối khi chưa trả lời");
                return;
            }
        }
    }
    for line in lines.map_while(Result::ok) {
        // giữ kết nối, chờ thông báo của server
        println!("{tag} t={:.1}s nhận: {}", t(), line.trim());
    }
    println!("{tag} t={:.1}s server đóng kết nối", t());
}
