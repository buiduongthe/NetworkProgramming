//! Tắt server an toàn (graceful shutdown) với tokio.
//!
//! Giao thức dòng văn bản: client gửi "LAM <ms>", server mô phỏng công việc dài <ms> mili giây
//! rồi trả "XONG <ms>". Khi nhận SIGTERM (lệnh kill, trình quản lý dịch vụ, Docker/Kubernetes)
//! hoặc SIGINT (Ctrl+C):
//!   1. Đóng socket nghe: ngừng nhận kết nối mới (client mới bị từ chối).
//!   2. Kết nối đang rỗi: gửi "BYE" rồi đóng ngay.
//!   3. Kết nối đang làm dở: cho làm xong yêu cầu hiện tại, trả kết quả, gửi "BYE", đóng.
//!   4. Quá hạn chờ (--han-cho-ms): hủy các yêu cầu còn dở, báo "HUY" cho client.
//! Hai "tín hiệu" bên trong chương trình là hai kênh tokio::sync::watch: `stop` (bắt đầu tắt) và
//! `kill` (hết hạn chờ). Mỗi kết nối dùng tokio::select! để chờ dòng lệnh HOẶC tín hiệu.
//! PID được ghi vào server.pid để có thể gửi tín hiệu bằng: kill -TERM $(cat server.pid)

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;

#[derive(Default)]
struct Stats {
    xong: AtomicUsize,
    huy: AtomicUsize,
}

fn log(text: &str) {
    println!("[server] {text}");
}

/// Chờ tới khi kênh watch mang giá trị true (tín hiệu đã bật).
/// wait_for trả về một "khóa đọc" (Ref); bỏ nó ngay trong hàm này để không giữ qua các .await khác.
async fn doi(rx: &mut watch::Receiver<bool>) {
    let _ = rx.wait_for(|v| *v).await;
}

async fn handle(conn: TcpStream, cid: usize, mut stop: watch::Receiver<bool>, mut kill: watch::Receiver<bool>, stats: Arc<Stats>) {
    let port = conn.peer_addr().map(|a| a.port()).unwrap_or(0);
    log(&format!("#{cid} kết nối từ cổng {port}"));
    let (r, mut w) = conn.into_split();
    let mut lines = BufReader::new(r).lines();
    loop {
        // chờ dòng lệnh kế tiếp HOẶC tín hiệu dừng (next_line an toàn khi bị bỏ giữa chừng)
        let line = tokio::select! {
            l = lines.next_line() => l,
            _ = doi(&mut stop) => {
                let _ = w.write_all("BYE server đang tắt\n".as_bytes()).await; // rỗi đúng lúc server tắt
                log(&format!("#{cid} đang rỗi -> gửi BYE, đóng"));
                return;
            }
        };
        let Ok(Some(line)) = line else {
            log(&format!("#{cid} client đóng kết nối"));
            return;
        };
        let parts: Vec<&str> = line.split_whitespace().collect();
        let job: u64 = match parts.as_slice() {
            ["LAM", ms] if ms.chars().all(|c| c.is_ascii_digit()) => ms.parse().unwrap_or(0),
            _ => {
                let _ = w.write_all("LOI lệnh không hợp lệ\n".as_bytes()).await;
                continue;
            }
        };
        log(&format!("#{cid} bắt đầu LAM {job}"));
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(job)) => {} // công việc dài
            _ = doi(&mut kill) => {                                     // quá hạn chờ tắt
                stats.huy.fetch_add(1, Ordering::SeqCst);
                log(&format!("#{cid} quá hạn chờ -> hủy LAM {job}"));
                let _ = w.write_all("HUY yêu cầu bị hủy vì server tắt\n".as_bytes()).await;
                return;
            }
        }
        let _ = w.write_all(format!("XONG {job}\n").as_bytes()).await;
        stats.xong.fetch_add(1, Ordering::SeqCst);
        let stopping = *stop.borrow(); // đọc rồi nhả khóa ngay, không giữ qua .await
        if stopping {
            // làm xong việc dở thì mới chào
            let _ = w.write_all("BYE server đang tắt\n".as_bytes()).await;
            log(&format!("#{cid} XONG {job} -> gửi BYE, đóng"));
            return;
        }
        log(&format!("#{cid} XONG {job}"));
    }
}

/// Chờ SIGTERM hoặc SIGINT (Ctrl+C); trả về tên tín hiệu. Windows chỉ có Ctrl+C.
async fn wait_signal() -> &'static str {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("đăng ký SIGTERM");
        tokio::select! {
            _ = term.recv() => "SIGTERM",
            _ = tokio::signal::ctrl_c() => "SIGINT",
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        "SIGINT"
    }
}

#[tokio::main]
pub async fn run(args: &[String]) {
    let mut pos = Vec::new();
    let mut han_cho_ms: u64 = 2000;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--han-cho-ms" {
            han_cho_ms = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(2000);
            i += 2;
        } else {
            pos.push(args[i].clone());
            i += 1;
        }
    }
    let host = pos.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let port: u16 = pos.get(1).and_then(|p| p.parse().ok()).unwrap_or(9000);
    let ln = TcpListener::bind((host.as_str(), port)).await.unwrap_or_else(|e| {
        log(&format!("Không mở được {host}:{port}: {e}"));
        std::process::exit(1)
    });
    let (stop_tx, stop_rx) = watch::channel(false);
    let (kill_tx, kill_rx) = watch::channel(false);
    let stats = Arc::new(Stats::default());
    std::fs::write("server.pid", std::process::id().to_string()).ok();
    log(&format!("Lắng nghe tại {host}:{port}, PID ghi vào server.pid"));

    let mut conns = JoinSet::new(); // các tác vụ đang phục vụ kết nối
    let mut cid = 0;
    let sig = loop {
        tokio::select! {
            r = ln.accept() => if let Ok((c, _)) = r {
                cid += 1;
                conns.spawn(handle(c, cid, stop_rx.clone(), kill_rx.clone(), Arc::clone(&stats)));
            },
            s = wait_signal() => break s,
        }
    };
    let t0 = Instant::now();
    drop(ln); // 1. ngừng nhận kết nối mới
    log(&format!("Nhận {sig}: ngừng nhận kết nối mới"));
    while conns.try_join_next().is_some() {} // bỏ các kết nối đã kết thúc
    log(&format!("Chờ tối đa {han_cho_ms} ms cho {} kết nối", conns.len()));
    let _ = stop_tx.send(true); // 2, 3. báo mọi kết nối: rỗi thì chào, đang làm thì làm xong rồi chào
    let all_done = async {
        while conns.join_next().await.is_some() {}
    };
    if tokio::time::timeout(Duration::from_millis(han_cho_ms), all_done).await.is_err() {
        // 4. quá hạn: hủy phần còn lại
        while conns.try_join_next().is_some() {}
        log(&format!("Hết hạn chờ: hủy {} kết nối", conns.len()));
        let _ = kill_tx.send(true);
        while conns.join_next().await.is_some() {}
    }
    std::fs::remove_file("server.pid").ok();
    log(&format!(
        "Đã tắt sau {:.1} s: xong {} yêu cầu, hủy {}",
        t0.elapsed().as_secs_f64(),
        stats.xong.load(Ordering::SeqCst),
        stats.huy.load(Ordering::SeqCst)
    ));
}
