//! Server tra cứu UDP xác thực địa chỉ nguồn bằng mã thách thức (cookie) không trạng thái.
//!
//! Cài đặt Mã giả 4.8 (Mục 4.6.1, Giáo trình Lập trình mạng): COOKIE, HANDLE_DATAGRAM.
//! Kế thừa: Mã giả 4.3 (server tra cứu UDP) – thay thân vòng lặp bằng HANDLE_DATAGRAM.
//! Cookie = HMAC-SHA256(KEY, "ip:port:w"), w là khung thời gian 1 phút; server không lưu trạng thái.
//! Với địa chỉ chưa xác thực, server không gửi phản hồi lớn hơn MAX_RATIO lần yêu cầu mà gửi thách thức 401
//! (chỉ khi chính thách thức không vượt MAX_RATIO lần yêu cầu); datagram quá lớn, sai định dạng bị bỏ im lặng.

use crate::endpoint::parse_endpoint;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_DGRAM: usize = 1200;
const MAX_RATIO: usize = 3;

/// Dữ liệu dùng chung của server: khóa bí mật và từ điển.
struct Server {
    key: [u8; 32],
    dict: Map<String, Value>,
}

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

/// PROCESS_REQUEST (kế thừa nguyên vẹn từ server tra cứu): nghiệp vụ, độc lập với socket.
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

fn to_bytes(obj: &Value) -> Vec<u8> {
    obj.to_string().into_bytes()
}

/// HMAC-SHA256 theo RFC 2104: H((K ⊕ opad) ‖ H((K ⊕ ipad) ‖ msg)), khối 64 byte.
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key)); // khóa dài hơn một khối: băm trước
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = Sha256::new().chain_update(ipad).chain_update(msg).finalize();
    Sha256::new().chain_update(opad).chain_update(inner).finalize().into()
}

/// SAFE_EQUAL – so sánh thời gian hằng: thời gian xử lý không phụ thuộc vị trí byte khác nhau đầu tiên.
fn safe_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Mã giả 4.8, dòng 2–5: mã thách thức gắn với địa chỉ (ip, cổng) và khung thời gian w.
fn cookie(key: &[u8], addr: SocketAddr, w: u64) -> String {
    let msg = format!("{}:{}:{w}", addr.ip(), addr.port());
    hex(&hmac_sha256(key, msg.as_bytes()))
}

/// Mã giả 4.8 – HANDLE_DATAGRAM(s, data, addr).
fn handle_datagram(s: &UdpSocket, srv: &Server, data: &[u8], addr: SocketAddr) {
    if data.len() > MAX_DGRAM {
        println!("[server] Bỏ im lặng datagram {} byte > MAX_DGRAM từ {addr}", data.len()); // quá lớn: bỏ
        return;
    }
    // TRY JSON_DECODE … CATCH JsonError → RETURN; IF NOT IS_OBJECT(req) THEN RETURN – bỏ im lặng
    let req: Value = match std::str::from_utf8(data).ok().and_then(|t| serde_json::from_str::<Value>(t).ok()) {
        Some(v) if v.is_object() => v,
        _ => {
            println!("[server] Bỏ im lặng datagram sai định dạng từ {addr}");
            return;
        }
    };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let w = now / 60; // FLOOR(NOW_SECONDS() / 60)
    let c = req.get("cookie");
    // ok ← SAFE_EQUAL(req.cookie, COOKIE(addr, w)) OR SAFE_EQUAL(req.cookie, COOKIE(addr, w − 1))
    let ok = match c.and_then(|v| v.as_str()) {
        Some(c) => {
            safe_equal(c.as_bytes(), cookie(&srv.key, addr, w).as_bytes())
                || safe_equal(c.as_bytes(), cookie(&srv.key, addr, w.wrapping_sub(1)).as_bytes())
        }
        None => false,
    };
    let state = match c {
        None | Some(Value::Null) => "không có",
        _ if ok => "hợp lệ",
        _ => "không hợp lệ",
    };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    println!("[server] Nhận {} byte từ {addr} (id={id}, cookie: {state})", data.len());
    let mut resp = process_request(&req, &srv.dict); // resp ← PROCESS_REQUEST(req); resp.id ← req.id
    resp["id"] = id.clone();
    let mut out = to_bytes(&resp);
    if !ok && out.len() > MAX_RATIO * data.len() {
        let ch = json!({"id": id, "status": 401, "cookie": cookie(&srv.key, addr, w)});
        let small = to_bytes(&ch); // phản hồi nhỏ
        if small.len() > MAX_RATIO * data.len() {
            // chính thách thức cũng vượt MAX_RATIO lần yêu cầu: không gửi gì
            println!(
                "[server]   -> phản hồi {} byte, thách thức {} byte đều > {MAX_RATIO} × {} byte: bỏ im lặng",
                out.len(),
                small.len(),
                data.len()
            );
            return;
        }
        println!(
            "[server]   -> phản hồi {} byte > {MAX_RATIO} × {} byte: gửi thách thức 401 ({} byte)",
            out.len(),
            data.len(),
            small.len()
        );
        out = small;
    } else {
        println!("[server]   -> gửi phản hồi status={} ({} byte)", resp["status"], out.len());
    }
    if let Err(e) = s.send_to(&out, addr) {
        // dòng 17
        println!("[server] Lỗi gửi tới {addr}: {e}");
    }
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9100);
    let mut key = [0u8; 32]; // Mã giả 4.8, dòng 1: KEY ← RANDOM_BYTES(32)
    getrandom::fill(&mut key).expect("không lấy được số ngẫu nhiên từ hệ điều hành");
    let srv = Server { key, dict: load_dict("glossary.json") };
    let s = UdpSocket::bind((host.as_str(), port)).unwrap_or_else(|e| {
        eprintln!("[server] Không mở được {host}:{port}: {e}");
        process::exit(1);
    });
    println!(
        "[server] Đang lắng nghe UDP tại {host}:{port}, từ điển {} thuật ngữ, MAX_RATIO = {MAX_RATIO}",
        srv.dict.len()
    );
    let mut buf = vec![0u8; 65535];
    loop {
        let (n, addr) = match s.recv_from(&mut buf) {
            Ok(v) => v,
            Err(e) if e.kind() == ErrorKind::ConnectionReset => continue, // Windows: ICMP port unreachable
            Err(e) => {
                println!("[server] Lỗi nhận: {e}");
                continue;
            }
        };
        handle_datagram(&s, &srv, &buf[..n], addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kiểm tra HMAC-SHA256 tự viết với ca thử số 2 của RFC 4231 (chạy: cargo test).
    #[test]
    fn hmac_rfc4231_case2() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(hex(&mac), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }
}
