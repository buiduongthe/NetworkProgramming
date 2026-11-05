//! Tranh chấp dữ liệu (race condition), khóa, và tránh bế tắc (deadlock).
//!
//! 1. Tranh chấp: 4 luồng cùng "đọc – làm việc khác – ghi" lên một biến đếm chung. Rust an toàn KHÔNG
//!    cho hai luồng cùng ghi một biến thường (lỗi biên dịch), nên bài dùng AtomicI64 với hai thao tác
//!    RIÊNG load() rồi store(): mỗi thao tác nguyên tử, nhưng cả cặp thì không – vẫn mất cập nhật.
//!    Đây là "tranh chấp logic" mà trình biên dịch không bắt được. Sửa bằng Mutex hoặc fetch_add.
//! 2. Bế tắc: luồng T1 giữ khóa X rồi chờ khóa Y, luồng T2 giữ Y rồi chờ X – cả hai chờ nhau mãi.
//!    Mutex của thư viện chuẩn không có "chờ có hạn", bài tự viết lock_timeout bằng try_lock lặp lại
//!    để phát hiện thay vì treo; cách tránh là luôn lấy khóa theo cùng một thứ tự.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::thread;
use std::time::{Duration, Instant};

const N_THREADS: i64 = 4;
const N_ITER: i64 = 5_000;

fn race_demo() {
    println!("Tranh chấp dữ liệu: {N_THREADS} luồng × {N_ITER} lần \"đọc – nhường CPU – ghi\"");
    println!("  lên một biến đếm chung (kết quả đúng phải là {}).", N_THREADS * N_ITER);
    let total = N_THREADS * N_ITER;
    let report = |name: &str, value: i64, dt: f64| {
        let verdict = if value == total { "đúng".to_string() } else { format!("mất {} lần cộng", total - value) };
        println!("  {name:<16}: {value:>6} / {total} → {verdict} ({dt:.2} s)");
    };

    // không khóa: load và store tách rời, luồng khác chen vào giữa
    let c = AtomicI64::new(0);
    let t = Instant::now();
    thread::scope(|s| {
        for _ in 0..N_THREADS {
            s.spawn(|| {
                for _ in 0..N_ITER {
                    let v = c.load(Ordering::SeqCst); // đọc
                    thread::yield_now(); // nhường CPU: luồng khác có thể chen vào đây
                    c.store(v + 1, Ordering::SeqCst); // ghi – có thể đè lên kết quả của luồng khác
                }
            });
        }
    });
    report("không khóa", c.load(Ordering::SeqCst), t.elapsed().as_secs_f64());

    // có khóa: đoạn găng đọc – nhường – ghi chỉ một luồng được vào
    let m = Mutex::new(0i64);
    let t = Instant::now();
    thread::scope(|s| {
        for _ in 0..N_THREADS {
            s.spawn(|| {
                for _ in 0..N_ITER {
                    let mut g = m.lock().unwrap();
                    let v = *g;
                    thread::yield_now();
                    *g = v + 1;
                }
            });
        }
    });
    report("có khóa (Mutex)", *m.lock().unwrap(), t.elapsed().as_secs_f64());

    // thao tác nguyên tử đọc-cộng-ghi trong một bước
    let c = AtomicI64::new(0);
    let t = Instant::now();
    thread::scope(|s| {
        for _ in 0..N_THREADS {
            s.spawn(|| {
                for _ in 0..N_ITER {
                    c.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
    });
    report("AtomicI64 add", c.load(Ordering::SeqCst), t.elapsed().as_secs_f64());
}

/// Chờ khóa tối đa `timeout`; None nếu quá hạn (nghi bế tắc).
fn lock_timeout<T>(m: &Mutex<T>, timeout: Duration) -> Option<MutexGuard<'_, T>> {
    let deadline = Instant::now() + timeout;
    loop {
        match m.try_lock() {
            Ok(g) => return Some(g),
            Err(TryLockError::Poisoned(p)) => return Some(p.into_inner()),
            Err(TryLockError::WouldBlock) if Instant::now() >= deadline => return None,
            Err(TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(1)),
        }
    }
}

fn deadlock_demo() {
    println!();
    println!("Bế tắc (deadlock): T1 chuyển X→Y, T2 chuyển Y→X, mỗi luồng cần cả");
    println!("  hai khóa; số dư ban đầu X = 100, Y = 100.");
    // mỗi tài khoản một khóa bảo vệ số dư của chính nó
    let x = Mutex::new(100i64);
    let y = Mutex::new(100i64);
    let out = Mutex::new(()); // khóa riêng để hai luồng không in xen giữa dòng
    let say = |text: String| {
        let _g = out.lock().unwrap();
        println!("  {text}");
    };
    let acc = |n: &str| if n == "X" { &x } else { &y };

    let transfer = |name: &str, src: &str, dst: &str, amount: i64, ordered: bool| {
        let (first, second) = if ordered && src > dst { (dst, src) } else { (src, dst) };
        let mut g1 = acc(first).lock().unwrap();
        say(format!("{name} giữ khóa {first}, chờ khóa {second}"));
        thread::sleep(Duration::from_millis(50)); // để luồng kia kịp giữ khóa còn lại
        let Some(mut g2) = lock_timeout(acc(second), Duration::from_millis(500)) else {
            say(format!("{name}: chờ {second} quá 0,5 s → nghi bế tắc, nhả {first}, bỏ cuộc"));
            return;
        };
        // g1 là khóa của `first`, g2 là của `second`; xác định đâu là nguồn, đâu là đích
        let (s, d) = if first == src { (&mut *g1, &mut *g2) } else { (&mut *g2, &mut *g1) };
        *s -= amount;
        *d += amount;
        say(format!("{name}: đã chuyển {amount} từ {src} sang {dst}"));
    };

    for ordered in [false, true] {
        if ordered {
            println!("Cách tránh: mọi luồng lấy khóa theo cùng thứ tự (X trước, Y sau):");
        }
        thread::scope(|s| {
            s.spawn(|| transfer("T1", "X", "Y", 10, ordered));
            thread::sleep(Duration::from_millis(10));
            s.spawn(|| transfer("T2", "Y", "X", 5, ordered));
        });
        let (bx, by) = (*x.lock().unwrap(), *y.lock().unwrap());
        println!("  Số dư: X = {bx}, Y = {by} (tổng {})", bx + by);
    }
}

pub fn run() {
    race_demo();
    deadlock_demo();
}
