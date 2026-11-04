//! Client tra cứu bất đồng bộ dùng khung JSON (minh họa Mã giả 5.1).
//!
//! Kế thừa Mã giả 3.8 (LOOKUP_CLIENT): đọc từng thuật ngữ từ bàn phím, gửi {op:"lookup"} và in
//! phản hồi; mọi lời gọi vào/ra được thay bằng phiên bản bất đồng bộ (Mục 5.2.4).
//! Chương trình in thêm 4 byte tiêu đề của mỗi khung và minh họa việc hủy một lời gọi nhận
//! đang chờ: sau khi hủy, trạng thái định khung không còn tin cậy nên kết nối bị đóng (Mục 5.4.1).

use crate::khung_json_async::{
    json_bytes, make_frame, parse_endpoint, recv_json_async, safe_connect_async, send_json_async, CancelToken, FrameError,
};
use serde_json::{json, Value};
use std::io;
use std::process;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout, Duration};

/// READ_CONSOLE bất đồng bộ: đọc bàn phím trên luồng phụ (spawn_blocking) để không chặn vòng lặp sự kiện.
async fn read_console() -> Option<String> {
    let line = tokio::task::spawn_blocking(|| {
        let mut s = String::new();
        match io::stdin().read_line(&mut s) {
            Ok(0) | Err(_) => None, // hết dữ liệu vào (Ctrl+D)
            Ok(_) => Some(s),
        }
    })
    .await
    .ok()
    .flatten()?;
    Some(line.trim_end_matches(['\r', '\n']).to_string())
}

fn show(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

async fn session(s: &mut TcpStream) -> Result<(), FrameError> {
    let ct = CancelToken::new(); // tín hiệu hủy của phiên
    while let Some(term) = read_console().await {
        let req = json!({"op": "lookup", "term": term});
        let frame = make_frame(&req);
        let head: Vec<String> = frame[..4].iter().map(|b| format!("{b:02X}")).collect();
        println!(
            "[client] Gửi khung {} byte, tiêu đề {}: {}",
            frame.len(),
            head.join(" "),
            String::from_utf8_lossy(&json_bytes(&req))
        );
        send_json_async(s, &req, &ct).await?;
        let resp = match timeout(Duration::from_millis(5000), recv_json_async(s, &ct)).await {
            // RECV_TIMEOUT 5000 ms
            Ok(r) => r?,
            Err(_) => return Err(FrameError::Protocol("Hết thời gian chờ phản hồi 5000 ms".to_string())),
        };
        if resp["status"] == 200 {
            println!("[client] {}: {}", show(&resp["term"]), show(&resp["text"]));
        } else {
            println!("[client] Lỗi {}: {}", resp["status"], show(&resp["error"]));
        }
    }
    // Minh họa hủy: chờ một khung mà server không gửi, tín hiệu hủy kích hoạt sau 500 ms
    let cts = CancelToken::new();
    let trigger = cts.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(500)).await;
        trigger.cancel(); // CANCEL(cts) sau 500 ms
    });
    match recv_json_async(s, &cts).await {
        Err(FrameError::Cancelled) => println!(
            "[client] RECV_JSON_ASYNC bị hủy sau 500 ms (Cancelled); trạng thái khung không xác định nên đóng kết nối"
        ),
        Ok(v) => println!("[client] Nhận bất ngờ: {v}"),
        Err(e) => return Err(e),
    }
    Ok(())
}

pub async fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "127.0.0.1", 9100);
    let mut s = match safe_connect_async(&host, port, 3000).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[client] Không kết nối được: {e}");
            process::exit(1);
        }
    };
    println!("[client] Đã kết nối {host}:{port}. Nhập thuật ngữ cần tra (Ctrl+D để kết thúc).");
    if let Err(e) = session(&mut s).await {
        println!("[client] Lỗi: {e}");
    }
    let _ = s.shutdown().await; // CLOSE: không dùng tiếp kết nối đã hủy
    drop(s);
    println!("[client] Đã đóng kết nối");
}
