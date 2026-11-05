//! Máy chủ thử cục bộ cho ứng dụng giao diện đa tác vụ (chỉ chạy trong máy). Ba dịch vụ giả lập:
//!   - HTTP  <port>/tcp : GET /tep?kb=<số KB>&tre=<ms> – gửi tệp theo khối 16 KB, nghỉ <tre> ms mỗi khối;
//!   - "DNS" <port>/udp : nhận "A <tên>", trả "<tên> <địa chỉ>" hoặc "<tên> NXDOMAIN" sau 100–400 ms
//!     (bảng tên giả lập, không phải giao thức DNS thật – xem Bài 4.3);
//!   - cổng mở <port+1>, <port+2>/tcp: chỉ nhận rồi đóng kết nối, để ứng dụng kiểm tra cổng.

use crate::http::{read_request, respond};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

const CHUNK: usize = 16 * 1024;

fn slog(text: &str) {
    println!("[server] {text}");
}

fn fake_name(name: &str) -> (&'static str, u64) {
    match name {
        "router.lab" => ("10.10.0.1", 100),
        "may-in.lab" => ("10.10.0.21", 250),
        "camera-1.lab" => ("10.10.0.31", 400),
        "nas.lab" => ("10.10.0.40", 150),
        _ => ("NXDOMAIN", 300),
    }
}

async fn file_handler(conn: TcpStream) {
    let (r, mut w) = conn.into_split();
    let mut r = BufReader::new(r);
    let Ok(req) = read_request(&mut r).await else { return };
    if req.method != "GET" || req.path != "/tep" {
        let _ = respond(&mut w, "404 Not Found", "text/plain; charset=utf-8", b"khong co").await;
        return;
    }
    let kb = match req.num("kb") {
        k if k > 0 => k as usize,
        _ => 256,
    };
    let delay = req.num("tre").max(0) as u64;
    let size = kb * 1024;
    slog(&format!("HTTP GET /tep {kb} KB, nghỉ {delay} ms mỗi khối"));
    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n");
    if w.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    let block = vec![b'x'; CHUNK];
    let mut sent = 0;
    let mut probe = [0u8; 1];
    while sent < size {
        let n = CHUNK.min(size - sent);
        if w.write_all(&block[..n]).await.is_err() {
            slog(&format!("HTTP: client đóng kết nối giữa chừng ({}/{kb} KB)", sent / 1024));
            return;
        }
        sent += n;
        if sent >= size {
            break; // khối cuối: không cần nghỉ
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(delay)) => {}
            // client đóng kết nối (hủy) trong lúc nghỉ: read trả 0 hoặc lỗi -> dừng sớm
            _ = r.read(&mut probe) => {
                slog(&format!("HTTP: client đóng kết nối giữa chừng ({}/{kb} KB)", sent / 1024));
                return;
            }
        }
    }
}

async fn dns_server(sock: Arc<UdpSocket>) {
    let mut buf = [0u8; 512];
    loop {
        let Ok((n, addr)) = sock.recv_from(&mut buf).await else { return };
        let text = String::from_utf8_lossy(&buf[..n]).to_string();
        let name = text.strip_prefix("A ").unwrap_or(&text).trim().to_string();
        let sock = Arc::clone(&sock);
        tokio::spawn(async move {
            let (ip, ms) = fake_name(&name);
            tokio::time::sleep(Duration::from_millis(ms)).await;
            let _ = sock.send_to(format!("{name} {ip}").as_bytes(), addr).await;
        });
    }
}

#[tokio::main]
pub async fn run(host: &str, port: u16) {
    let fail = |e: std::io::Error| -> ! {
        slog(&format!("lỗi: {e}"));
        std::process::exit(1)
    };
    let udp = Arc::new(UdpSocket::bind((host, port)).await.unwrap_or_else(|e| fail(e)));
    tokio::spawn(dns_server(udp));
    for p in [port + 1, port + 2] {
        let ln = TcpListener::bind((host, p)).await.unwrap_or_else(|e| fail(e));
        tokio::spawn(async move {
            while let Ok((c, _)) = ln.accept().await {
                drop(c); // cổng mở: nhận rồi đóng ngay
            }
        });
    }
    let ln = TcpListener::bind((host, port)).await.unwrap_or_else(|e| fail(e));
    slog(&format!("HTTP {host}:{port}/tcp, \"DNS\" {host}:{port}/udp, cổng mở {}, {}", port + 1, port + 2));
    loop {
        tokio::select! {
            r = ln.accept() => if let Ok((c, _)) = r { tokio::spawn(file_handler(c)); },
            _ = tokio::signal::ctrl_c() => { slog("dừng"); return; }
        }
    }
}
