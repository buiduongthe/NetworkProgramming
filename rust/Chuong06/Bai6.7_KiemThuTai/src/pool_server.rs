//! Bài 6.7 – Bản sao server nhóm luồng của Bài 6.1 (Mã giả 6.2, Mục 6.2.2) để kiểm thử tải.
//!
//! Khác Bài 6.1: không ghi nhật ký từng kết nối (số kết nối bị từ chối 503 được bộ sinh tải thống kê)
//! và có tham số delay_ms – độ trễ giả lập của PROCESS_REQUEST.
//!
//! Chạy:  cargo run -- pool [host] [port] [k] [queue_size] [delay_ms]   (mặc định 0.0.0.0 9000 8 16 50)
use crate::lookup_service::DELAY_MS;
use crate::netutil::{exit_with, int_arg, parse_endpoint, send_json};
use crate::thread_server::{log, serve};
use serde_json::json;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;

/// Hàng đợi dùng chung: nhiều luồng thợ cùng lấy việc nên đầu nhận được bọc trong Mutex.
type SharedQueue = Arc<Mutex<Receiver<(TcpStream, String)>>>;

/// WORKER: lấy kết nối khỏi hàng đợi rồi phục vụ bằng SERVE.
fn worker(q: SharedQueue) {
    loop {
        let job = q.lock().unwrap().recv(); // q.take(): chờ đến khi có việc
        let Ok((conn, addr)) = job else { return }; // hàng đợi đã đóng
        serve(conn, addr, Arc::new(AtomicUsize::new(1))); // bộ đếm tạm ATOMIC_COUNTER(1)
    }
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9000);
    let k = int_arg(args, 2, 8);
    let queue_size = int_arg(args, 3, 16) as usize;
    DELAY_MS.store(int_arg(args, 4, 50), Ordering::SeqCst);
    // SOCKET, SETOPT, BIND, LISTEN như Mã giả 6.1
    let srv = TcpListener::bind((host.as_str(), port))
        .unwrap_or_else(|e| exit_with(&format!("Không mở được cổng {port}: {e}")));
    // BLOCKING_QUEUE(queue_size): kênh đồng bộ có sức chứa queue_size
    let (tx, rx) = sync_channel::<(TcpStream, String)>(queue_size);
    let q: SharedQueue = Arc::new(Mutex::new(rx));
    for _ in 1..=k {
        // FOR i ← 1 TO k: SPAWN_THREAD(WORKER, q)
        let q = Arc::clone(&q);
        thread::spawn(move || worker(q));
    }
    log(&format!("[server] Đang lắng nghe tại {host}:{port}"));
    log(&format!("[server] Nhóm {k} luồng thợ, hàng đợi {queue_size} chỗ, độ trễ {} ms", DELAY_MS.load(Ordering::SeqCst)));
    loop {
        let (conn, a) = match srv.accept() {
            Ok(x) => x, // ACCEPT
            Err(_) => continue,
        };
        let addr = a.to_string();
        // q.try_put: không chờ; hàng đợi đầy thì nhận lại kết nối để trả 503
        if let Err(TrySendError::Full((mut conn, _))) = tx.try_send((conn, addr)) {
            // client đã đi thì bỏ qua lỗi gửi
            let _ = send_json(&mut conn, &json!({"status": 503, "error": "Server bận"}));
            drop(conn); // CLOSE(conn)
        }
    }
}
