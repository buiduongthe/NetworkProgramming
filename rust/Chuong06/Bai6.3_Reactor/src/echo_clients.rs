//! Bài 6.3 – Nhiều client echo đồng thời để kiểm tra server reactor.
//!
//! Mỗi client (một luồng) dựa trên Mã giả 3.2 nhưng nhận bằng RECV_EXACT (Mã giả 3.4): gửi một dòng chào,
//! nhận lại đúng số byte đã gửi; sau đó gửi một khối lớn (mặc định 1 MiB) trong một luồng phụ, đồng thời
//! nhận lại đủ khối ở luồng chính và so khớp. Khối lớn hơn MAX_BUF của server nên kiểm tra được việc
//! gửi một phần và áp lực ngược (server ngừng đọc khi bộ đệm gửi đầy). Gửi xong, luồng phụ đóng chiều
//! gửi (half-close) để kiểm tra tập closing: server phải gửi nốt phần còn lại rồi mới đóng.
//!
//! Chạy:  cargo run -- client [host] [port] [n] [size]   (mặc định 127.0.0.1 9000 3 1048576)
use crate::netutil::{int_arg, parse_endpoint, recv_exact, safe_connect, NetError};
use std::io::Write;
use std::net::Shutdown;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn log(msg: &str) {
    println!("{msg}"); // println! in trọn một dòng, các luồng không chen giữa dòng của nhau
}

/// Tạo size byte giả ngẫu nhiên (thuật toán xorshift; thư viện chuẩn Rust không có bộ sinh số ngẫu nhiên).
fn random_bytes(size: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..size)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

fn one_client(i: u64, host: &str, port: u16, size: usize) {
    let mut s = match safe_connect(host, port, 3000) {
        Ok(s) => s,
        Err(e) => return log(&format!("[client {i}] Lỗi: {e}")),
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let result = (|| -> Result<(), NetError> {
        let hello = format!("Xin chào reactor từ client {i}").into_bytes();
        s.write_all(&hello)?;
        let back = recv_exact(&mut s, hello.len())?;
        log(&format!("[client {i}] Nhận lại: '{}'", String::from_utf8_lossy(&back)));
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
        let block = random_bytes(size, nanos ^ i);
        let mut writer = s.try_clone()?; // bản sao socket để gửi song song với nhận
        let to_send = block.clone();
        // gửi xong thì đóng chiều gửi (SHUTDOWN(s, WRITE) như client echo Mã giả 3.2) nhưng vẫn nhận tiếp
        let sender = thread::spawn(move || {
            writer.write_all(&to_send)?;
            writer.shutdown(Shutdown::Write)
        });
        let got = recv_exact(&mut s, size)?;
        sender.join().unwrap()?;
        let ok = if got == block { "có" } else { "không" };
        log(&format!("[client {i}] Gửi {size} byte, nhận lại {} byte, khớp: {ok}", got.len()));
        Ok(())
    })();
    if let Err(e) = result {
        log(&format!("[client {i}] Lỗi: {e}"));
    }
    // s được đóng tự động khi ra khỏi hàm
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "127.0.0.1", 9000);
    let n = int_arg(args, 2, 3);
    let size = int_arg(args, 3, 1048576) as usize;
    println!("[client] Mở {n} kết nối đồng thời, mỗi kết nối gửi thêm {size} byte");
    let t0 = Instant::now();
    let threads: Vec<_> = (1..=n)
        .map(|i| {
            let host = host.clone();
            thread::spawn(move || one_client(i, &host, port, size))
        })
        .collect();
    for th in threads {
        th.join().unwrap();
    }
    println!("[client] Hoàn tất sau {} ms", t0.elapsed().as_millis());
}
