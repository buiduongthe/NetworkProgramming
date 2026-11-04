//! Tải tệp bất đồng bộ có báo cáo tiến trình và hủy (tokio).
//!
//! Cài đặt Mã giả 5.6 PUT_FILE_ASYNC (Mục 5.3.4, Giáo trình Lập trình mạng).
//! Kế thừa Mã giả 3.10 (PUT_CLIENT): giữ nguyên giao thức và trình tự 4 bước; thêm async/await,
//! hàm báo tiến trình on_progress (tối đa ~10 lần/giây) và tín hiệu hủy ct.
//! Nếu tệp chưa tồn tại, chương trình tạo một tệp thử 4 MiB.

use crate::khung_json_async::{json_bytes, parse_endpoint, recv_json_async, send_json_async, CancelToken, FrameError, NetError};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};
use tokio::time::{sleep, timeout, Duration, Instant};

/// SHA-256 của tệp (bản chặn, chạy trên luồng phụ bằng spawn_blocking).
fn sha256_of_file(path: &str) -> io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// Mã giả 5.6 – PUT_FILE_ASYNC(s, path, on_progress, ct).
async fn put_file_async(s: &mut TcpStream, path: &str, on_progress: fn(u64, u64), ct: &CancelToken) -> Result<Value, FrameError> {
    let size = fs::metadata(path)?.len(); // FILE_SIZE(path)
    // AWAIT SHA256_OF_FILE_ASYNC(path, ct): tệp lớn tính lâu, chạy trên luồng phụ
    let p = path.to_string();
    let job = tokio::task::spawn_blocking(move || sha256_of_file(&p));
    let digest = tokio::select! {
        _ = ct.cancelled() => return Err(FrameError::Cancelled),
        r = job => r.map_err(|e| FrameError::Protocol(e.to_string()))??,
    };
    let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(); // BASENAME
    let hdr = json!({"op": "put", "name": name, "size": size, "sha256": digest});
    send_json_async(s, &hdr, ct).await?;
    if recv_json_async(s, ct).await?["status"] != 100 {
        return Err(FrameError::Protocol("Server từ chối".to_string()));
    }
    let mut f = tokio::fs::File::open(path).await?; // OPEN_FILE(path, READ)
    let mut buf = vec![0u8; 65536];
    let mut sent: u64 = 0;
    let mut last: Option<Instant> = None;
    loop {
        // AWAIT READ_ASYNC(f, 65536, ct)
        let n = tokio::select! {
            _ = ct.cancelled() => return Err(FrameError::Cancelled),
            r = f.read(&mut buf) => r?,
        };
        if n == 0 {
            break;
        }
        // AWAIT SEND_ALL_ASYNC(s, chunk, ct): điểm có thể hủy
        tokio::select! {
            _ = ct.cancelled() => return Err(FrameError::Cancelled),
            r = s.write_all(&buf[..n]) => r?,
        }
        sent += n as u64; // đã vào bộ đệm gửi (chưa chắc server đã nhận)
        if last.map_or(true, |t| t.elapsed() >= Duration::from_millis(100)) {
            // tối đa ~10 lần/giây
            on_progress(sent, size);
            last = Some(Instant::now());
        }
    }
    drop(f); // FINALLY CLOSE_FILE(f) – cũng xảy ra tự động khi hàm thoát sớm vì lỗi hoặc bị hủy
    let resp = recv_json_async(s, ct).await?; // server đã nhận đủ
    if resp["status"] == 200 {
        on_progress(size, size); // 100% chỉ sau khi server xác nhận
    }
    Ok(resp)
}

/// Tạo tệp thử nội dung giả ngẫu nhiên cố định (bộ sinh xorshift, hạt giống 5).
fn make_test_file(path: &str, mib: usize) -> io::Result<()> {
    let mut x: u64 = 5;
    let mut data = Vec::with_capacity(mib * 1_048_576);
    while data.len() < mib * 1_048_576 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        data.extend_from_slice(&x.to_le_bytes());
    }
    fs::write(path, data)?;
    println!("[client] Tạo tệp thử {path} ({mib} MiB)");
    Ok(())
}

/// on_progress: in số MiB đã gửi.
fn show_progress(sent: u64, size: u64) {
    let pct = if size > 0 { sent * 100 / size } else { 100 };
    println!(
        "[client] Tiến trình: {:.2}/{:.2} MiB ({pct}%)",
        sent as f64 / 1_048_576.0,
        size as f64 / 1_048_576.0
    );
}

/// SAFE_CONNECT_ASYNC (Mã giả 2.2 + AWAIT) với bộ đệm gửi nhỏ 64 KiB để số byte "đã gửi"
/// bám sát số byte server đã nhận (chỉ để minh họa tiến trình).
async fn connect_small_sndbuf(host: &str, port: u16, timeout_ms: u64) -> Result<TcpStream, NetError> {
    let addrs: Vec<_> = match tokio::net::lookup_host((host, port)).await {
        Ok(it) => it.collect(),
        Err(e) => return Err(NetError { code: "NOT_FOUND", retry: false, cause: e.to_string() }),
    };
    let mut last = String::new();
    for addr in addrs {
        let sock = if addr.is_ipv4() { TcpSocket::new_v4() } else { TcpSocket::new_v6() };
        let Ok(sock) = sock else { continue };
        let _ = sock.set_send_buffer_size(65536); // SO_SNDBUF 64 KiB
        match timeout(Duration::from_millis(timeout_ms), sock.connect(addr)).await {
            Ok(Ok(s)) => return Ok(s),
            Ok(Err(e)) => last = e.to_string(),
            Err(_) => last = format!("hết thời gian chờ {timeout_ms} ms"),
        }
    }
    Err(NetError { code: "UNREACHABLE", retry: true, cause: last })
}

pub async fn run(args: &[String]) {
    let (host, port) = parse_endpoint(&args[..args.len().min(2)], "127.0.0.1", 9100);
    let path = args.get(2).cloned().unwrap_or_else(|| "du_lieu_thu.bin".to_string());
    let cancel_ms: u64 = args.get(3).map(|t| t.trim().parse().unwrap_or(0)).unwrap_or(0);
    if !Path::new(&path).exists() {
        if let Err(e) = make_test_file(&path, 4) {
            eprintln!("[client] Không tạo được tệp thử: {e}");
            process::exit(1);
        }
    }
    let mut s = match connect_small_sndbuf(&host, port, 3000).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[client] Không kết nối được: {e}");
            process::exit(1);
        }
    };
    let note = if cancel_ms > 0 { format!(", sẽ bấm Hủy sau {cancel_ms} ms") } else { String::new() };
    println!("[client] Đã kết nối {host}:{port}, tải lên {path}{note}");
    let t0 = Instant::now();
    let cts = CancelToken::new(); // nguồn tín hiệu hủy của nút Hủy
    if cancel_ms > 0 {
        let button = cts.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(cancel_ms)).await;
            button.cancel(); // nút Hủy: CANCEL(cts)
        });
    }
    match put_file_async(&mut s, &path, show_progress, &cts).await {
        Ok(resp) => println!(
            "[client] Kết quả: {} sau {:.2} s",
            String::from_utf8_lossy(&json_bytes(&resp)),
            t0.elapsed().as_secs_f64()
        ),
        Err(FrameError::Cancelled) => println!(
            "[client] Đã hủy sau {:.2} s; đóng kết nối (trạng thái giao thức không còn tin cậy)",
            t0.elapsed().as_secs_f64()
        ),
        Err(e) => println!("[client] Lỗi: {e}"),
    }
    let _ = s.shutdown().await; // hủy giữa chừng: đóng kết nối (Mục 5.4.1)
}
