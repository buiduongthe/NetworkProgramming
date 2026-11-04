//! Bài 6.2 – Server tra cứu bất đồng bộ: mỗi kết nối một tác vụ.
//!
//! Cài đặt Mã giả 6.3 (Mục 6.2.3, Giáo trình Lập trình mạng). Kế thừa Mã giả 6.1 (SOCKET, SETOPT,
//! BIND, LISTEN), các hàm khung bất đồng bộ của Mã giả 5.1 (async_frame.rs) và PROCESS_REQUEST
//! của Mã giả 3.7.
//!
//! Chạy:  cargo run -- server [host] [port]      (mặc định 0.0.0.0 9000)
use crate::async_frame::{recv_json_async, send_json_async};
use crate::netutil::{exit_with, parse_endpoint, NetError};
use serde_json::{json, Map, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::net::{TcpListener, TcpSocket, TcpStream};

/// LOAD_JSON_FILE("glossary.json")
static DICT: LazyLock<Map<String, Value>> = LazyLock::new(|| {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/glossary.json"); // tệp nằm cạnh mã nguồn
    let text = std::fs::read_to_string(path).expect("Không đọc được glossary.json");
    serde_json::from_str(&text).expect("glossary.json sai định dạng")
});

/// Mã giả 3.7, dòng 2–12 (giữ nguyên): nghiệp vụ nhanh, không chặn.
fn process_request(req: &Value) -> Value {
    match req["op"].as_str() {
        Some("lookup") => {
            let term = req["term"].as_str().unwrap_or("");
            match DICT.get(term) {
                Some(text) => json!({"status": 200, "term": term, "text": text}),
                None => json!({"status": 404, "error": "Không có thuật ngữ"}),
            }
        }
        Some("list") => json!({"status": 200, "terms": DICT.keys().collect::<Vec<_>>()}),
        _ => json!({"status": 400, "error": "Sai thao tác"}),
    }
}

/// SERVE_ASYNC: vòng yêu cầu–phản hồi; khi chờ ở .await, tác vụ không chiếm luồng nào.
async fn serve_async(mut conn: TcpStream, addr: String) {
    let reason = loop {
        // WITH_TIMEOUT(300000, RECV_JSON_ASYNC(conn))
        let req = match tokio::time::timeout(Duration::from_millis(300000), recv_json_async(&mut conn)).await {
            Err(_) => break "quá thời gian chờ nhận".to_string(), // Timeout
            Ok(Ok(req)) => req,
            Ok(Err(NetError::ConnectionClosed(0))) => break "client đóng kết nối".to_string(),
            Ok(Err(e @ NetError::ConnectionClosed(_))) => break e.to_string(),
            Ok(Err(e @ (NetError::Protocol(_) | NetError::Json(_)))) => break format!("lỗi giao thức: {e}"),
            Ok(Err(NetError::Io(e))) => break format!("lỗi mạng: {e}"), // NetError
        };
        if let Err(e) = send_json_async(&mut conn, &process_request(&req)).await {
            break format!("lỗi mạng: {e}");
        }
    };
    drop(conn); // FINALLY: CLOSE(conn)
    println!("[server] Kết thúc {addr}: {reason}");
}

/// SOCKET(INET, STREAM), SETOPT(REUSEADDR), BIND, LISTEN(128) như Mã giả 6.1.
async fn open_listener(host: &str, port: u16) -> std::io::Result<TcpListener> {
    let addr = tokio::net::lookup_host((host, port))
        .await?
        .find(|a| a.is_ipv4())
        .ok_or(std::io::Error::other("không có địa chỉ IPv4"))?;
    let srv = TcpSocket::new_v4()?; // SOCKET(INET, STREAM)
    srv.set_reuseaddr(true)?; // SETOPT(REUSEADDR, TRUE)
    srv.bind(addr)?; // BIND
    srv.listen(128) // LISTEN, backlog 128
}

async fn async_server(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9000);
    let srv = open_listener(&host, port)
        .await
        .unwrap_or_else(|e| exit_with(&format!("Không mở được cổng {port}: {e}")));
    let tasks = Arc::new(AtomicUsize::new(0)); // số tác vụ đang chạy
    let threads = tokio::runtime::Handle::current().metrics().num_workers(); // số luồng thợ của tokio
    println!("[server] Server bất đồng bộ đang lắng nghe tại {host}:{port}");
    loop {
        let (conn, a) = match srv.accept().await {
            Ok(x) => x, // AWAIT ACCEPT_ASYNC(srv)
            Err(e) => {
                println!("[server] Lỗi ACCEPT: {e}");
                continue;
            }
        };
        let addr = a.to_string();
        let n = tasks.fetch_add(1, Ordering::SeqCst) + 1;
        println!("[server] Nhận {addr} (tác vụ: {n}, luồng: {threads})");
        let counter = Arc::clone(&tasks);
        tokio::spawn(async move {
            // RUN_BACKGROUND(SERVE_ASYNC(conn, addr)): không chờ
            serve_async(conn, addr).await;
            counter.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

pub async fn run(args: &[String]) {
    tokio::select! {
        _ = async_server(args) => {}
        _ = tokio::signal::ctrl_c() => println!("[server] Dừng server"),
    }
}
