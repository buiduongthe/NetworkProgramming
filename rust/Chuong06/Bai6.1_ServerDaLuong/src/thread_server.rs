//! Bài 6.1 – Server tra cứu đa luồng: mỗi kết nối một luồng.
//!
//! Cài đặt Mã giả 6.1 (Mục 6.2.1, Giáo trình Lập trình mạng). Kế thừa Mã giả 3.1 (khung server),
//! phần nghiệp vụ là HANDLE_LOOKUP của Mã giả 3.7 (lookup_service.rs).
//!
//! Chạy:  cargo run -- threaded [host] [port] [max_clients]   (mặc định 0.0.0.0 9000 500)
use crate::lookup_service::handle_lookup;
use crate::netutil::{int_arg, parse_endpoint};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const MAX_CLIENTS: u64 = 500; // CONST MAX_CLIENTS ← 500

/// ATOMIC_COUNTER: phép tăng, giảm không bị xen ngang khi nhiều luồng cùng thao tác.
pub type AtomicCounter = Arc<AtomicUsize>;

/// LOG: println! khóa stdout trong lúc in nên các luồng không chen vào giữa dòng của nhau.
pub fn log(msg: &str) {
    println!("{msg}");
}

/// SERVE – khối TRY của Mã giả 3.1, chạy trong luồng riêng của mỗi kết nối.
pub fn serve(mut conn: TcpStream, addr: String, active: AtomicCounter) {
    // SETOPT(RECV_TIMEOUT, 300000): đóng kết nối nhàn rỗi
    let result = conn
        .set_read_timeout(Some(Duration::from_millis(300000)))
        .map_err(|e| e.into())
        .and_then(|_| handle_lookup(&mut conn));
    if let Err(e) = result {
        // NetError, Timeout (và ConnectionClosed, ProtocolError như khối CATCH của Mã giả 3.1)
        log(&format!("[server] Lỗi với {addr}: {e}"));
    }
    // FINALLY: luôn đóng socket và giảm bộ đếm, kể cả khi lỗi
    drop(conn); // CLOSE(conn)
    active.fetch_sub(1, Ordering::SeqCst); // active.decrement()
    log(&format!("[server] Client {addr} đã đóng kết nối"));
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9000);
    let max_clients = int_arg(args, 2, MAX_CLIENTS) as usize;
    // SOCKET(INET, STREAM), SETOPT(REUSEADDR), BIND, LISTEN: TcpListener::bind làm cả bốn bước
    // (trên Linux/macOS Rust tự bật SO_REUSEADDR; backlog mặc định 128)
    let srv = TcpListener::bind((host.as_str(), port))
        .unwrap_or_else(|e| crate::netutil::exit_with(&format!("Không mở được cổng {port}: {e}")));
    log(&format!("[server] Đang lắng nghe tại {host}:{port}"));
    log(&format!("[server] Mỗi kết nối một luồng, MAX_CLIENTS = {max_clients}"));
    let active: AtomicCounter = Arc::new(AtomicUsize::new(0)); // ATOMIC_COUNTER(0)
    loop {
        let (conn, a) = match srv.accept() {
            Ok(x) => x, // ACCEPT
            Err(e) => {
                log(&format!("[server] Lỗi ACCEPT: {e}"));
                continue;
            }
        };
        let addr = a.to_string();
        if active.load(Ordering::SeqCst) >= max_clients {
            // MỚI: giới hạn
            log(&format!("[server] Từ chối {addr}: đã đủ {max_clients} kết nối"));
            drop(conn); // CLOSE(conn); CONTINUE
            continue;
        }
        let n = active.fetch_add(1, Ordering::SeqCst) + 1; // active.increment()
        log(&format!("[server] Nhận {addr}, tạo luồng mới (đang phục vụ: {n})"));
        let counter = Arc::clone(&active);
        thread::spawn(move || serve(conn, addr, counter)); // SPAWN_THREAD(SERVE, conn, addr, active)
    }
}
