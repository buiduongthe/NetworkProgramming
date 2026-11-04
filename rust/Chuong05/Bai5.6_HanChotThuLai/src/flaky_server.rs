//! Server tra cứu "khó tính" để chạy thử hạn chót và thử lại.
//!
//! Server dùng khung JSON của Mã giả 5.1 và cố ý gặp sự cố ở các yêu cầu đầu tiên:
//!   yêu cầu #1: trả lời chậm 2000 ms (vượt hạn chót của client);
//!   yêu cầu #2: trả lời {status: 503, retry_after_ms: 300} – quá tải tạm thời;
//!   từ yêu cầu #3: trả lời bình thường. Trường delay_ms (nếu có) là thời gian xử lý giả lập.

use crate::khung_json_async::{parse_endpoint, recv_json_async, send_json_async, CancelToken, FrameError};
use serde_json::{json, Value};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, Duration};

static COUNT: AtomicUsize = AtomicUsize::new(0); // số yêu cầu đã nhận

fn lookup(term: &Value) -> Value {
    let dict = [("TCP", "Giao thức truyền tin cậy, hướng kết nối"), ("UDP", "Giao thức datagram, không kết nối")];
    match dict.iter().find(|(k, _)| Some(*k) == term.as_str()) {
        Some((k, text)) => json!({"status": 200, "term": k, "text": text}),
        None => json!({"status": 404, "error": "Không có thuật ngữ"}),
    }
}

async fn answer(req: &Value) -> Value {
    let n = COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    if n == 1 {
        println!("[server] Yêu cầu #{n}: cố ý trả lời chậm 2000 ms");
        sleep(Duration::from_millis(2000)).await;
        return lookup(&req["term"]);
    }
    if n == 2 {
        println!("[server] Yêu cầu #{n}: trả lời 503, đề nghị chờ 300 ms");
        return json!({"status": 503, "error": "Quá tải tạm thời", "retry_after_ms": 300});
    }
    let delay = req["delay_ms"].as_u64().unwrap_or(0);
    println!("[server] Yêu cầu #{n}: xử lý {delay} ms rồi trả lời");
    sleep(Duration::from_millis(delay)).await;
    lookup(&req["term"])
}

async fn session(mut conn: TcpStream) {
    let ct = CancelToken::new();
    let result: Result<(), FrameError> = async {
        loop {
            let req = recv_json_async(&mut conn, &ct).await?;
            let resp = answer(&req).await;
            send_json_async(&mut conn, &resp, &ct).await?;
        }
    }
    .await;
    match result {
        Err(FrameError::ConnectionClosed(0)) | Ok(()) => {} // client đóng tại ranh giới khung
        Err(FrameError::ConnectionClosed(n)) => println!("[server] Kết nối đóng giữa khung (đã nhận {n} byte)"),
        Err(FrameError::Io(e)) => println!("[server] Không gửi được phản hồi, client đã đóng kết nối ({:?})", e.kind()),
        Err(e) => println!("[server] Không gửi được phản hồi, client đã đóng kết nối ({e})"),
    }
}

pub async fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9100);
    let srv = match TcpListener::bind((host.as_str(), port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[server] Không mở được {host}:{port}: {e}");
            process::exit(1);
        }
    };
    println!("[server] Đang lắng nghe tại {host}:{port}");
    loop {
        tokio::select! {
            r = srv.accept() => if let Ok((conn, _)) = r {
                tokio::spawn(session(conn));
            },
            _ = tokio::signal::ctrl_c() => {
                println!("[server] Dừng server");
                return;
            }
        }
    }
}
