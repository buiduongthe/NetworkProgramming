//! Bài 6.1 – Server tra cứu dùng nhóm luồng và hàng đợi có giới hạn.
//!
//! Cài đặt Mã giả 6.2 (Mục 6.2.2, Giáo trình Lập trình mạng). Kế thừa Mã giả 6.1: dùng lại
//! nguyên SERVE (thread_server.rs); k luồng thợ dùng chung một hàng đợi; hàng đợi đầy → 503.
//!
//! Chạy:  cargo run -- pool [host] [port] [k] [queue_size]   (mặc định 0.0.0.0 9000 8 16)
use crate::netutil::{exit_with, int_arg, parse_endpoint, send_json};
use crate::thread_server::{log, serve};
use serde_json::json;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{sync_channel, Receiver, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;

/// Hàng đợi dùng chung: nhiều luồng thợ cùng lấy việc nên đầu nhận được bọc trong Mutex.
type SharedQueue = Arc<Mutex<Receiver<(TcpStream, String)>>>;

/// WORKER: lấy kết nối khỏi hàng đợi rồi phục vụ bằng SERVE.
fn worker(i: u64, q: SharedQueue) {
    loop {
        let job = q.lock().unwrap().recv(); // q.take(): chờ đến khi có việc
        let Ok((conn, addr)) = job else { return }; // hàng đợi đã đóng
        log(&format!("[server] Luồng thợ {i} phục vụ {addr}"));
        serve(conn, addr, Arc::new(AtomicUsize::new(1))); // bộ đếm tạm ATOMIC_COUNTER(1)
    }
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9000);
    let k = int_arg(args, 2, 8);
    let queue_size = int_arg(args, 3, 16) as usize;
    // SOCKET, SETOPT, BIND, LISTEN như Mã giả 6.1
    let srv = TcpListener::bind((host.as_str(), port))
        .unwrap_or_else(|e| exit_with(&format!("Không mở được cổng {port}: {e}")));
    // BLOCKING_QUEUE(queue_size): kênh đồng bộ có sức chứa queue_size
    let (tx, rx) = sync_channel::<(TcpStream, String)>(queue_size);
    let q: SharedQueue = Arc::new(Mutex::new(rx));
    for i in 1..=k {
        // FOR i ← 1 TO k: SPAWN_THREAD(WORKER, q)
        let q = Arc::clone(&q);
        thread::spawn(move || worker(i, q));
    }
    log(&format!("[server] Đang lắng nghe tại {host}:{port}"));
    log(&format!("[server] Nhóm {k} luồng thợ, hàng đợi {queue_size} chỗ"));
    loop {
        let (conn, a) = match srv.accept() {
            Ok(x) => x, // ACCEPT
            Err(e) => {
                log(&format!("[server] Lỗi ACCEPT: {e}"));
                continue;
            }
        };
        let addr = a.to_string();
        log(&format!("[server] Nhận {addr}"));
        // q.try_put: không chờ; hàng đợi đầy thì nhận lại kết nối để trả 503
        if let Err(TrySendError::Full((mut conn, addr))) = tx.try_send((conn, addr)) {
            log(&format!("[server] Hàng đợi đầy, trả 503 cho {addr}"));
            // client đã đi thì bỏ qua lỗi gửi
            let _ = send_json(&mut conn, &json!({"status": 503, "error": "Server bận"}));
            drop(conn); // CLOSE(conn)
        }
    }
}
