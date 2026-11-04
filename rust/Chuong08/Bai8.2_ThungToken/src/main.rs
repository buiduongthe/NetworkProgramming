//! Bài 8.2 – Thùng token giới hạn tần suất.
//!
//! Cài đặt Mã giả 8.3 – ACQUIRE_TOKENS (Mục 8.2.4, Giáo trình Lập trình mạng).
//! Thùng dung lượng C, nạp lại R đơn vị/giây; mỗi yêu cầu lấy n đơn vị, thiếu thì chờ.
//! Chương trình cho nhiều tác vụ đồng thời gửi một chuỗi yêu cầu qua cùng một thùng và
//! in thời điểm mỗi yêu cầu được phép, so sánh với lý thuyết: C yêu cầu đầu đi ngay,
//! yêu cầu thứ m > C được phép sau (m − C)/R giây.
//!
//! Chạy:  cargo run -- [C] [R] [số_yêu_cầu] [số_tác_vụ]      (mặc định 10 2 14 2)
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Phần trạng thái được khóa bảo vệ: số đơn vị hiện có và thời điểm tính gần nhất.
struct State {
    tokens: f64,
    last: Instant,
}

/// STRUCT Bucket { cap, rate, tokens, last, lock ← ASYNC_LOCK() }  (Mã giả 8.3, dòng 1).
struct Bucket {
    cap: f64,
    rate: f64,
    lock: Mutex<State>, // ASYNC_LOCK(): tokens, last chỉ truy cập khi giữ khóa
}

impl Bucket {
    fn new(cap: f64, rate: f64) -> Bucket {
        // thùng đầy lúc khởi tạo
        Bucket { cap, rate, lock: Mutex::new(State { tokens: cap, last: Instant::now() }) }
    }
}

/// Mã giả 8.3 – ACQUIRE_TOKENS(b, n).
async fn acquire_tokens(b: &Bucket, n: f64) -> Result<(), String> {
    if n > b.cap {
        // dòng 3
        return Err("n vượt dung lượng".into()); // ArgumentError
    }
    loop {
        // dòng 4: LOOP
        let wait_ms = {
            let mut s = b.lock.lock().await; // dòng 5: WITH b.lock DO
            let now = Instant::now(); // dòng 6: NOW_SECONDS()
            let refill = now.duration_since(s.last).as_secs_f64() * b.rate; // dòng 7
            s.tokens = b.cap.min(s.tokens + refill); // dòng 8: không vượt cap
            s.last = now; // dòng 9
            if s.tokens >= n {
                // dòng 10–12
                s.tokens -= n;
                return Ok(());
            }
            (n - s.tokens) / b.rate * 1000.0 // dòng 13
        }; // nhả khóa khi ra khỏi khối
        tokio::time::sleep(Duration::from_secs_f64(wait_ms / 1000.0)).await; // dòng 15: chờ ngoài khóa
    }
}

/// Một tác vụ gửi `count` yêu cầu, mỗi yêu cầu cần 1 đơn vị.
async fn worker(b: Arc<Bucket>, wid: usize, count: usize, t0: Instant, granted: Arc<std::sync::Mutex<Vec<(f64, usize)>>>) {
    for _ in 0..count {
        acquire_tokens(&b, 1.0).await.unwrap();
        granted.lock().unwrap().push((t0.elapsed().as_secs_f64(), wid)); // ghi lại thời điểm được phép
        tokio::task::yield_now().await; // nhường lượt: giả lập gửi yêu cầu
    }
}

fn num_arg(args: &[String], i: usize, default: f64) -> f64 {
    match args.get(i) {
        Some(s) => s.parse().unwrap_or_else(|_| {
            eprintln!("Tham số phải là số: {s}");
            std::process::exit(1)
        }),
        None => default,
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cap = num_arg(&args, 0, 10.0);
    let rate = num_arg(&args, 1, 2.0);
    let total = num_arg(&args, 2, 14.0) as usize;
    let tasks = (num_arg(&args, 3, 2.0) as usize).max(1);
    let b = Arc::new(Bucket::new(cap, rate));
    println!("[bucket] Thùng: dung lượng C = {cap}, tốc độ nạp R = {rate} đơn vị/giây, mỗi yêu cầu n = 1");
    println!("[bucket] Gửi {total} yêu cầu từ {tasks} tác vụ đồng thời (thùng đầy lúc bắt đầu)");
    let t0 = Instant::now();
    let granted = Arc::new(std::sync::Mutex::new(Vec::new()));
    let handles: Vec<_> = (0..tasks)
        .map(|w| {
            let count = total / tasks + if w < total % tasks { 1 } else { 0 };
            tokio::spawn(worker(Arc::clone(&b), w + 1, count, t0, Arc::clone(&granted)))
        })
        .collect();
    for h in handles {
        h.await.unwrap(); // WHEN_ALL
    }
    let mut granted = granted.lock().unwrap().clone();
    granted.sort_by(|a, b| a.partial_cmp(b).unwrap()); // in theo thứ tự thời gian
    for (m, (t, wid)) in granted.iter().enumerate() {
        let m = m + 1;
        let theory = ((m as f64 - cap) / rate).max(0.0);
        println!("[bucket] Yêu cầu {m:02} (tác vụ {wid}) được phép tại t = {t:6.3} s   lý thuyết {theory:6.3} s");
    }
    println!("[bucket] Thử lấy n = {} > C = {cap} đơn vị", cap + 1.0);
    if let Err(e) = acquire_tokens(&b, cap + 1.0).await {
        println!("[bucket] Lỗi ArgumentError: {e}"); // dòng 3: báo lỗi ngay, không chờ mãi
    }
}
