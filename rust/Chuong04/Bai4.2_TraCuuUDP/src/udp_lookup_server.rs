//! Server tra cứu thuật ngữ trên UDP – Mã giả 4.3 (Mục 4.3.1, Giáo trình Lập trình mạng).
//!
//! Kế thừa: Mã giả 4.1 (vòng RECVFROM/SENDTO), server tra cứu TCP (PROCESS_REQUEST, DICT).
//! Mỗi yêu cầu và mỗi phản hồi là một datagram chứa đối tượng JSON; server chép trường id
//! của yêu cầu vào phản hồi và giới hạn phản hồi trong MAX_DGRAM = 1200 byte.
//! Tham số thứ ba (thêm cho bài thực hành): bỏ qua N datagram đầu tiên để mô phỏng mất gói.

use crate::endpoint::parse_endpoint;
use serde_json::{json, Map, Value};
use std::fs;
use std::io::ErrorKind;
use std::net::UdpSocket;
use std::process;

const MAX_DGRAM: usize = 1200; // Mã giả 4.3, dòng 1

/// DICT ← LOAD_JSON_FILE("glossary.json").
fn load_dict(path: &str) -> Map<String, Value> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("[server] Không đọc được {path}: {e}");
        process::exit(1);
    });
    serde_json::from_str(&text).unwrap_or_else(|e| {
        eprintln!("[server] {path} không phải đối tượng JSON: {e}");
        process::exit(1);
    })
}

/// PROCESS_REQUEST (kế thừa nguyên vẹn từ server tra cứu TCP): nghiệp vụ, độc lập với socket.
fn process_request(req: &Value, dict: &Map<String, Value>) -> Value {
    match req.get("op").and_then(|v| v.as_str()) {
        Some("lookup") => match req.get("term").and_then(|v| v.as_str()) {
            Some(term) if dict.contains_key(term) => json!({"status": 200, "term": term, "text": dict[term]}),
            _ => json!({"status": 404, "error": "Không có thuật ngữ"}),
        },
        Some("list") => json!({"status": 200, "terms": dict.keys().collect::<Vec<_>>()}),
        _ => json!({"status": 400, "error": "Sai thao tác"}),
    }
}

/// ENCODE(JSON_ENCODE(obj), "UTF-8"), dạng gọn, giữ nguyên chữ có dấu.
fn to_bytes(obj: &Value) -> Vec<u8> {
    obj.to_string().into_bytes()
}

/// In một giá trị JSON để đọc: chuỗi không kèm dấu nháy.
fn show(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(&args[..args.len().min(2)], "0.0.0.0", 9100); // dòng 3
    let drop_n: u64 = match args.get(2) {
        // mô phỏng mất gói (thêm)
        None => 0,
        Some(t) => t.trim().parse().unwrap_or_else(|_| {
            eprintln!("Số datagram bỏ qua phải là số nguyên không âm: {t}");
            process::exit(1);
        }),
    };
    let dict = load_dict("glossary.json");
    // dòng 4–5: SOCKET(INET, DGRAM) + BIND
    let s = UdpSocket::bind((host.as_str(), port)).unwrap_or_else(|e| {
        eprintln!("[server] Không mở được {host}:{port}: {e}");
        process::exit(1);
    });
    println!("[server] Đang lắng nghe UDP tại {host}:{port}, từ điển {} thuật ngữ", dict.len());
    if drop_n > 0 {
        println!("[server] Mô phỏng mất gói: bỏ qua {drop_n} datagram đầu tiên");
    }
    let mut count = 0u64;
    let mut buf = vec![0u8; 65535];
    loop {
        // dòng 6: LOOP
        let (n, addr) = match s.recv_from(&mut buf) {
            // dòng 7
            Ok(v) => v,
            Err(e) if e.kind() == ErrorKind::ConnectionReset => continue, // Windows: ICMP port unreachable
            Err(e) => {
                println!("[server] Lỗi nhận: {e}");
                continue;
            }
        };
        count += 1;
        if count <= drop_n {
            println!("[server] Bỏ qua datagram thứ {count} từ {addr} (mô phỏng mất gói)");
            continue;
        }
        // dòng 8–14: TRY JSON_DECODE … CATCH JsonError (gồm lỗi UTF-8 và JSON không phải đối tượng)
        let parsed: Option<Value> = std::str::from_utf8(&buf[..n])
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(t).ok())
            .filter(|v| v.is_object());
        let (req, mut resp) = match parsed {
            Some(req) => {
                let resp = process_request(&req, &dict);
                (req, resp)
            }
            None => (json!({"id": null}), json!({"status": 400, "error": "JSON sai"})),
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        resp["id"] = id.clone(); // dòng 15: ghép phản hồi với yêu cầu
        let mut out = to_bytes(&resp); // dòng 16
        if out.len() > MAX_DGRAM {
            // dòng 17–20
            resp = json!({"id": id, "status": 413, "error": "Quá lớn"});
            out = to_bytes(&resp);
        }
        if let Err(e) = s.send_to(&out, addr) {
            // dòng 21
            println!("[server] Lỗi gửi tới {addr}: {e}");
            continue;
        }
        println!(
            "[server] Yêu cầu id={id} op={} từ {addr} -> status={}, {} byte",
            show(&req["op"]),
            resp["status"],
            out.len()
        );
    }
}
