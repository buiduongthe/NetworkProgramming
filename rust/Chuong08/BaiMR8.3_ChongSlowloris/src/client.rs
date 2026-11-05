//! Client thí nghiệm (vai "slow", chỉ loopback) và vai "probe" (yêu cầu bình thường).

use socket2::{Domain, Protocol, Socket, Type};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::process;
use std::time::{Duration, Instant};

/// host có phải địa chỉ loopback (127.0.0.0/8, ::1) hay không – dùng cho vai 'slow'.
fn la_loopback(host: &str) -> bool {
    match (host, 0).to_socket_addrs() {
        Ok(addrs) => {
            let v: Vec<_> = addrs.collect();
            !v.is_empty() && v.iter().all(|sa| sa.ip().is_loopback())
        }
        Err(_) => false,
    }
}

/// Mở n kết nối và nhỏ giọt từng byte tiêu đề, KHÔNG bao giờ kết thúc phần đầu.
/// Chỉ chạy được khi host là loopback của chính máy bạn. Giữ kết nối khoảng 9 giây rồi thoát.
/// Dùng địa chỉ nguồn loopback riêng (mặc định 127.0.0.2) để server phân biệt với probe.
pub fn chay_slow(host: &str, port: &str, n: usize, nguon: &str) {
    if !la_loopback(host) {
        eprintln!("[slow] TỪ CHỐI: đích {host} không phải loopback. Vai này chỉ dùng cho 127.0.0.1 trong phòng thí nghiệm của chính bạn.");
        process::exit(1);
    }
    let nguon_ip: IpAddr = match nguon.parse() {
        Ok(ip) if IpAddr::is_loopback(&ip) => ip,
        _ => {
            eprintln!("[slow] TỪ CHỐI: địa chỉ nguồn {nguon} không phải loopback.");
            process::exit(1);
        }
    };
    println!("[slow] (CHỈ THÍ NGHIỆM) mở {n} kết nối nhỏ giọt tới {host}:{port} từ nguồn {nguon} trong ~9s");
    let dich: SocketAddr = format!("{host}:{port}").to_socket_addrs().unwrap().next().unwrap();
    let mut socks: Vec<TcpStream> = Vec::new();
    for _ in 0..n {
        // Gắn địa chỉ nguồn loopback trước khi connect (thư viện chuẩn chưa hỗ trợ -> dùng socket2).
        if let Ok(mut s) = ket_noi_tu_nguon(&dich, nguon_ip) {
            if s.write_all(b"GET /?slow HTTP/1.1\r\nHost: localhost\r\n").is_ok() {
                // phần đầu CHƯA kết thúc (thiếu dòng trống)
                socks.push(s);
            }
        }
    }
    println!("[slow] đã mở được {} kết nối; bắt đầu nhỏ giọt mỗi ~7s một byte", socks.len());
    // SIGINT (từ tools/demo.py) -> in dòng đóng rồi thoát, giống except KeyboardInterrupt của Python.
    ctrlc::set_handler(|| {
        println!("[slow] đóng toàn bộ kết nối thí nghiệm");
        process::exit(0);
    })
    .expect("đặt bộ xử lý Ctrl+C");
    let het_han = Instant::now() + Duration::from_secs(9);
    while Instant::now() < het_han {
        std::thread::sleep(Duration::from_secs(7));
        let mut song = 0;
        socks.retain_mut(|s| {
            if s.write_all(b"X").is_ok() {
                // một byte rác, giữ kết nối "còn sống"
                song += 1;
                true
            } else {
                false
            }
        });
        println!("[slow] còn {song} kết nối đang giữ");
    }
    drop(socks);
    println!("[slow] đóng toàn bộ kết nối thí nghiệm");
}

fn ket_noi_tu_nguon(dich: &SocketAddr, nguon: IpAddr) -> std::io::Result<TcpStream> {
    let dom = if dich.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
    let s = Socket::new(dom, Type::STREAM, Some(Protocol::TCP))?;
    s.bind(&SocketAddr::new(nguon, 0).into())?;
    s.connect_timeout(&(*dich).into(), Duration::from_secs(2))?;
    Ok(s.into())
}

/// Gửi một yêu cầu HTTP/1.1 bình thường, đo xem có được phục vụ không (hạn 4s).
pub fn chay_probe(host: &str, port: &str) {
    let t0 = Instant::now();
    let dich: SocketAddr = match format!("{host}:{port}").to_socket_addrs().ok().and_then(|mut a| a.next()) {
        Some(a) => a,
        None => {
            println!("[probe] THẤT BẠI: không phân giải được {host}:{port}");
            return;
        }
    };
    let mut s = match TcpStream::connect_timeout(&dich, Duration::from_secs(4)) {
        Ok(s) => s,
        Err(e) => {
            println!("[probe] THẤT BẠI: {}", loi(&e));
            return;
        }
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(4)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(4)));
    if s.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").is_err() {
        bao_that_bai(t0);
        return;
    }
    let mut data = Vec::new();
    let mut buf = [0u8; 1024];
    while !data.windows(4).any(|w| w == b"\r\n\r\n") {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => data.extend_from_slice(&buf[..n]),
        }
    }
    let dt = t0.elapsed().as_secs_f64() * 1000.0;
    if data.is_empty() {
        bao_that_bai(t0);
        return;
    }
    let dong_dau: String = data.split(|&b| b == b'\n').next().unwrap_or(&[]).iter().filter(|&&b| b != b'\r').map(|&b| b as char).collect();
    if data.starts_with(b"HTTP/1.1 200") {
        println!("[probe] THÀNH CÔNG sau {dt:.0} ms: {dong_dau}");
    } else {
        println!("[probe] phản hồi khác: {dong_dau} ({dt:.0} ms)");
    }
}

fn bao_that_bai(t0: Instant) {
    let dt = t0.elapsed().as_secs_f64() * 1000.0;
    println!("[probe] THẤT BẠI: hết thời gian chờ sau {dt:.0} ms – server không phục vụ kịp (bị tấn công làm nghẽn).");
}

/// Bỏ đuôi " (os error N)" cho thông báo giống bản Python/Go.
fn loi(e: &std::io::Error) -> String {
    let s = e.to_string();
    s.split(" (os error").next().unwrap_or(&s).to_string()
}
