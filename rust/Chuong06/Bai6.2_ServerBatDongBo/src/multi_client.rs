//! Bài 6.2 – Mở nhiều client tra cứu đồng thời để quan sát server bất đồng bộ.
//!
//! Mỗi client chạy trong một luồng, thực hiện LOOKUP_CLIENT (Mã giả 3.8) rút gọn: gửi một yêu cầu
//! lookup "TCP", in phản hồi kèm thời điểm t tính từ lúc chương trình bắt đầu, giữ kết nối hold_ms
//! (mô phỏng người dùng chưa rời đi) rồi đóng kết nối. Không gửi bye vì SERVE_ASYNC (Mã giả 6.3)
//! không có thao tác này. Các client khởi động cách nhau 100 ms.
//!
//! Chạy:  cargo run -- client [host] [port] [n] [hold_ms]   (mặc định 127.0.0.1 9000 5 1000)
use crate::netutil::{int_arg, parse_endpoint, recv_json, safe_connect, send_json, NetError};
use crate::log;
use serde_json::json;
use std::io::ErrorKind;
use std::thread;
use std::time::{Duration, Instant};

fn one_client(i: u64, host: &str, port: u16, hold_ms: u64, t0: Instant) {
    let t = || t0.elapsed().as_millis();
    let mut s = match safe_connect(host, port, 3000) {
        Ok(s) => s,
        Err(e) => {
            log(&format!("[client {i}] Lỗi: {e}"));
            return;
        }
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(5000))); // RECV_TIMEOUT 5000 ms
    let result = (|| -> Result<(), NetError> {
        send_json(&mut s, &json!({"op": "lookup", "term": "TCP"}))?;
        let resp = recv_json(&mut s)?;
        if resp["status"] != 200 {
            let err = resp["error"].as_str().unwrap_or("");
            log(&format!("[client {i}] t = {} ms: bị từ chối {} – {err}", t(), resp["status"]));
            return Ok(());
        }
        let term = resp["term"].as_str().unwrap_or("");
        let text = resp["text"].as_str().unwrap_or("");
        log(&format!("[client {i}] t = {} ms: 200 {term} – {text}", t()));
        thread::sleep(Duration::from_millis(hold_ms)); // giữ kết nối rồi đóng
        Ok(())
    })();
    match result {
        Ok(()) => {}
        // server đóng ngay sau ACCEPT: thấy EOF hoặc RST
        Err(NetError::ConnectionClosed(_)) => log(&format!("[client {i}] t = {} ms: server đóng kết nối ngay", t())),
        Err(NetError::Io(e)) if e.kind() == ErrorKind::ConnectionReset => {
            log(&format!("[client {i}] t = {} ms: server đóng kết nối ngay", t()))
        }
        Err(e) => log(&format!("[client {i}] Lỗi: {e}")),
    }
    // s được đóng tự động khi ra khỏi hàm
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "127.0.0.1", 9000);
    let n = int_arg(args, 2, 5);
    let hold_ms = int_arg(args, 3, 1000);
    println!("[client] {n} client cách nhau 100 ms, giữ kết nối {hold_ms} ms");
    let t0 = Instant::now();
    let mut threads = Vec::new();
    for i in 1..=n {
        let host = host.clone();
        threads.push(thread::spawn(move || one_client(i, &host, port, hold_ms, t0)));
        thread::sleep(Duration::from_millis(100));
    }
    for th in threads {
        th.join().unwrap();
    }
    println!("[client] Hoàn tất sau {} ms", t0.elapsed().as_millis());
}
