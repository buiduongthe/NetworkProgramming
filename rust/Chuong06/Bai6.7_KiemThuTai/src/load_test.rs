//! Bài 6.7 – Bộ sinh tải vòng kín đo thông lượng và phân vị độ trễ p50, p95, p99.
//!
//! Cài đặt Mã giả 6.9 (Mục 6.5.1, Giáo trình Lập trình mạng). Kế thừa Mã giả 5.2 (khởi động N tác vụ
//! rồi WHEN_ALL) và các hàm khung bất đồng bộ của Mã giả 5.1 (async_frame.rs).
//! Chạy lần lượt các mức tải (số client) trong danh sách levels và in bảng kết quả.
//!
//! Chạy:  cargo run -- load [host] [port] [n_requests] [levels]   (mặc định 127.0.0.1 9000 20 1,10,100)
use crate::async_frame::{recv_json_async, send_json_async};
use crate::netutil::{exit_with, int_arg, parse_endpoint};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

/// PERCENTILE theo hạng gần nhất: giá trị mà p% số mẫu không vượt quá.
fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut s = values.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let rank = (p / 100.0 * s.len() as f64).ceil() as usize;
    s[rank.max(1) - 1]
}

/// Kết quả một mức tải.
struct Report {
    ok: usize,
    errors: usize,
    rps: f64,
    p50: f64,
    p95: f64,
    p99: f64,
}

/// ONE_CLIENT: một client ảo gửi n_requests yêu cầu nối tiếp (vòng kín) trên một kết nối.
async fn one_client(host: String, port: u16, n_requests: u64, lat: Arc<Mutex<Vec<f64>>>, errors: Arc<AtomicUsize>) {
    // SAFE_CONNECT_ASYNC(host, port, 3000)
    let mut s = match tokio::time::timeout(Duration::from_secs(3), TcpStream::connect((host.as_str(), port))).await {
        Ok(Ok(s)) => s,
        _ => {
            errors.fetch_add(1, Ordering::SeqCst);
            return;
        }
    };
    for _ in 0..n_requests {
        let t0 = Instant::now(); // NOW_PRECISE()
        let resp = match send_json_async(&mut s, &json!({"op": "lookup", "term": "TCP"})).await {
            Ok(()) => recv_json_async(&mut s).await,
            Err(e) => Err(e),
        };
        match resp {
            Ok(r) if r["status"] == 200 => {
                let ms = t0.elapsed().as_secs_f64() * 1000.0;
                lat.lock().unwrap().push(ms); // APPEND(lat, NOW_PRECISE() - t0)
            }
            // r.status ≠ 200 (ví dụ 503: server bận, server đóng kết nối ngay sau đó) hoặc
            // NetError/ConnectionClosed/ProtocolError: INCREMENT(errors) rồi dừng client này
            _ => {
                errors.fetch_add(1, Ordering::SeqCst);
                return;
            }
        }
    }
    // CLOSE(s) khi s ra khỏi phạm vi
}

/// LOAD_TEST(host, port, n_clients, n_requests)
async fn load_test(host: &str, port: u16, n_clients: u64, n_requests: u64) -> Report {
    let lat = Arc::new(Mutex::new(Vec::new())); // CONCURRENT_LIST()
    let errors = Arc::new(AtomicUsize::new(0)); // errors ← ATOMIC_COUNTER(0): số client gặp lỗi
    let t = Instant::now();
    let tasks: Vec<_> = (0..n_clients)
        .map(|_| tokio::spawn(one_client(host.to_string(), port, n_requests, Arc::clone(&lat), Arc::clone(&errors))))
        .collect();
    for task in tasks {
        task.await.unwrap(); // WHEN_ALL: chờ mọi tác vụ
    }
    let t = t.elapsed().as_secs_f64();
    let lat = lat.lock().unwrap();
    Report {
        ok: lat.len(),
        errors: errors.load(Ordering::SeqCst),
        rps: lat.len() as f64 / t,
        p50: percentile(&lat, 50.0),
        p95: percentile(&lat, 95.0),
        p99: percentile(&lat, 99.0),
    }
}

pub async fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "127.0.0.1", 9000);
    let n_requests = int_arg(args, 2, 20);
    let levels_arg = args.get(3).map(String::as_str).unwrap_or("1,10,100");
    let levels: Vec<u64> = levels_arg
        .split(',')
        .map(|x| x.trim().parse().unwrap_or_else(|_| exit_with(&format!("Mức tải không hợp lệ: {x}"))))
        .collect();
    println!("[load] Server {host}:{port}, {n_requests} yêu cầu/client, vòng kín");
    println!("[load] {:>6} {:>6} {:>5} {:>8} {:>7} {:>7} {:>7}", "client", "OK", "lỗi", "yc/giây", "p50", "p95", "p99");
    for n in levels {
        let r = load_test(&host, port, n, n_requests).await;
        println!(
            "[load] {n:>6} {:>6} {:>5} {:>8.1} {:>7.1} {:>7.1} {:>7.1}",
            r.ok, r.errors, r.rps, r.p50, r.p95, r.p99
        );
    }
    println!("[load] OK: số yêu cầu thành công; lỗi: số client bị từ chối");
    println!("[load] p50, p95, p99: phân vị độ trễ, tính bằng ms");
}
