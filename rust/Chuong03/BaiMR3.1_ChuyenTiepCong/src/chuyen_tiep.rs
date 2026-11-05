//! Bộ chuyển tiếp: mỗi client nối vào cổng nghe được ghép với một kết nối mới tới máy đích;
//! hai luồng bơm dữ liệu theo hai chiều. Khi một chiều gặp EOF, bộ chuyển tiếp gọi
//! SHUTDOWN(WRITE) trên socket bên kia để báo "hết dữ liệu" (chuyển tiếp gói FIN), chứ không
//! đóng hẳn, vì chiều ngược lại có thể vẫn đang truyền.
//! Đây là nguyên lý của "ssh -L", proxy TCP và cân bằng tải lớp 4.

use crate::{loi, port_arg};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

static COUNTER: AtomicUsize = AtomicUsize::new(1);

/// Chép dữ liệu src -> dst tới khi src hết dữ liệu, rồi báo EOF cho dst. Trả về số byte đã chép.
fn pump(mut src: TcpStream, mut dst: TcpStream, label: String) -> usize {
    let mut total = 0;
    let mut buf = vec![0u8; 65536];
    let r: io::Result<()> = (|| loop {
        let n = src.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        dst.write_all(&buf[..n])?; // SEND_ALL
        total += n;
    })();
    if let Err(e) = r {
        println!("[chuyen-tiep] {label}: lỗi {}", loi(&e));
    }
    let _ = dst.shutdown(Shutdown::Write); // chuyển tiếp FIN, không đóng hẳn socket
    total
}

/// Kết nối tới đích với thời gian chờ 3 giây (thử lần lượt các địa chỉ phân giải được).
fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "không có địa chỉ nào");
    for a in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&a, Duration::from_secs(3)) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn serve(client: TcpStream, addr: SocketAddr, thost: String, tport: u16) {
    let cid = COUNTER.fetch_add(1, Ordering::SeqCst);
    let upstream = match connect(&thost, tport) {
        Ok(s) => s,
        Err(e) => {
            println!("[chuyen-tiep] #{cid} {addr}: không nối được đích {thost}:{tport} ({})", loi(&e));
            return; // client bị đóng khi ra khỏi hàm: nhận EOF (hoặc RST) ngay
        }
    };
    println!("[chuyen-tiep] #{cid} {addr} <-> đích {thost}:{tport}");
    // mỗi chiều một luồng; try_clone tạo thêm một "tay cầm" tới cùng socket
    let (c2, u2) = (client.try_clone().unwrap(), upstream.try_clone().unwrap());
    let t1 = thread::spawn(move || pump(c2, u2, format!("#{cid} lên")));
    let (c3, u3) = (client.try_clone().unwrap(), upstream.try_clone().unwrap());
    let t2 = thread::spawn(move || pump(u3, c3, format!("#{cid} xuống")));
    let up = t1.join().unwrap(); // chờ cả hai chiều kết thúc
    let down = t2.join().unwrap();
    drop(client);
    drop(upstream);
    println!("[chuyen-tiep] #{cid} xong: lên {up} byte, xuống {down} byte");
}

pub fn run(args: &[String]) {
    if args.len() < 4 {
        eprintln!("Cách dùng: chuyentiep <host_nghe> <cổng_nghe> <host_đích> <cổng_đích>");
        process::exit(2);
    }
    let (lhost, lport, thost, tport) = (args[0].clone(), port_arg(&args[1]), args[2].clone(), port_arg(&args[3]));
    let srv = TcpListener::bind((lhost.as_str(), lport)).unwrap_or_else(|e| {
        eprintln!("[chuyen-tiep] Không mở được {lhost}:{lport}: {}", loi(&e));
        process::exit(1)
    });
    ctrlc::set_handler(|| {
        println!("[chuyen-tiep] Dừng");
        process::exit(0);
    })
    .expect("Không cài được hàm bắt Ctrl+C");
    println!("[chuyen-tiep] {lhost}:{lport} -> {thost}:{tport}");
    for conn in srv.incoming() {
        match conn {
            Ok(c) => {
                let addr = c.peer_addr().unwrap();
                let th = thost.clone();
                thread::spawn(move || serve(c, addr, th, tport));
            }
            Err(e) => println!("[chuyen-tiep] Lỗi accept: {}", loi(&e)),
        }
    }
}
