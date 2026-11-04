//! Server nhận tệp có kiểm tra mã băm (đi kèm để chạy thử PUT_FILE_ASYNC).
//!
//! Cài đặt Mã giả 3.9 (HANDLE_PUT, Mục 3.5.2) trên nền bất đồng bộ của Mã giả 5.1: ghi vào tệp
//! tạm .part, chỉ đổi tên khi mã băm SHA-256 khớp; kết nối đóng giữa chừng thì xóa tệp .part.
//! Tham số rate_kib giới hạn tốc độ đọc (KiB/s) để mô phỏng đường truyền chậm; 0 = không giới hạn.

use crate::khung_json_async::{parse_endpoint, recv_json_async, send_json_async, CancelToken, FrameError};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::Path;
use std::process;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};
use tokio::time::{sleep, Duration};

const MAX_FILE: u64 = 100 * 1_048_576;
const UPLOAD_DIR: &str = "uploads";

/// BASENAME: bỏ mọi thành phần thư mục, tách theo cả '/' và '\' (như Bài 3.4).
fn basename(name: &str) -> String {
    name.replace('\\', "/").rsplit('/').next().unwrap_or("").to_string()
}

/// Mã giả 3.9 – HANDLE_PUT, phiên bản bất đồng bộ.
async fn handle_put(conn: &mut TcpStream, req: &Value, rate_kib: u64, ct: &CancelToken) -> Result<(), FrameError> {
    let name = basename(req["name"].as_str().unwrap_or("")); // bỏ thư mục: chặn "../"
    let size = match req["size"].as_u64() {
        Some(n) if n <= MAX_FILE && !name.is_empty() && name != "." && name != ".." => n,
        _ => return send_json_async(conn, &json!({"status": 400, "error": "Sai tham số"}), ct).await,
    };
    println!("[server] Nhận yêu cầu put: {name}, {size} byte");
    send_json_async(conn, &json!({"status": 100}), ct).await?; // sẵn sàng nhận nội dung
    fs::create_dir_all(UPLOAD_DIR).await?;
    let tmp = Path::new(UPLOAD_DIR).join(format!("{name}.part"));
    let mut f = fs::File::create(&tmp).await?;
    let mut h = Sha256::new();
    let mut remaining = size;
    let mut buf = vec![0u8; 65536];
    while remaining > 0 {
        let want = remaining.min(65536) as usize;
        let n = match conn.read(&mut buf[..want]).await {
            Ok(0) => {
                drop(f);
                let _ = fs::remove_file(&tmp).await;
                let got = size - remaining;
                println!("[server] Kết nối đóng khi mới nhận {got}/{size} byte: xóa {}", tmp.display());
                return Err(FrameError::ConnectionClosed(got as usize));
            }
            Ok(n) => n,
            Err(e) => {
                // lỗi khác (kết nối bị đặt lại...): cũng xóa tệp tạm rồi báo tiếp
                drop(f);
                let _ = fs::remove_file(&tmp).await;
                return Err(e.into());
            }
        };
        f.write_all(&buf[..n]).await?;
        h.update(&buf[..n]);
        remaining -= n as u64;
        if rate_kib > 0 {
            // mô phỏng đường truyền chậm
            sleep(Duration::from_secs_f64(n as f64 / (rate_kib as f64 * 1024.0))).await;
        }
    }
    f.flush().await?;
    drop(f);
    if Some(format!("{:x}", h.finalize()).as_str()) == req["sha256"].as_str() {
        fs::rename(&tmp, Path::new(UPLOAD_DIR).join(&name)).await?;
        println!("[server] Đã lưu {UPLOAD_DIR}/{name}, mã băm khớp (200)");
        send_json_async(conn, &json!({"status": 200, "saved": name}), ct).await
    } else {
        let _ = fs::remove_file(&tmp).await;
        println!("[server] Mã băm không khớp (422)");
        send_json_async(conn, &json!({"status": 422, "error": "Mã băm không khớp"}), ct).await
    }
}

async fn session(mut conn: TcpStream, peer: SocketAddr, rate_kib: u64) {
    let addr = peer.to_string();
    println!("[server] Client kết nối: {addr}");
    let ct = CancelToken::new();
    let result: Result<(), FrameError> = async {
        loop {
            let req = match recv_json_async(&mut conn, &ct).await {
                Ok(v) => v,
                Err(FrameError::ConnectionClosed(0)) => return Ok(()),
                Err(e) => return Err(e),
            };
            match req["op"].as_str() {
                Some("put") => handle_put(&mut conn, &req, rate_kib, &ct).await?,
                Some("bye") => return Ok(()),
                _ => send_json_async(&mut conn, &json!({"status": 400, "error": "Sai thao tác"}), &ct).await?,
            }
        }
    }
    .await;
    if let Err(e) = result {
        println!("[server] Lỗi với {addr}: {e}");
    }
    drop(conn);
    println!("[server] Client {addr} đã đóng kết nối");
}

pub async fn run(args: &[String]) {
    let (host, port) = parse_endpoint(&args[..args.len().min(2)], "0.0.0.0", 9100);
    let rate_kib: u64 = args.get(2).map(|t| t.trim().parse().unwrap_or(0)).unwrap_or(0);
    let addr: SocketAddr = match tokio::net::lookup_host((host.as_str(), port)).await.map(|mut it| it.next()) {
        Ok(Some(a)) => a,
        _ => {
            eprintln!("[server] Địa chỉ không hợp lệ: {host}");
            process::exit(1);
        }
    };
    // SOCKET + SETOPT(REUSEADDR) [+ SO_RCVBUF nhỏ] + BIND + LISTEN trên socket đã cấu hình
    let listener = (|| {
        let sock = if addr.is_ipv4() { TcpSocket::new_v4()? } else { TcpSocket::new_v6()? };
        sock.set_reuseaddr(true)?;
        if rate_kib > 0 {
            sock.set_recv_buffer_size(65536)?; // bộ đệm nhận nhỏ: mô phỏng đường truyền chậm
        }
        sock.bind(addr)?;
        sock.listen(16)
    })();
    let listener = match listener {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[server] Không mở được {host}:{port}: {e}");
            process::exit(1);
        }
    };
    println!("[server] Đang lắng nghe tại {host}:{port}, giới hạn đọc {rate_kib} KiB/s");
    loop {
        tokio::select! {
            r = listener.accept() => if let Ok((conn, peer)) = r {
                tokio::spawn(session(conn, peer, rate_kib));
            },
            _ = tokio::signal::ctrl_c() => {
                println!("[server] Dừng server");
                return;
            }
        }
    }
}
