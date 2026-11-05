//! Client thử: gửi dữ liệu, đóng chiều gửi, đọc phản hồi tới EOF.

use crate::{loi, port_arg};
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::process;
use std::time::Duration;

pub fn run(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Cách dùng: client <host> <port> <chuỗi> [số_lần_lặp]");
        process::exit(2);
    }
    let (host, port, text) = (&args[0], port_arg(&args[1]), &args[2]);
    let repeat: usize = args.get(3).and_then(|r| r.parse().ok()).unwrap_or(1);
    let data = text.repeat(repeat).into_bytes();
    let addr = (host.as_str(), port).to_socket_addrs().ok().and_then(|mut a| a.next());
    let mut s = match addr.map(|a| TcpStream::connect_timeout(&a, Duration::from_secs(5))) {
        Some(Ok(s)) => s,
        Some(Err(e)) => {
            println!("[client] Không kết nối được {host}:{port}: {}", loi(&e));
            process::exit(1)
        }
        None => {
            println!("[client] Không phân giải được {host}");
            process::exit(1)
        }
    };
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut reply = Vec::new();
    let r: io::Result<()> = (|| {
        s.write_all(&data)?;
        s.shutdown(Shutdown::Write)?; // báo hết dữ liệu: server mới bắt đầu trả lời
        s.read_to_end(&mut reply)?; // đọc tới EOF: phía bên kia đã đóng chiều gửi
        Ok(())
    })();
    match r {
        Err(e) if e.kind() == io::ErrorKind::ConnectionReset => {
            // bên kia đóng khi còn dữ liệu chưa đọc -> RST
            println!("[client] Kết nối bị đặt lại (RST) trước khi có phản hồi");
        }
        Err(e) => println!("[client] Lỗi: {}", loi(&e)),
        Ok(()) => {}
    }
    println!("[client] Gửi {} byte, nhận {} byte", data.len(), reply.len());
    if reply.is_empty() {
        println!("[client] Kết nối bị đóng mà không có dữ liệu trả về");
        process::exit(1);
    }
    let preview: String = String::from_utf8_lossy(&reply).chars().take(40).collect();
    let ok = reply == text.to_uppercase().repeat(repeat).into_bytes();
    let more = if reply.len() > 40 { "..." } else { "" };
    println!("[client] Phản hồi: '{preview}'{more}");
    println!("[client] So khớp với bản chữ hoa: {}", if ok { "KHỚP" } else { "SAI" });
}
