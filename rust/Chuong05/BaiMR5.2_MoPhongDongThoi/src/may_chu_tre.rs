//! Máy chủ thử có độ trễ giả lập (chỉ chạy cục bộ), viết bằng tokio.
//!
//! Giao thức dòng văn bản trên TCP: client gửi "GET <k>\n", máy chủ chờ <trễ> mili giây (giả lập thời
//! gian xử lý/đường truyền) rồi trả "OK <k>\n". Mỗi kết nối là một tác vụ tokio nên phục vụ được nhiều
//! kết nối cùng lúc. Sau mỗi "đợt" yêu cầu (máy chủ rỗi 300 ms), in số yêu cầu và số kết nối đồng thời
//! lớn nhất của đợt – nhờ đó thấy được mỗi mô hình phía client gửi đồng thời bao nhiêu yêu cầu.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Default)]
struct State {
    active: u32,
    peak: u32,
    count: u32,
    total: u32,
    burst: u32,
}

fn log(text: &str) {
    println!("[server] {text}");
}

async fn report_when_idle(st: Arc<Mutex<State>>) {
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut s = st.lock().unwrap();
    if s.active == 0 && s.count > 0 {
        s.burst += 1;
        log(&format!("đợt {:>2}: {:>2} yêu cầu, đồng thời tối đa {:>2}", s.burst, s.count, s.peak));
        s.count = 0;
        s.peak = 0;
    }
}

async fn handle(conn: TcpStream, delay: Duration, st: Arc<Mutex<State>>) {
    {
        let mut s = st.lock().unwrap();
        s.active += 1;
        s.count += 1;
        s.total += 1;
        s.peak = s.peak.max(s.active);
    }
    let (r, mut w) = conn.into_split();
    let mut line = String::new();
    if BufReader::new(r).read_line(&mut line).await.is_ok() {
        tokio::time::sleep(delay).await; // giả lập độ trễ
        let k = line.trim().trim_start_matches("GET ").to_string();
        let _ = w.write_all(format!("OK {k}\n").as_bytes()).await;
    }
    drop(w); // đóng kết nối
    let idle = {
        let mut s = st.lock().unwrap();
        s.active -= 1;
        s.active == 0
    };
    if idle {
        tokio::spawn(report_when_idle(st));
    }
}

#[tokio::main(flavor = "current_thread")]
pub async fn run(host: &str, port: u16, delay_ms: u64) {
    let srv = TcpListener::bind((host, port)).await.unwrap_or_else(|e| {
        eprintln!("[server] Không mở được {host}:{port}: {e}");
        std::process::exit(1)
    });
    let st = Arc::new(Mutex::new(State::default()));
    log(&format!("lắng nghe {host}:{port}, mỗi yêu cầu trễ {delay_ms} ms"));
    loop {
        tokio::select! {
            r = srv.accept() => {
                if let Ok((conn, _)) = r {
                    tokio::spawn(handle(conn, Duration::from_millis(delay_ms), Arc::clone(&st)));
                }
            }
            _ = tokio::signal::ctrl_c() => {
                log(&format!("dừng; đã phục vụ {} yêu cầu", st.lock().unwrap().total));
                return;
            }
        }
    }
}
