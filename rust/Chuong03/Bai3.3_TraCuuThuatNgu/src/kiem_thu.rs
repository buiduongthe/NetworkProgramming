//! Client kiểm thử server tra cứu theo tập ca kiểm thử tối thiểu (Mục 3.5.3).
//!
//! Client này tự tạo dãy byte gửi đi (không dùng client chuẩn) để thực hiện các ca 1–7
//! của bảng ca kiểm thử, cộng một ca bổ sung cho thao tác không có trong đặc tả.
//! Mỗi ca mở một kết nối riêng tới server.

use crate::framing::{json_encode, recv_json, send_all, send_frame, send_json, show, FrameError};
use crate::netutil::{parse_endpoint, safe_connect};
use serde_json::{json, Value};
use std::io::{ErrorKind, Read};
use std::net::TcpStream;
use std::process;
use std::thread;
use std::time::Duration;

struct Tester {
    host: String,
    port: u16,
    results: Vec<bool>,
}

impl Tester {
    fn connect(&self) -> TcpStream {
        let s = safe_connect(&self.host, self.port, 3000).unwrap_or_else(|e| {
            eprintln!("[kiểm thử] Không kết nối được: {e}");
            process::exit(1);
        });
        s.set_read_timeout(Some(Duration::from_secs(5))).ok();
        s
    }

    fn report(&mut self, no: &str, name: &str, ok: bool, detail: String) {
        self.results.push(ok);
        let verdict = if ok { "ĐẠT" } else { "KHÔNG ĐẠT" };
        println!("[kiểm thử] Ca {no} – {name}: {verdict} ({detail})");
    }
}

/// Khung hoàn chỉnh (tiêu đề + nội dung) của một đối tượng JSON.
fn frame_of(obj: &Value) -> Vec<u8> {
    let data = json_encode(obj).into_bytes();
    let mut f = (data.len() as u32).to_be_bytes().to_vec();
    f.extend_from_slice(&data);
    f
}

fn lookup(s: &mut TcpStream, term: &str) -> Result<Value, FrameError> {
    send_json(s, &json!({"op": "lookup", "term": term}))?;
    recv_json(s)
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "127.0.0.1", 9100);
    let mut t = Tester { host, port, results: Vec::new() };
    println!("[kiểm thử] Server {}:{}", t.host, t.port);
    if let Err(e) = all_cases(&mut t) {
        println!("[kiểm thử] Lỗi bất ngờ: {e}");
    }
    let passed = t.results.iter().filter(|&&ok| ok).count();
    println!("[kiểm thử] Kết quả: {passed}/{} ca đạt", t.results.len());
}

fn all_cases(t: &mut Tester) -> Result<(), FrameError> {
    {
        // Ca 1, 2
        let mut s = t.connect();
        let r = lookup(&mut s, "TCP")?;
        let ok = r["status"] == 200 && r["term"] == "TCP";
        t.report("1", "thuật ngữ có trong từ điển", ok, format!("status {}", r["status"]));
        let r = lookup(&mut s, "Bắt tay ba bước")?;
        let ok = r["status"] == 200 && r["term"] == "Bắt tay ba bước";
        t.report("2", "thuật ngữ có dấu", ok, format!("status {}, term '{}'", r["status"], show(&r["term"])));
        send_json(&mut s, &json!({"op": "bye"}))?;
    } // ra khỏi khối: socket được đóng
    {
        // Ca 3: gửi từng byte, cách nhau 100 ms
        let mut s = t.connect();
        s.set_nodelay(true).ok();
        let data = frame_of(&json!({"op": "lookup", "term": "UDP"}));
        for i in 0..data.len() {
            send_all(&mut s, &data[i..i + 1])?;
            thread::sleep(Duration::from_millis(100));
        }
        let r = recv_json(&mut s)?;
        let ok = r["status"] == 200 && r["term"] == "UDP";
        let name = format!("gửi {} byte từng byte một", data.len());
        t.report("3", &name, ok, format!("status {}, term '{}'", r["status"], show(&r["term"])));
        send_json(&mut s, &json!({"op": "bye"}))?;
    }
    {
        // Ca 4: hai yêu cầu trong một lần SEND
        let mut s = t.connect();
        let mut both = frame_of(&json!({"op": "lookup", "term": "TCP"}));
        both.extend(frame_of(&json!({"op": "lookup", "term": "XYZ"})));
        send_all(&mut s, &both)?;
        let r1 = recv_json(&mut s)?;
        let r2 = recv_json(&mut s)?;
        let ok = r1["status"] == 200 && r2["status"] == 404;
        t.report("4", "hai yêu cầu trong một lần SEND", ok, format!("status {} rồi {}", r1["status"], r2["status"]));
        send_json(&mut s, &json!({"op": "bye"}))?;
    }
    {
        // Ca 5: độ dài 0xFFFFFFFF > MAX_FRAME
        let mut s = t.connect();
        send_all(&mut s, b"\xff\xff\xff\xff")?;
        let mut one = [0u8; 1];
        let closed = match s.read(&mut one) {
            Ok(0) => true,                                             // server đóng kết nối
            Err(e) if e.kind() == ErrorKind::ConnectionReset => true, // hoặc gửi RST
            _ => false,
        };
        let detail = if closed { "server đóng kết nối" } else { "không đóng" };
        t.report("5", "độ dài 4294967295 > MAX_FRAME", closed, detail.to_string());
    }
    {
        // Ca 6: nội dung không phải JSON
        let mut s = t.connect();
        send_frame(&mut s, "đây không phải JSON".as_bytes())?;
        let r = recv_json(&mut s)?;
        send_json(&mut s, &json!({"op": "list"}))?; // kết nối vẫn dùng được
        let r2 = recv_json(&mut s)?;
        let n = r2["terms"].as_array().map(|a| a.len()).unwrap_or(0);
        let ok = r["status"] == 400 && r2["status"] == 200;
        let detail = format!("status {}, sau đó list -> {}, {n} thuật ngữ", r["status"], r2["status"]);
        t.report("6", "nội dung không phải JSON", ok, detail);
        send_json(&mut s, &json!({"op": "xoa", "term": "TCP"}))?; // Ca bổ sung: thao tác sai
        let r = recv_json(&mut s)?;
        let ok = r["status"] == 400;
        t.report("bổ sung 1", "thao tác \"xoa\"", ok, format!("status {}: {}", r["status"], show(&r["error"])));
        send_json(&mut s, &json!([1, 2]))?; // Ca bổ sung 2: JSON hợp lệ nhưng không phải đối tượng
        let r = recv_json(&mut s)?;
        let ok = r["status"] == 400 && r["error"] == "JSON sai";
        t.report("bổ sung 2", "JSON [1,2] không phải đối tượng", ok, format!("status {}: {}", r["status"], show(&r["error"])));
        send_json(&mut s, &json!({"op": "bye"}))?;
    }
    {
        // Ca 7: đóng kết nối giữa khung – khai báo 50 byte nhưng chỉ gửi 10 byte rồi đóng
        let mut s = t.connect();
        let mut part = 50u32.to_be_bytes().to_vec();
        part.extend_from_slice(b"0123456789");
        send_all(&mut s, &part)?;
    }
    {
        let mut s = t.connect();
        let r = lookup(&mut s, "TCP")?;
        let ok = r["status"] == 200;
        t.report("7", "đóng giữa khung (14/54 byte)", ok, format!("client mới vẫn được phục vụ, status {}", r["status"]));
        send_json(&mut s, &json!({"op": "bye"}))?;
    }
    Ok(())
}
