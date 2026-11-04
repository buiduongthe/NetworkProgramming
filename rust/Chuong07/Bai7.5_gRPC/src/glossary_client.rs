//! Bài 7.5 – Gọi dịch vụ gRPC tra cứu thuật ngữ (phía client, tonic).
//!
//! Cài đặt Mã giả 7.6, phần "Phía client" (Mục 7.4.3): Lookup có hạn chót (deadline), Watch nhận luồng.
//! Kiểu GlossaryClient do tonic-build sinh từ tra_cuu.proto. Lỗi được báo bằng mã trạng thái gRPC.
//!
//! Chạy:  cargo run -- client [host] [port] [hạn_chót_ms]   (mặc định 127.0.0.1 5001 2000)
use crate::pb::glossary_client::GlossaryClient;
use crate::pb::{LookupRequest, WatchRequest};
use crate::{int_arg, port_arg};
use prost::Message;
use std::time::Duration;
use tonic::transport::Channel;
use tonic::{Code, Request};

/// Tên mã trạng thái gRPC dạng chuẩn (NotFound → NOT_FOUND) để in giống các ngôn ngữ khác.
fn code_name(code: Code) -> String {
    let mut out = String::new();
    for (i, ch) in format!("{code:?}").chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_uppercase());
    }
    out
}

async fn lookup(c: &mut GlossaryClient<Channel>, term: &str, deadline_ms: u64) {
    let mut req = Request::new(LookupRequest { term: term.into() });
    req.set_timeout(Duration::from_millis(deadline_ms)); // deadline ← 2000: gửi kèm tiêu đề grpc-timeout
    // client tự đặt hạn chót: hết giờ thì bỏ lời gọi (luồng HTTP/2 bị hủy) và báo DEADLINE_EXCEEDED
    match tokio::time::timeout(Duration::from_millis(deadline_ms), c.lookup(req)).await {
        Ok(Ok(t)) => {
            let t = t.into_inner();
            println!("[client] Lookup('{term}') -> {}: {}", t.term, t.text);
        }
        // server tonic 0.12 báo hết hạn chót (grpc-timeout) bằng CANCELLED "Timeout expired", khác đặc tả
        // gRPC (DEADLINE_EXCEEDED); hai bên hết giờ gần như cùng lúc nên quy về cùng một kết quả
        Ok(Err(e)) if e.code() == Code::Cancelled && e.message() == "Timeout expired" => {
            println!("[client] Lookup('{term}') -> lỗi DEADLINE_EXCEEDED: Deadline Exceeded")
        }
        Ok(Err(e)) => println!("[client] Lookup('{term}') -> lỗi {}: {}", code_name(e.code()), e.message()),
        Err(_) => println!("[client] Lookup('{term}') -> lỗi DEADLINE_EXCEEDED: Deadline Exceeded"),
    }
}

pub async fn run(args: &[String]) {
    let host = args.first().map(String::as_str).unwrap_or("127.0.0.1");
    let port = port_arg(args, 1, 5001);
    let deadline_ms = int_arg(args, 2, 2000);
    let wire = LookupRequest { term: "TCP".into() }.encode_to_vec(); // mã hóa Protocol Buffers (Mục 7.4.2)
    let hex: Vec<String> = wire.iter().map(|b| format!("{b:02X}")).collect();
    println!("[client] LookupRequest{{term: \"TCP\"}} = {} ({} byte)", hex.join(" "), wire.len());
    // GRPC_CHANNEL (h2c, không TLS)
    let mut c = match GlossaryClient::connect(format!("http://{host}:{port}")).await {
        Ok(c) => c, // GlossaryClient(ch)
        Err(e) => {
            println!("[client] Không kết nối được {host}:{port}: {e}");
            return;
        }
    };
    println!("[client] Gọi Lookup với hạn chót {deadline_ms} ms");
    lookup(&mut c, "TCP", deadline_ms).await;
    lookup(&mut c, "XYZ", deadline_ms).await;
    println!("[client] Gọi Watch(prefix='HTTP'), nhận luồng:");
    match c.watch(WatchRequest { prefix: "HTTP".into() }).await {
        Ok(resp) => {
            let mut stream = resp.into_inner();
            loop {
                match stream.message().await {
                    // FOR EACH x IN AWAIT_STREAM(...)
                    Ok(Some(x)) => println!("[client]   {} – {}", x.term, x.text),
                    Ok(None) => break, // server kết thúc luồng
                    Err(e) => {
                        println!("[client] Watch -> lỗi {}: {}", code_name(e.code()), e.message());
                        break;
                    }
                }
            }
        }
        Err(e) => println!("[client] Watch -> lỗi {}: {}", code_name(e.code()), e.message()),
    }
    println!("[client] Kết thúc");
}
