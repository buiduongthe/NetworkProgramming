//! Kiểm tra nhiều cổng đồng thời có giới hạn (tokio).
//!
//! Cài đặt Mã giả 5.2 (Mục 5.2.5, Giáo trình Lập trình mạng): SCAN_PORTS, CHECK_ONE.
//! Kế thừa Mã giả 1.2 (PORT_CHECK): PORT_CHECK_ASYNC có cùng thân hàm, CONNECT được thay bằng
//! AWAIT CONNECT_ASYNC. Chương trình quét cùng một dải cổng hai lần: tuần tự (max_parallel = 1)
//! và đồng thời (max_parallel = k), rồi in thời gian của mỗi lần.
//! Chỉ chạy trên máy thuộc quyền quản lý hoặc mạng thí nghiệm cô lập (Mục 1.2.5).

use std::io::ErrorKind;
use std::process;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio::time::{timeout, Duration, Instant};

/// Mã giả 1.2 + AWAIT: PORT_CHECK_ASYNC(host, port, timeout_ms).
async fn port_check_async(host: &str, port: u16, timeout_ms: u64) -> &'static str {
    let addrs: Vec<_> = match tokio::net::lookup_host((host, port)).await {
        // RESOLVE_HOST
        Ok(it) => it.collect(),
        Err(_) => return "UNRESOLVED",
    };
    let mut state = "FILTERED"; // chưa địa chỉ nào phản hồi
    for addr in addrs {
        // AWAIT CONNECT_ASYNC với CONNECT_TIMEOUT
        match timeout(Duration::from_millis(timeout_ms), TcpStream::connect(addr)).await {
            Ok(Ok(_s)) => return "OPEN", // bắt tay ba bước thành công; _s tự đóng (FINALLY CLOSE)
            Ok(Err(e)) if e.kind() == ErrorKind::ConnectionRefused => state = "CLOSED", // nhận gói RST
            _ => continue, // hết giờ hoặc không có đường đi: thử địa chỉ tiếp theo
        }
    }
    state
}

/// Mã giả 5.2, dòng 9–16: CHECK_ONE(host, p, gate).
async fn check_one(host: Arc<String>, p: u16, gate: Arc<Semaphore>) -> (u16, &'static str) {
    // AWAIT ACQUIRE_ASYNC(gate); lượt được trả lại (RELEASE) khi `_permit` ra khỏi phạm vi,
    // kể cả khi có lỗi – tương đương khối FINALLY RELEASE(gate)
    let _permit = gate.acquire().await.expect("semaphore không bị đóng");
    (p, port_check_async(&host, p, 1000).await)
}

/// Mã giả 5.2, dòng 1–8: SCAN_PORTS(host, ports, max_parallel).
async fn scan_ports(host: &str, ports: &[u16], max_parallel: usize) -> Vec<(u16, &'static str)> {
    let gate = Arc::new(Semaphore::new(max_parallel)); // giới hạn số kết nối
    let host = Arc::new(host.to_string());
    let mut tasks = Vec::new();
    for &p in ports {
        // khởi động tác vụ nhưng chưa chờ
        tasks.push(tokio::spawn(check_one(host.clone(), p, gate.clone())));
    }
    let mut results = Vec::new();
    for t in tasks {
        // WHEN_ALL: chờ tất cả hoàn tất, giữ đúng thứ tự cổng
        results.push(t.await.expect("tác vụ kiểm tra cổng bị lỗi"));
    }
    results
}

async fn timed_scan(host: &str, ports: &[u16], k: usize) -> (Vec<(u16, &'static str)>, f64) {
    let t0 = Instant::now();
    let result = scan_ports(host, ports, k).await;
    (result, t0.elapsed().as_secs_f64())
}

fn number<T: std::str::FromStr>(text: Option<&String>, default: T, what: &str) -> T {
    match text {
        None => default,
        Some(t) => t.trim().parse().unwrap_or_else(|_| {
            eprintln!("{what} không hợp lệ: {t}");
            process::exit(1);
        }),
    }
}

pub async fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".to_string());
    let first: u16 = number(args.get(1), 9000, "Cổng đầu");
    let last: u16 = number(args.get(2), 9019, "Cổng cuối");
    let k: usize = number(args.get(3), 10, "max_parallel");
    if first == 0 || last < first || k == 0 {
        eprintln!("Cần 1 ≤ cổng_đầu ≤ cổng_cuối và max_parallel ≥ 1");
        process::exit(1);
    }
    let ports: Vec<u16> = (first..=last).collect();
    println!("[scan] Quét {} cổng {first}–{last} trên {host}, thời gian chờ 1000 ms", ports.len());
    let (seq, t_seq) = timed_scan(&host, &ports, 1).await;
    let (par, t_par) = timed_scan(&host, &ports, k).await;
    for (p, state) in &par {
        if *state != "CLOSED" {
            println!("[scan]   cổng {p}: {state}"); // chỉ in cổng không đóng
        }
    }
    let count = |s: &str| par.iter().filter(|(_, x)| *x == s).count();
    println!("[scan] Tổng hợp: OPEN={}, CLOSED={}, FILTERED={}", count("OPEN"), count("CLOSED"), count("FILTERED"));
    println!("[scan] Hai lần quét cho cùng kết quả: {}", if seq == par { "có" } else { "không" });
    println!("[scan] Tuần tự   (max_parallel=1):  {t_seq:.2} s");
    println!("[scan] Đồng thời (max_parallel={k}): {t_par:.2} s");
}
