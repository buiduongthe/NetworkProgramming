//! Bài 7.5 – Dịch vụ gRPC tra cứu thuật ngữ (phía server, tonic).
//!
//! Cài đặt Mã giả 7.6, phần CLASS GlossaryService (Mục 7.4.3, Giáo trình Lập trình mạng):
//! Lookup (unary, NOT_FOUND khi không có) và Watch (luồng từ server, dừng khi client hủy).
//! Kế thừa: Mã giả 3.7 – nhánh "lookup" của PROCESS_REQUEST thành phương thức Lookup.
//! Trait cơ sở `Glossary` do tonic-build sinh từ tra_cuu.proto (build.rs).
//!
//! Chạy:  cargo run -- server [host] [port] [độ_trễ_ms]   (mặc định 0.0.0.0 5001 0)
use crate::pb::glossary_server::{Glossary, GlossaryServer};
use crate::pb::{ImportSummary, LookupRequest, Term, WatchRequest};
use crate::{exit_with, int_arg, port_arg};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tonic::{Request, Response, Status, Streaming};

const CHANGES: [&str; 5] = ["HTTP/2", "QUIC", "HTTP/3", "gRPC", "HTTPS"]; // dòng thay đổi mô phỏng

fn dict(term: &str) -> Option<&'static str> {
    match term {
        "TCP" => Some("Giao thức vận chuyển hướng kết nối"),
        "UDP" => Some("Giao thức vận chuyển không kết nối"),
        _ => None,
    }
}

/// Khi lời gọi bị hủy (client hết hạn chót), tonic hủy bỏ tác vụ xử lý; giá trị này bị hủy theo
/// và in thông báo. Gọi `disarm()` khi xử lý xong bình thường.
struct CancelGuard {
    term: String,
    armed: bool,
}

impl CancelGuard {
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        if self.armed {
            println!("[server] Lookup('{}') bị hủy: client đã hết hạn chót", self.term);
        }
    }
}

/// CLASS GlossaryService EXTENDS GlossaryBase
pub struct GlossaryService {
    delay_ms: u64, // dùng để thử hạn chót (deadline)
}

#[tonic::async_trait]
impl Glossary for GlossaryService {
    async fn lookup(&self, req: Request<LookupRequest>) -> Result<Response<Term>, Status> {
        let term = req.into_inner().term;
        if self.delay_ms > 0 {
            let guard = CancelGuard { term: term.clone(), armed: true };
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await; // server cố ý chậm
            guard.disarm();
        }
        let Some(text) = dict(&term) else {
            println!("[server] Lookup('{term}') -> NOT_FOUND");
            return Err(Status::not_found(format!("Không có {term}"))); // RAISE RpcError(NOT_FOUND, …)
        };
        println!("[server] Lookup('{term}') -> OK");
        Ok(Response::new(Term { term, text: text.into(), tags: vec![] }))
    }

    type WatchStream = Pin<Box<dyn Stream<Item = Result<Term, Status>> + Send>>;

    async fn watch(&self, req: Request<WatchRequest>) -> Result<Response<Self::WatchStream>, Status> {
        let prefix = req.into_inner().prefix;
        println!("[server] Watch(prefix='{prefix}') bắt đầu");
        let (stream, rx) = mpsc::channel(4); // stream.write(t) = gửi vào kênh, tonic chuyển thành khung HTTP/2
        tokio::spawn(async move {
            for term in CHANGES {
                // CHANGES_FEED(): mỗi 0,5 giây có một thuật ngữ mới
                tokio::time::sleep(Duration::from_millis(500)).await;
                if stream.is_closed() {
                    return; // client đã hủy
                }
                if term.starts_with(&prefix) {
                    let t = Term { term: term.into(), text: format!("Thuật ngữ mới: {term}"), tags: vec!["moi".into()] };
                    if stream.send(Ok(t)).await.is_err() {
                        return;
                    }
                    println!("[server] Watch: gửi {term}");
                }
            }
            println!("[server] Watch kết thúc");
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    // Import và Sync có trong hợp đồng nhưng ngoài phạm vi Mã giả 7.6
    async fn import(&self, _req: Request<Streaming<Term>>) -> Result<Response<ImportSummary>, Status> {
        Err(Status::unimplemented("Chưa cài đặt"))
    }

    type SyncStream = Pin<Box<dyn Stream<Item = Result<Term, Status>> + Send>>;

    async fn sync(&self, _req: Request<Streaming<Term>>) -> Result<Response<Self::SyncStream>, Status> {
        Err(Status::unimplemented("Chưa cài đặt"))
    }
}

pub async fn run(args: &[String]) {
    let host = args.first().map(String::as_str).unwrap_or("0.0.0.0");
    let port = port_arg(args, 1, 5001);
    let delay_ms = int_arg(args, 2, 0);
    let addr = format!("{host}:{port}")
        .parse()
        .unwrap_or_else(|_| exit_with(&format!("Địa chỉ không hợp lệ: {host}:{port}")));
    let server = tonic::transport::Server::builder()
        .add_service(GlossaryServer::new(GlossaryService { delay_ms }))
        // HTTP/2 không TLS (h2c) cho phòng thí nghiệm; dừng gọn khi nhấn Ctrl+C
        .serve_with_shutdown(addr, async {
            let _ = tokio::signal::ctrl_c().await;
        });
    println!("[server] Dịch vụ gRPC glossary.v1.Glossary tại {host}:{port} (độ trễ Lookup {delay_ms} ms)");
    if let Err(e) = server.await {
        exit_with(&format!("Không chạy được server: {e}"));
    }
    println!("[server] Dừng server");
}
