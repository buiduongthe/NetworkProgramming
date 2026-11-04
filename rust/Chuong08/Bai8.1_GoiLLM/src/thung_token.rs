//! Bài 8.1 – Mã giả 8.3 ACQUIRE_TOKENS (chép gọn từ Bài 8.2, bản đồng bộ: Mutex, thread::sleep).
//!
//! Thùng token để client tự giới hạn tần suất gọi LLM (Mục 8.2.4). Được llm_client.rs dùng.
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// STRUCT Bucket { cap, rate, tokens, last, lock }: tokens, last nằm trong khóa.
pub struct Bucket {
    cap: f64,
    rate: f64,
    lock: Mutex<(f64, Instant)>, // (tokens, last)
}

impl Bucket {
    pub fn new(cap: f64, rate: f64) -> Bucket {
        Bucket { cap, rate, lock: Mutex::new((cap, Instant::now())) } // thùng đầy lúc khởi tạo
    }
}

/// ACQUIRE_TOKENS(b, n): lấy n đơn vị, thiếu thì chờ (ngoài khóa) rồi tính lại.
pub fn acquire_tokens(b: &Bucket, n: f64) -> Result<(), String> {
    if n > b.cap {
        return Err("n vượt dung lượng".into()); // ArgumentError
    }
    loop {
        let wait_ms = {
            let mut g = b.lock.lock().unwrap(); // WITH b.lock DO
            let (tokens, last) = &mut *g;
            let now = Instant::now();
            *tokens = b.cap.min(*tokens + now.duration_since(*last).as_secs_f64() * b.rate);
            *last = now;
            if *tokens >= n {
                *tokens -= n;
                return Ok(());
            }
            (n - *tokens) / b.rate * 1000.0
        }; // nhả khóa
        thread::sleep(Duration::from_secs_f64(wait_ms / 1000.0)); // không giữ khóa khi chờ
    }
}
