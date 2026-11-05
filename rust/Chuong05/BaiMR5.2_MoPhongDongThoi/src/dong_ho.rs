//! Ghi lại khoảng thời gian chạy của từng tác vụ và vẽ biểu đồ Gantt bằng văn bản.
//!
//! Mỗi đoạn ghi (tác vụ, worker, bắt đầu, kết thúc) theo đồng hồ đơn điệu Instant – dùng chung cho mọi
//! luồng. Mỗi hàng của biểu đồ là một worker (luồng, vòng lặp sự kiện…); mỗi cột là một khoảng thời gian:
//!     chữ cái  = đúng một tác vụ đang chạy trên worker đó (A = tác vụ 1, B = tác vụ 2, ...)
//!     *        = nhiều tác vụ cùng lúc trên một worker (đặc trưng của mô hình bất đồng bộ)
//! Ký tự của một cột lấy theo thời điểm giữa cột.

use std::collections::HashMap;
use std::sync::Mutex;
use std::thread::{self, ThreadId};
use std::time::Instant;

const COLS: usize = 48;
const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Một đoạn chạy: (tác vụ, worker, bắt đầu, kết thúc) – giây kể từ t0.
type Seg = (usize, String, f64, f64);

pub struct Recorder {
    t0: Instant,
    segs: Mutex<Vec<Seg>>,
}

impl Recorder {
    pub fn new() -> Recorder {
        Recorder { t0: Instant::now(), segs: Mutex::new(Vec::new()) }
    }

    /// start, end: các mốc Instant tuyệt đối.
    pub fn add(&self, task: usize, worker: &str, start: Instant, end: Instant) {
        let s = start.duration_since(self.t0).as_secs_f64();
        let e = end.duration_since(self.t0).as_secs_f64();
        self.segs.lock().unwrap().push((task, worker.to_string(), s, e));
    }

    pub fn total(&self) -> f64 {
        self.segs.lock().unwrap().iter().map(|s| s.3).fold(0.0, f64::max)
    }
}

/// Đặt tên dễ đọc cho worker: luồng-1, luồng-2, ... theo thứ tự xuất hiện.
pub struct Names {
    prefix: String,
    map: Mutex<HashMap<ThreadId, String>>,
}

impl Names {
    pub fn new(prefix: &str) -> Names {
        Names { prefix: prefix.into(), map: Mutex::new(HashMap::new()) }
    }
    /// Tên của luồng đang chạy.
    pub fn current(&self) -> String {
        let mut m = self.map.lock().unwrap();
        let n = m.len();
        m.entry(thread::current().id()).or_insert_with(|| format!("{}-{}", self.prefix, n + 1)).clone()
    }
}

pub fn gantt(title: &str, rec: &Recorder, scale: f64, kind: &str) {
    let max_rows = 8;
    let total = rec.total();
    println!("{title}: {total:.2} s");
    let mut segs = rec.segs.lock().unwrap().clone();
    segs.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
    let mut workers: Vec<String> = Vec::new();
    for s in &segs {
        if !workers.contains(&s.1) {
            workers.push(s.1.clone());
        }
    }
    for w in workers.iter().take(max_rows) {
        let mine: Vec<&Seg> = segs.iter().filter(|s| s.1 == *w).collect();
        let mut cells = String::new();
        for c in 0..COLS {
            let lo = scale * c as f64 / COLS as f64;
            let hi = scale * (c + 1) as f64 / COLS as f64;
            let mid = (lo + hi) / 2.0;
            let now: Vec<&&Seg> = mine.iter().filter(|s| s.2 <= mid && mid < s.3).collect(); // đang chạy giữa cột
            if now.len() >= 2 {
                cells.push('*'); // nhiều tác vụ cùng lúc
            } else if let Some(s) = now.first() {
                cells.push(LETTERS[s.0 % 26] as char);
            } else {
                // tác vụ ngắn hơn một cột
                match mine.iter().find(|s| s.2 < hi && s.3 > lo) {
                    Some(s) => cells.push(LETTERS[s.0 % 26] as char),
                    None => cells.push(' '),
                }
            }
        }
        println!("  {w:<12}|{cells}|");
    }
    if workers.len() > max_rows {
        println!("  … và {} {kind} khác, mỗi {kind} chạy 1 tác vụ", workers.len() - max_rows);
    }
}

pub fn axis(scale: f64) {
    let right = format!("{scale:.1} s");
    println!("  {:<12} 0{right:>COLS$}", "");
}

/// rows: [(tên mô hình, thời gian)]; hàng đầu là mô hình tuần tự (mốc so sánh).
pub fn table(rows: &[(String, f64)]) {
    let base = rows[0].1;
    println!("  {:<30} {:>9} {:>9}", "Mô hình", "Thời gian", "Tăng tốc");
    for (name, t) in rows {
        println!("  {name:<30} {t:>7.2} s {:>8.1}×", base / t);
    }
}
