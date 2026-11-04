//! Bài 8.4 – Client chat tối giản có TLS và xác thực token.
//!
//! Cài đặt Mã giả 8.6 phía client (CONNECT: SAFE_CONNECT_ASYNC rồi TLS_WRAP_ASYNC với server_name ← host;
//! Mục 8.4.5 Giáo trình Lập trình mạng), kế thừa Mã giả 5.5 (ChatClient: RECEIVE_LOOP, SEND_MESSAGE có khóa).
//! Client gửi hello kèm token, vào phòng "chung", gửi từng dòng đọc từ bàn phím, rồi gửi ping và ngắt khi nhận pong.
//! Tệp CA: nếu chỉ định, client tin cậy CA nội bộ đó; nếu không, dùng kho tin cậy của hệ điều hành.
//!
//! Chạy:  cargo run -- client <host> <port> <tệp_token> [tệp_CA]
use crate::khung::{recv_json_async, send_json_async};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use serde_json::{json, Value};
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

type Tls = TlsStream<TcpStream>;

fn log(text: &str) {
    println!("[client] {text}");
}

/// Sự kiện vòng nhận báo cho chương trình chính (thay cho các asyncio.Event của bản Python).
enum Event {
    Welcome(bool), // true: được chấp nhận; false: nhận lỗi
    Pong,
    Closed,
}

/// ChatClient: nửa ghi của luồng TLS nằm trong send_lock (mỗi lúc gửi một khung).
struct ChatClient {
    send_lock: Mutex<WriteHalf<Tls>>,
}

impl ChatClient {
    /// SEND_MESSAGE: khóa gửi.
    async fn send(&self, obj: Value) -> std::io::Result<()> {
        let mut w = self.send_lock.lock().await;
        send_json_async(&mut *w, &obj).await
    }
}

/// Cấu hình TLS phía client: tin cậy CA trong tệp, hoặc kho chứng chỉ của hệ điều hành; luôn kiểm tra
/// chuỗi chứng chỉ và tên máy (không bao giờ tắt). rustls chỉ hỗ trợ TLS 1.2 và 1.3 (tối thiểu TLS 1.2).
fn client_config(ca_file: Option<&str>) -> Result<Arc<ClientConfig>, String> {
    let mut roots = RootCertStore::empty();
    match ca_file {
        Some(f) => {
            for cert in CertificateDer::pem_file_iter(f).map_err(|e| format!("Không đọc được {f}: {e}"))? {
                roots.add(cert.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            }
        }
        None => {
            for cert in rustls_native_certs::load_native_certs().certs {
                let _ = roots.add(cert);
            }
        }
    }
    Ok(Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth()))
}

/// CONNECT của Mã giả 5.5 + 8.6: kết nối TCP, bọc TLS, gửi hello kèm token, chạy nền vòng nhận.
async fn connect(host: &str, port: u16, token: &str, config: Arc<ClientConfig>, events: mpsc::UnboundedSender<Event>) -> Result<ChatClient, String> {
    // raw ← AWAIT SAFE_CONNECT_ASYNC(host, port, 3000)
    let raw = match tokio::time::timeout(Duration::from_secs(3), TcpStream::connect((host, port))).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(format!("Không kết nối được: {e}")),
        Err(_) => return Err("Không kết nối được: hết thời gian chờ".into()),
    };
    // c.sock ← AWAIT TLS_WRAP_ASYNC(raw, CLIENT, server_name ← host)
    let name = ServerName::try_from(host.to_string()).map_err(|e| format!("Tên máy sai: {e}"))?;
    let tls = TlsConnector::from(config).connect(name, raw).await.map_err(|e| format!("Bắt tay TLS thất bại: {e}"))?;
    let (_, info) = tls.get_ref();
    let version = match info.protocol_version() {
        Some(rustls::ProtocolVersion::TLSv1_3) => "TLSv1.3".to_string(),
        Some(rustls::ProtocolVersion::TLSv1_2) => "TLSv1.2".to_string(),
        v => format!("{v:?}"),
    };
    let suite = info.negotiated_cipher_suite().map(|s| format!("{:?}", s.suite())).unwrap_or_default();
    log(&format!("Đã kết nối TLS tới {host}:{port}: {version}, {suite}"));
    log(&format!("Chứng chỉ server hợp lệ (chuỗi tới CA tin cậy), tên '{host}' khớp SAN"));
    let (r, w) = tokio::io::split(tls);
    let c = ChatClient { send_lock: Mutex::new(w) };
    c.send(json!({"type": "hello", "token": token})).await.map_err(|e| e.to_string())?;
    tokio::spawn(receive_loop(r, events)); // RUN_BACKGROUND: không AWAIT
    Ok(c)
}

/// RECEIVE_LOOP: in thông điệp nhận được, báo welcome/lỗi/pong cho chương trình chính.
async fn receive_loop(mut r: ReadHalf<Tls>, events: mpsc::UnboundedSender<Event>) {
    loop {
        let m = match recv_json_async(&mut r).await {
            Ok(m) => m,
            Err(_) => {
                log("Server đã đóng kết nối");
                let _ = events.send(Event::Closed);
                return;
            }
        };
        let s = |k: &str| m[k].as_str().unwrap_or("").to_string();
        match m["type"].as_str().unwrap_or("") {
            "welcome" => {
                let users: Vec<&str> = m["users"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
                log(&format!("Server chấp nhận: tên = {} (từ token), trực tuyến: {}", s("name"), users.join(", ")));
                let _ = events.send(Event::Welcome(true));
            }
            "error" => {
                log(&format!("Lỗi từ server: {} {}", m["code"], s("text")));
                let _ = events.send(Event::Welcome(false));
            }
            "sys" => log(&format!("* {}", s("text"))),
            "msg" => log(&format!("[{}] {}: {}", s("room"), s("from"), s("text"))),
            "pong" => {
                let _ = events.send(Event::Pong);
            }
            _ => {}
        }
    }
}

async fn session(args: &[String]) -> Result<(), String> {
    if args.len() < 3 {
        return Err("Cách dùng: cargo run -- client <host> <port> <tệp_token> [tệp_CA]".into());
    }
    let (host, token_file) = (&args[0], &args[2]);
    let port: u16 = args[1].parse().ok().filter(|p| *p >= 1).ok_or(format!("Cổng không hợp lệ: {}", args[1]))?;
    let ca_file = args.get(3).map(String::as_str);
    let token = std::fs::read_to_string(token_file).map_err(|e| format!("Không đọc được {token_file}: {e}"))?;
    let trust = match ca_file {
        Some(f) => format!("CA trong {f}"),
        None => "kho chứng chỉ của hệ điều hành".into(),
    };
    log(&format!("Tin cậy: {trust}; token: {token_file}"));
    let (tx, mut events) = mpsc::unbounded_channel();
    let c = connect(host, port, token.trim(), client_config(ca_file)?, tx).await?;
    // chờ welcome hoặc lỗi (tối đa 5 giây)
    match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
        Ok(Some(Event::Welcome(true))) => {}
        _ => return Ok(()), // bị từ chối hoặc server đóng: DISCONNECT khi c bị hủy
    }
    c.send(json!({"type": "join", "room": "chung"})).await.map_err(|e| e.to_string())?;
    // READ_CONSOLE: đọc các dòng cần gửi (đọc chặn chạy ở luồng riêng)
    let lines = tokio::task::spawn_blocking(|| {
        let mut s = String::new();
        let _ = std::io::stdin().read_to_string(&mut s);
        s
    })
    .await
    .unwrap_or_default();
    for line in lines.lines() {
        c.send(json!({"type": "msg", "room": "chung", "text": line})).await.map_err(|e| e.to_string())?;
    }
    c.send(json!({"type": "ping"})).await.map_err(|e| e.to_string())?; // pong về sau mọi tin trước đó
    let _ = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(ev) = events.recv().await {
            if matches!(ev, Event::Pong | Event::Closed) {
                break;
            }
        }
    })
    .await;
    drop(c); // DISCONNECT
    log("Đã ngắt kết nối");
    Ok(())
}

pub async fn run(args: &[String]) {
    if let Err(e) = session(args).await {
        log(&e);
        std::process::exit(1);
    }
}
