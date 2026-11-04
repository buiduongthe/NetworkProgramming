//! Yêu cầu có hạn chót và thử lại có lùi theo cấp số nhân + nhiễu ngẫu nhiên.
//!
//! Cài đặt Mã giả 5.7 REQUEST_WITH_DEADLINE (Mục 5.4.2) và Mã giả 5.8 RETRY_ASYNC (Mục 5.4.3).
//! Kế thừa: Mã giả 5.1 (khung JSON bất đồng bộ), 2.2 (SAFE_CONNECT, NetError có trường retry),
//! 4.4 (UDP_REQUEST – vòng gửi lại có lùi thời gian).
//! Ánh xạ: CANCEL_AFTER(ms) và LINK(a, b) được viết thành hai hàm nhỏ trên CancelToken;
//! user_ct là tín hiệu hủy của nút Hủy, được truyền qua RETRY_ASYNC tới từng lần thử op(ct) và DELAY_ASYNC.

use crate::khung_json_async::{json_bytes, parse_endpoint, recv_json_async, safe_connect_async, send_json_async, CancelToken, FrameError, NetError};
use serde_json::{json, Value};
use std::cell::Cell;
use std::future::Future;
use std::hash::{BuildHasher, RandomState};
use std::sync::Mutex;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration, Instant};

/// Lỗi của REQUEST_WITH_DEADLINE (Mã giả 5.7).
enum DeadlineError {
    Cancelled,          // người dùng bấm Hủy
    Timeout(String),    // hết hạn chót của toàn bộ yêu cầu
    Other(FrameError),  // lỗi mạng, lỗi giao thức khác
}

/// Lỗi của một lần thử op().
enum OpError {
    Net(NetError),   // lỗi đã phân loại: RETRY_ASYNC xem trường retry để quyết định thử lại
    Timeout(String), // hết hạn chót (Mã giả 5.7): RETRY_ASYNC luôn thử lại
    Cancelled,       // người dùng hủy: không thử lại
    Other(String),   // lỗi khác: không thử lại
}

/// CANCEL_AFTER(ms): tín hiệu tự phát sau ms mili giây.
fn cancel_after(ms: u64) -> CancelToken {
    let t = CancelToken::new();
    let t2 = t.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(ms)).await;
        t2.cancel();
    });
    t
}

/// LINK(a, b): tín hiệu mới, phát khi a hoặc b phát.
fn link(a: &CancelToken, b: &CancelToken) -> CancelToken {
    let t = CancelToken::new();
    let (a, b, t2) = (a.clone(), b.clone(), t.clone());
    tokio::spawn(async move {
        tokio::select! {
            _ = a.cancelled() => {},
            _ = b.cancelled() => {},
        }
        t2.cancel();
    });
    t
}

/// Mã giả 5.7 – REQUEST_WITH_DEADLINE(sock, req, timeout_ms, user_ct).
async fn request_with_deadline(sock: &mut TcpStream, req: &Value, timeout_ms: u64, user_ct: &CancelToken) -> Result<Value, DeadlineError> {
    let ct = link(user_ct, &cancel_after(timeout_ms)); // một trong hai
    let result = async {
        send_json_async(sock, req, &ct).await?;
        recv_json_async(sock, &ct).await
    }
    .await;
    match result {
        Ok(resp) => Ok(resp),
        Err(FrameError::Cancelled) => {
            let _ = sock.shutdown().await; // CLOSE(sock): trạng thái khung không xác định
            if user_ct.is_cancelled() {
                return Err(DeadlineError::Cancelled); // IF user_ct.cancelled THEN RAISE Cancelled
            }
            Err(DeadlineError::Timeout(format!("Quá hạn {timeout_ms} ms")))
        }
        Err(e) => Err(DeadlineError::Other(e)),
    }
}

/// Số ngẫu nhiên trong [0, 1) – dùng khóa ngẫu nhiên của RandomState (thư viện chuẩn chưa có hàm random).
fn random_unit() -> f64 {
    (RandomState::new().hash_one(0u8) >> 11) as f64 / (1u64 << 53) as f64
}

/// Mã giả 5.8 – RETRY_ASYNC(op, max_tries, base_ms, cap_ms, ct).
/// `op` là một hàm: mỗi lần gọi `op(ct)` tạo một lần thử mới (một Future mới) nhận tín hiệu hủy ct.
async fn retry_async<F, Fut>(mut op: F, max_tries: u32, base_ms: u64, cap_ms: u64, ct: CancelToken) -> Result<Value, OpError>
where
    F: FnMut(CancelToken) -> Fut,
    Fut: Future<Output = Result<Value, OpError>>,
{
    if max_tries < 1 {
        return Err(OpError::Other("max_tries phải ≥ 1".to_string()));
    }
    for k in 0..max_tries {
        // CATCH Timeout, NetError AS e – Cancelled và lỗi khác không bắt: trả ngay cho nơi gọi
        let (err, ok, code, retry_after) = match op(ct.clone()).await {
            Ok(v) => return Ok(v), // RETURN AWAIT op(ct)
            Err(OpError::Timeout(m)) => (OpError::Timeout(m), true, "TIMEOUT", None), // Timeout luôn đáng thử lại
            Err(OpError::Net(e)) => {
                let (ok, code, ra) = (e.retry, e.code, e.retry_after);
                (OpError::Net(e), ok, code, ra)
            }
            Err(other) => return Err(other),
        };
        if !ok || k == max_tries - 1 {
            return Err(err); // RAISE e
        }
        let upper = cap_ms.min(base_ms * 2u64.pow(k)); // MIN(cap_ms, base_ms * 2^k)
        let mut delay = random_unit() * upper as f64; // RANDOM(0, upper) – jitter
        if let Some(ra) = retry_after {
            delay = delay.max(ra); // server đề nghị
        }
        log(&format!("  lỗi {code} (có thể thử lại); chờ {delay:.0} ms, giới hạn trên {upper} ms"));
        // AWAIT DELAY_ASYNC(delay, ct): không chặn luồng, hủy được khi đang chờ
        tokio::select! {
            _ = sleep(Duration::from_secs_f64(delay / 1000.0)) => {}
            _ = ct.cancelled() => return Err(OpError::Cancelled),
        }
    }
    unreachable!("vòng lặp luôn trả kết quả hoặc lỗi ở lần thử cuối")
}

static T0: Mutex<Option<Instant>> = Mutex::new(None);

fn log(text: &str) {
    let t0 = T0.lock().unwrap().unwrap_or_else(Instant::now);
    println!("[client] [t={:5.0} ms] {text}", t0.elapsed().as_secs_f64() * 1000.0);
}

/// Một lần thử của op(ct): kết nối mới, một lần REQUEST_WITH_DEADLINE.
async fn lookup_once(host: &str, port: u16, req: &Value, deadline_ms: u64, user_ct: CancelToken, tries: &Cell<u32>) -> Result<Value, OpError> {
    tries.set(tries.get() + 1);
    log(&format!("Lần thử {}: gửi {}", tries.get(), String::from_utf8_lossy(&json_bytes(req))));
    let mut sock = safe_connect_async(host, port, 3000).await.map_err(OpError::Net)?;
    let resp = match request_with_deadline(&mut sock, req, deadline_ms, &user_ct).await {
        Ok(r) => r,
        Err(DeadlineError::Timeout(m)) => return Err(OpError::Timeout(m)), // hết hạn: RETRY_ASYNC sẽ thử lại
        Err(DeadlineError::Cancelled) => return Err(OpError::Cancelled),
        Err(DeadlineError::Other(e)) => return Err(OpError::Other(e.to_string())),
    };
    if resp["status"] == 503 {
        let ra = resp["retry_after_ms"].as_f64();
        return Err(OpError::Net(NetError { code: "UNAVAILABLE", retry: true, cause: String::new(), retry_after: ra }));
    }
    Ok(resp) // sock tự đóng khi ra khỏi hàm
}

/// Chạy một kịch bản; nút Hủy (nếu có) phát user_ct sau cancel_ms.
async fn scenario(title: &str, host: &str, port: u16, req: Value, deadline_ms: u64, cancel_ms: u64) {
    *T0.lock().unwrap() = Some(Instant::now());
    println!("[client] === {title} ===");
    let user_ct = CancelToken::new();
    if cancel_ms > 0 {
        let button = user_ct.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(cancel_ms)).await;
            button.cancel(); // nút Hủy
        });
    }
    let tries = Cell::new(0);
    let op = |ct: CancelToken| lookup_once(host, port, &req, deadline_ms, ct, &tries);
    match retry_async(op, 5, 200, 5000, user_ct.clone()).await {
        Ok(v) => log(&format!("Thành công: {}", String::from_utf8_lossy(&json_bytes(&v)))),
        Err(OpError::Cancelled) => log("Người dùng đã hủy (Cancelled) – không thử lại"),
        Err(OpError::Net(e)) => log(&format!("Thất bại, không thử lại: {} (retry={})", e.code, if e.retry { "True" } else { "False" })),
        Err(OpError::Timeout(m)) => log(&format!("Thất bại sau lần thử cuối: TIMEOUT ({m})")),
        Err(OpError::Other(m)) => log(&format!("Thất bại: {m}")),
    }
}

pub async fn run(args: &[String]) {
    let (host, port) = parse_endpoint(&args[..args.len().min(2)], "127.0.0.1", 9100);
    let bad: u16 = args.get(2).and_then(|t| t.trim().parse().ok()).unwrap_or(9101);
    scenario(
        "Kịch bản 1: hạn chót 1000 ms, thử lại tối đa 5 lần (base 200, cap 5000)",
        &host,
        port,
        json!({"op": "lookup", "term": "TCP"}),
        1000,
        0,
    )
    .await;
    scenario(
        "Kịch bản 2: server xử lý 3000 ms, hạn chót 5000 ms, người dùng bấm Hủy sau 500 ms",
        &host,
        port,
        json!({"op": "lookup", "term": "UDP", "delay_ms": 3000}),
        5000,
        500,
    )
    .await;
    scenario(
        &format!("Kịch bản 3: cổng {bad} không có server (REFUSED)"),
        &host,
        bad,
        json!({"op": "lookup", "term": "TCP"}),
        1000,
        0,
    )
    .await;
}
