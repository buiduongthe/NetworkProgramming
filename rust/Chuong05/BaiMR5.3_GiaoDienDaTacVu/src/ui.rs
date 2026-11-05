//! Chương trình giao diện web cục bộ: phục vụ trang giaodien.html và API cho trình duyệt.
//!   - mỗi tác vụ là một tác vụ tokio, xin lượt qua bộ giới hạn đồng thời, có kênh hủy (watch);
//!   - tiến trình được đẩy tới trình duyệt bằng Server-Sent Events (SSE, Bài 7.4) qua /api/events;
//!   - trình duyệt chỉ gọi API bằng fetch() bất đồng bộ nên luồng chính của trang không bao giờ chờ;
//!   - /api/bad: handler tải tệp xong mới trả lời (~1,2 s). Server KHÔNG bị đứng (handler là một tác vụ
//!     async, chờ mạng thì nhường cho tác vụ khác), nhưng trang gọi nó bằng XMLHttpRequest ĐỒNG BỘ nên
//!     luồng chính của trình duyệt bị chặn – tương tự "làm sai trên luồng giao diện" ở bản Tkinter.

use crate::http::{read_request, respond, Request};
use crate::tac_vu::{check_ports, download, lookup_names, Limiter, Progress};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};

/// Trang giao diện được nhúng vào tệp chạy lúc biên dịch (giống //go:embed của bản Go).
const PAGE_HTML: &str = include_str!("../giaodien.html");
const FILES: [(u64, u64); 8] = [(256, 60), (256, 90), (384, 70), (256, 120), (320, 50), (256, 100), (192, 80), (256, 40)];
const NAMES: [&str; 5] = ["router.lab", "may-in.lab", "camera-1.lab", "nas.lab", "khong-co.lab"];

// ---------- trạm phát sự kiện SSE ----------
/// Mỗi trình duyệt đang mở /api/events là một kênh có bộ đệm 1024 sự kiện.
#[derive(Default)]
struct Hub {
    subs: Mutex<Vec<mpsc::Sender<String>>>,
}

impl Hub {
    fn subscribe(&self) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel(1024);
        self.subs.lock().unwrap().push(tx);
        rx
    }

    fn publish(&self, ev: Value) {
        let text = ev.to_string();
        // try_send không bao giờ chờ: trình duyệt quá chậm thì bỏ sự kiện; kênh đã đóng thì gỡ khỏi danh sách
        self.subs.lock().unwrap().retain(|tx| !matches!(tx.try_send(text.clone()), Err(mpsc::error::TrySendError::Closed(_))));
    }
}

// ---------- quản lý tác vụ ----------
struct Task {
    cancel: watch::Sender<bool>, // gửi true để hủy
    done: bool,
}

struct Manager {
    tasks: Mutex<BTreeMap<u64, Task>>,
    next: Mutex<u64>,
    lim: Limiter,
    hub: Hub,
    test: String, // địa chỉ máy chủ thử host:port
    t0: Instant,
}

/// Một tác vụ: future trả về chuỗi kết quả, nhận hàm báo tiến trình.
type Job = Box<dyn FnOnce(Progress) -> Pin<Box<dyn Future<Output = io::Result<String>> + Send>> + Send>;

impl Manager {
    fn log(&self, text: &str) {
        let line = format!("{:6.2} s  {text}", self.t0.elapsed().as_secs_f64());
        println!("[ui] {line}");
        self.hub.publish(json!({"type": "log", "text": line}));
    }

    fn spawn(self: &Arc<Self>, name: String, job: Job) {
        let (cancel_tx, mut cancel_rx) = watch::channel(false);
        let id = {
            let mut n = self.next.lock().unwrap();
            *n += 1;
            *n
        };
        self.tasks.lock().unwrap().insert(id, Task { cancel: cancel_tx, done: false });
        self.hub.publish(json!({"type": "new", "id": id, "name": name}));
        let ticket = self.lim.enter(id); // xếp hàng ngay, theo thứ tự tạo tác vụ
        let m = Arc::clone(self);
        tokio::spawn(async move {
            let finish = |text: String| {
                if let Some(t) = m.tasks.lock().unwrap().get_mut(&id) {
                    t.done = true;
                }
                m.hub.publish(json!({"type": "done", "id": id, "text": text}));
                m.log(&format!("{name}: {text}"));
            };
            // chờ tới lượt, hoặc bị hủy khi đang chờ
            tokio::select! {
                _ = ticket => {}
                _ = cancel_rx.wait_for(|c| *c) => {
                    m.lim.leave(id);
                    finish("đã hủy (khi đang chờ lượt)".into());
                    return;
                }
            }
            m.hub.publish(json!({"type": "state", "id": id, "text": "đang chạy…"}));
            let hub = Arc::clone(&m);
            let progress: Progress = Box::new(move |pct, text| hub.hub.publish(json!({"type": "progress", "id": id, "pct": pct, "text": text})));
            let result = tokio::select! {
                r = job(progress) => Some(r),
                _ = cancel_rx.wait_for(|c| *c) => None, // future của tác vụ bị bỏ: socket đóng ngay
            };
            m.lim.release();
            match result {
                None => finish("đã hủy".into()),
                Some(Err(e)) => finish(format!("lỗi: {e}")),
                Some(Ok(text)) => finish(text),
            }
        });
    }

    fn cancel(&self, id: u64) -> bool {
        match self.tasks.lock().unwrap().get(&id) {
            Some(t) => {
                let _ = t.cancel.send(true);
                true
            }
            None => false,
        }
    }

    fn running(&self) -> Vec<u64> {
        self.tasks.lock().unwrap().iter().filter(|(_, t)| !t.done).map(|(id, _)| *id).collect()
    }
}

// ---------- các handler HTTP ----------
async fn route(m: &Arc<Manager>, req: Request, mut conn: TcpStream) -> io::Result<()> {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => respond(&mut conn, "200 OK", "text/html; charset=utf-8", PAGE_HTML.as_bytes()).await,
        ("GET", "/api/events") => events(m, conn).await,
        ("POST", "/api/start") => {
            let (host, port) = m.test.rsplit_once(':').map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(0))).unwrap();
            match req.get("kind") {
                "tai" => {
                    let n = req.num("n").clamp(1, FILES.len() as i64) as usize;
                    m.log(&format!("Bấm Tải tệp: {n} tệp, giới hạn đồng thời {}", m.lim.get()));
                    for &(kb, delay) in &FILES[..n] {
                        let addr = m.test.clone();
                        m.spawn(format!("Tải {kb} KB ({delay} ms/khối)"), Box::new(move |p| Box::pin(async move { download(&addr, kb, delay, &p).await })));
                    }
                }
                "cong" => {
                    let ports = [port, port + 1, port + 2, port + 3];
                    m.log(&format!("Bấm Kiểm tra cổng: {}, {}, {}, {}", ports[0], ports[1], ports[2], ports[3]));
                    m.spawn("Kiểm tra 4 cổng".into(), Box::new(move |p| Box::pin(async move { check_ports(&host, &ports, &p).await })));
                }
                "dns" => {
                    m.log(&format!("Bấm Tra cứu tên: {} tên qua UDP", NAMES.len()));
                    let addr = m.test.clone();
                    m.spawn(format!("Tra cứu {} tên", NAMES.len()), Box::new(move |p| Box::pin(async move { lookup_names(&addr, &NAMES, &p).await })));
                }
                _ => {}
            }
            ok204(&mut conn).await
        }
        ("POST", "/api/cancel") => {
            let id = req.num("id") as u64;
            if m.tasks.lock().unwrap().contains_key(&id) {
                m.log(&format!("Bấm Hủy ở tác vụ #{id}"));
                m.cancel(id);
            }
            ok204(&mut conn).await
        }
        ("POST", "/api/cancel_all") => {
            let run = m.running();
            m.log(&format!("Bấm Hủy tất cả: {} tác vụ", run.len()));
            for id in run {
                m.cancel(id);
            }
            ok204(&mut conn).await
        }
        ("POST", "/api/limit") => {
            let n = req.num("n").max(1) as usize;
            m.lim.set(n);
            m.log(&format!("Đổi giới hạn đồng thời: {n}"));
            ok204(&mut conn).await
        }
        ("POST", "/api/log") => {
            m.log(req.get("text")); // trang gửi số đo của nó để in vào nhật ký chung
            ok204(&mut conn).await
        }
        ("POST", "/api/bad") => {
            m.log("Bấm Làm SAI: trang gọi /api/bad ĐỒNG BỘ, handler tải 256 KB rồi mới trả lời");
            let t = Instant::now();
            let quiet: Progress = Box::new(|_, _| {});
            let ok = download(&m.test, 256, 80, &quiet).await.is_ok();
            let ms = t.elapsed().as_millis();
            m.log(&format!("/api/bad trả lời sau {ms} ms (các tác vụ khác vẫn chạy bình thường)"));
            respond(&mut conn, "200 OK", "application/json", json!({"ms": ms, "ok": ok}).to_string().as_bytes()).await
        }
        _ => respond(&mut conn, "404 Not Found", "text/plain; charset=utf-8", "không có".as_bytes()).await,
    }
}

async fn ok204(conn: &mut TcpStream) -> io::Result<()> {
    conn.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n").await
}

/// Luồng SSE: mỗi sự kiện một dòng "data: <json>", kết thúc bằng dòng trống.
async fn events(m: &Arc<Manager>, mut conn: TcpStream) -> io::Result<()> {
    conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n").await?;
    conn.write_all(": kết nối SSE\n\n".as_bytes()).await?;
    let mut rx = m.hub.subscribe();
    while let Some(ev) = rx.recv().await {
        conn.write_all(format!("data: {ev}\n\n").as_bytes()).await?; // lỗi ghi = trình duyệt đã đóng
    }
    Ok(())
}

async fn serve(m: Arc<Manager>, conn: TcpStream) {
    let (r, w) = conn.into_split();
    let mut br = BufReader::new(r);
    let Ok(req) = read_request(&mut br).await else { return };
    // ghép lại hai nửa để handler dùng một TcpStream
    let Ok(conn) = br.into_inner().reunite(w) else { return };
    let _ = route(&m, req, conn).await;
}

#[tokio::main]
pub async fn run(host: &str, port: u16, test: String) {
    let m = Arc::new(Manager {
        tasks: Mutex::new(BTreeMap::new()),
        next: Mutex::new(0),
        lim: Limiter::new(3),
        hub: Hub::default(),
        test,
        t0: Instant::now(),
    });
    let ln = TcpListener::bind((host, port)).await.unwrap_or_else(|e| {
        println!("[ui] lỗi: {e}");
        std::process::exit(1)
    });
    m.log(&format!("Giao diện: http://{host}:{port}/ (máy chủ thử {})", m.test));
    loop {
        tokio::select! {
            r = ln.accept() => if let Ok((c, _)) = r { tokio::spawn(serve(Arc::clone(&m), c)); },
            _ = tokio::signal::ctrl_c() => { m.log("dừng"); return; }
        }
    }
}
