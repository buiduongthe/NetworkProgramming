//! Bài 8.4 – Server chat tối giản có TLS và xác thực token.
//!
//! Cài đặt Mã giả 8.6 phía server (TLS_SESSION, VERIFY_JWT trong hello; Mục 8.4.5 Giáo trình Lập trình mạng),
//! kế thừa Mã giả 6.5–6.6 (CHAT_SESSION, BROADCAST, WRITER – bản rút gọn: hello, join, msg, ping).
//! Cấu hình: TLS_CERT, TLS_KEY (tệp PEM của chứng chỉ và khóa riêng), CHAT_JWT_KEY (khóa ký JWT).
//! Tên người dùng lấy từ claim sub của token đã kiểm tra.
//!
//! Chạy:  cargo run -- server [host] [port]      (mặc định 0.0.0.0 9200)
use crate::jwt_hs256::{secret, verify_jwt, ISSUER};
use crate::khung::{recv_json_async, send_json_async};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

type Tls = TlsStream<TcpStream>;

/// Client { name, outbox ← BOUNDED_QUEUE(256) }; kill: tín hiệu CLOSE(c.sock) khi client chậm.
struct Client {
    name: String,
    outbox: mpsc::Sender<Option<Value>>, // None: báo tác vụ ghi kết thúc
    kill: watch::Sender<bool>,
}

/// users: tên → Client; rooms: tên phòng → tập tên; bảo vệ bằng khóa (tokio chạy nhiều luồng).
#[derive(Default)]
struct Tables {
    users: HashMap<String, Arc<Client>>,
    rooms: HashMap<String, HashSet<String>>,
}
static LOCK: LazyLock<Mutex<Tables>> = LazyLock::new(|| Mutex::new(Tables::default()));

fn log(text: &str) {
    println!("[server] {text}");
}

/// LOAD_CERT_AND_KEY(ENV("TLS_CERT"), …) và cấu hình TLS phía server (rustls: TLS 1.2 và 1.3).
fn load_cert_and_key() -> Result<Arc<ServerConfig>, String> {
    let cert_path = std::env::var("TLS_CERT").map_err(|_| "Thiếu biến môi trường TLS_CERT")?;
    let key_path = std::env::var("TLS_KEY").map_err(|_| "Thiếu biến môi trường TLS_KEY")?;
    let certs: Vec<CertificateDer> = CertificateDer::pem_file_iter(&cert_path)
        .and_then(|it| it.collect())
        .map_err(|e| format!("Không đọc được {cert_path}: {e}"))?;
    let key = PrivateKeyDer::from_pem_file(&key_path).map_err(|e| format!("Không đọc được {key_path}: {e}"))?;
    let config = ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key).map_err(|e| e.to_string())?;
    Ok(Arc::new(config))
}

fn enqueue(c: &Client, msg: Value) {
    if let Err(mpsc::error::TrySendError::Full(_)) = c.outbox.try_send(Some(msg)) {
        log(&format!("Ngắt client chậm {}", c.name)); // client chậm: ngắt kết nối
        c.kill.send_replace(true);
    }
}

fn send_to(name: &str, msg: Value, sender: Option<&Client>) {
    let c = LOCK.lock().unwrap().users.get(name).cloned();
    match (c, sender) {
        (Some(c), _) => enqueue(&c, msg),
        (None, Some(s)) => enqueue(s, json!({"type": "error", "code": 404, "text": "Không có người nhận"})),
        _ => {}
    }
}

fn broadcast(room: &str, msg: Value, sender: Option<&Client>) {
    let targets = LOCK.lock().unwrap().rooms.get(room).cloned().unwrap_or_default(); // sao chép danh sách thành viên
    if let Some(s) = sender {
        if !targets.contains(&s.name) {
            enqueue(s, json!({"type": "error", "code": 403, "text": "Chưa tham gia phòng"}));
            return;
        }
    }
    for name in &targets {
        send_to(name, msg.clone(), None);
    }
}

/// Tác vụ ghi duy nhất của mỗi client.
async fn writer_task(c: Arc<Client>, mut outbox: mpsc::Receiver<Option<Value>>, mut w: WriteHalf<Tls>) {
    let mut kill = c.kill.subscribe();
    loop {
        let msg = tokio::select! {
            m = outbox.recv() => m,
            _ = kill.wait_for(|&k| k) => break,
        };
        let Some(Some(msg)) = msg else { break };
        if send_json_async(&mut w, &msg).await.is_err() {
            c.kill.send_replace(true);
            break;
        }
    }
}

/// Lời chào không hợp lệ: mã và lý do.
struct HelloError(u16, String);

fn now_ts() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
}

/// Tên loại lỗi để ghi nhật ký khi kết nối đóng trước khi đăng nhập.
fn kind_name(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::UnexpectedEof => "ConnectionClosed".into(),
        io::ErrorKind::InvalidData => "ProtocolError".into(),
        k => format!("{k:?}"),
    }
}

/// CHAT_SESSION (Mã giả 6.5) chạy trên kết nối đã bọc TLS; hello mang token thay cho tên.
async fn chat_session(tls: Tls, addr: String) {
    let (mut r, mut w): (ReadHalf<Tls>, WriteHalf<Tls>) = tokio::io::split(tls);
    // h ← AWAIT WITH_TIMEOUT(10000, RECV_JSON_ASYNC(tls))
    let h = match tokio::time::timeout(Duration::from_secs(10), recv_json_async(&mut r)).await {
        Ok(Ok(h)) => h,
        Ok(Err(e)) => return log(&format!("Kết nối {addr} đóng trước khi đăng nhập: {}", kind_name(&e))),
        Err(_) => return log(&format!("Kết nối {addr} đóng trước khi đăng nhập: Timeout")),
    };
    let registered = (|| {
        if h["type"] != "hello" {
            return Err(HelloError(400, "Thông điệp đầu phải là hello".into()));
        }
        let key = secret("CHAT_JWT_KEY").map_err(|e| HelloError(500, e))?;
        // claims ← VERIFY_JWT(h.token, issuer ← ISSUER, audience ← "chat")
        let claims = verify_jwt(&h["token"], &key, ISSUER, "chat").map_err(|e| HelloError(401, format!("Token không hợp lệ: {e}")))?;
        let name = claims["sub"].as_str().unwrap().to_string();
        // c ← REGISTER(claims.sub, conn): tên lấy từ token
        let mut t = LOCK.lock().unwrap();
        if t.users.contains_key(&name) {
            return Err(HelloError(409, "Tên đã được sử dụng".into()));
        }
        let (tx, rx) = mpsc::channel(256); // BOUNDED_QUEUE(256)
        let (kill, _) = watch::channel(false);
        let c = Arc::new(Client { name: name.clone(), outbox: tx, kill });
        t.users.insert(name, Arc::clone(&c));
        let mut users: Vec<String> = t.users.keys().cloned().collect();
        users.sort();
        Ok((c, rx, users))
    })();
    let (c, rx, users) = match registered {
        Ok(x) => x,
        Err(HelloError(code, text)) => {
            // lời chào không hợp lệ
            log(&format!("Từ chối {addr}: {code} {text}"));
            let _ = send_json_async(&mut w, &json!({"type": "error", "code": code, "text": text})).await;
            return;
        }
    };
    log(&format!("{} đăng nhập từ {addr} (tên lấy từ claim sub)", c.name));
    let task = tokio::spawn(writer_task(Arc::clone(&c), rx, w));
    enqueue(&c, json!({"type": "welcome", "name": c.name, "users": users}));
    let mut kill = c.kill.subscribe();
    loop {
        let m = tokio::select! {
            m = tokio::time::timeout(Duration::from_secs(90), recv_json_async(&mut r)) => m,
            _ = kill.wait_for(|&k| k) => break,
        };
        let Ok(Ok(m)) = m else { break }; // kết thúc phiên bình thường hoặc do lỗi
        let room = m["room"].as_str().unwrap_or("").to_string();
        match m["type"].as_str().unwrap_or("") {
            "join" => {
                LOCK.lock().unwrap().rooms.entry(room.clone()).or_default().insert(c.name.clone());
                broadcast(&room, json!({"type": "sys", "text": format!("{} đã vào phòng {room}", c.name)}), None);
            }
            "msg" => {
                let msg = json!({"type": "msg", "from": c.name, "room": room, "text": m["text"], "ts": now_ts()});
                broadcast(&room, msg, Some(&c));
            }
            "ping" => enqueue(&c, json!({"type": "pong"})),
            _ => enqueue(&c, json!({"type": "error", "code": 400, "text": "Loại thông điệp không hỗ trợ"})),
        }
    }
    // UNREGISTER, đóng hộp thư đi, chờ tác vụ ghi gửi nốt (tối đa 1 giây)
    {
        let mut t = LOCK.lock().unwrap();
        t.users.remove(&c.name);
        for members in t.rooms.values_mut() {
            members.remove(&c.name);
        }
    }
    let _ = c.outbox.try_send(None); // None: báo tác vụ ghi kết thúc
    let _ = tokio::time::timeout(Duration::from_secs(1), task).await;
    log(&format!("{} rời phiên", c.name));
}

/// Mã giả 8.6 – TLS_SESSION(conn, addr): thay SERVE_ASYNC; bắt tay TLS trước phiên chat.
async fn tls_session(conn: TcpStream, addr: String, acceptor: TlsAcceptor) {
    // tls ← AWAIT TLS_WRAP_ASYNC(conn, SERVER, cert, min ← TLS1_2), hạn chót bắt tay 10 giây
    let tls = match tokio::time::timeout(Duration::from_secs(10), acceptor.accept(conn)).await {
        Ok(Ok(tls)) => tls,
        Ok(Err(e)) => return log(&format!("Bắt tay TLS lỗi {addr}: {e}")), // CLOSE(conn): conn đã bị hủy
        Err(_) => return log(&format!("Bắt tay TLS lỗi {addr}: hết thời gian chờ")),
    };
    let (_, conn_info) = tls.get_ref();
    let version = match conn_info.protocol_version() {
        Some(rustls::ProtocolVersion::TLSv1_3) => "TLSv1.3".to_string(),
        Some(rustls::ProtocolVersion::TLSv1_2) => "TLSv1.2".to_string(),
        v => format!("{v:?}"),
    };
    let suite = conn_info.negotiated_cipher_suite().map(|s| format!("{:?}", s.suite())).unwrap_or_default();
    log(&format!("Bắt tay TLS thành công {addr}: {version}, {suite}"));
    chat_session(tls, addr).await; // tls thay cho conn
}

async fn serve(args: &[String]) -> Result<(), String> {
    // Mã giả 2.1 (rút gọn): tham số > APP_HOST/APP_PORT > mặc định
    let host = args.first().cloned().or(std::env::var("APP_HOST").ok()).unwrap_or("0.0.0.0".into());
    let port_text = args.get(1).cloned().or(std::env::var("APP_PORT").ok()).unwrap_or("9200".into());
    let port: u16 = port_text.parse().ok().filter(|p| *p >= 1).ok_or(format!("Cổng không hợp lệ: {port_text}"))?;
    let acceptor = TlsAcceptor::from(load_cert_and_key()?); // nạp một lần khi khởi động
    let srv = TcpListener::bind((host.as_str(), port)).await.map_err(|e| format!("Không mở được cổng {port}: {e}"))?;
    log(&format!("Đang lắng nghe (TLS ≥ 1.2) tại {host}:{port}"));
    loop {
        let Ok((conn, a)) = srv.accept().await else { continue };
        tokio::spawn(tls_session(conn, a.to_string(), acceptor.clone()));
    }
}

pub async fn run(args: &[String]) {
    tokio::select! {
        r = serve(args) => if let Err(e) = r { eprintln!("{e}"); std::process::exit(1) },
        _ = tokio::signal::ctrl_c() => log("Dừng server"),
    }
}
