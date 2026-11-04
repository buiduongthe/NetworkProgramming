//! Client kiểm thử chức năng tải tệp (ca 8, 9 của tập ca kiểm thử, Mục 3.5.3).
//!
//! Client tự tạo khung tiêu đề và nội dung (không dùng client chuẩn) để thử: tên tệp chứa "../"
//! hoặc "..\", tên chỉ gồm thư mục, mã băm khai báo sai, kích thước vượt MAX_FILE và ngắt kết nối
//! khi đang gửi nội dung. Mỗi ca mở một kết nối riêng tới server.

use crate::framing::{recv_json, send_all, send_json, show, FrameError};
use crate::netutil::{parse_endpoint, safe_connect};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::net::TcpStream;
use std::process;
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

/// Nội dung thử: 10 lần dòng "Nội dung thử\n" (170 byte UTF-8).
fn data() -> Vec<u8> {
    "Nội dung thử\n".repeat(10).into_bytes()
}

/// Gửi tiêu đề put; nếu server sẵn sàng (100) thì gửi nội dung và trả phản hồi cuối.
fn put_raw(s: &mut TcpStream, name: &str, data: &[u8], sha256: Option<&str>, size: Option<u64>) -> Result<Value, FrameError> {
    let digest = match sha256 {
        Some(d) => d.to_string(),
        None => format!("{:x}", Sha256::digest(data)),
    };
    let size = size.unwrap_or(data.len() as u64);
    send_json(s, &json!({"op": "put", "name": name, "size": size, "sha256": digest}))?;
    let resp = recv_json(s)?;
    if resp["status"] != 100 {
        return Ok(resp);
    }
    send_all(s, data)?;
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
    let d = data();
    {
        let mut s = t.connect();
        let r = put_raw(&mut s, "../thu.txt", &d, None, None)?; // Ca 8a
        let ok = r["status"] == 200 && r["saved"] == "thu.txt";
        t.report("8a", "tên '../thu.txt'", ok, format!("status {}, lưu '{}'", r["status"], show(&r["saved"])));
        let r = put_raw(&mut s, "..\\..\\thu2.txt", &d, None, None)?; // Ca 8b
        let ok = r["status"] == 200 && r["saved"] == "thu2.txt";
        t.report("8b", "tên '..\\..\\thu2.txt'", ok, format!("status {}, lưu '{}'", r["status"], show(&r["saved"])));
        let r = put_raw(&mut s, "../", &d, None, None)?; // Ca 8c
        let ok = r["status"] == 400;
        let detail = format!("bị từ chối: status {}, {}", r["status"], show(&r["error"]));
        t.report("8c", "tên '../' (không còn tên tệp)", ok, detail);
        let zeros = "0".repeat(64);
        let r = put_raw(&mut s, "sai_bam.txt", &d, Some(&zeros), None)?; // Ca 9
        let ok = r["status"] == 422;
        t.report("9", "mã băm khai báo sai", ok, format!("status {}, {}", r["status"], show(&r["error"])));
        let r = put_raw(&mut s, "lon.bin", &d, None, Some(200 * 1_048_576))?; // Ca bổ sung 1
        let ok = r["status"] == 400;
        t.report("bổ sung 1", "size 200 MiB > MAX_FILE", ok, format!("status {}, {}", r["status"], show(&r["error"])));
        let r = put_raw(&mut s, "hex.txt", &d, Some("xyz"), None)?; // Ca bổ sung 2: sha256 không phải 64 ký tự hex
        let ok = r["status"] == 400;
        t.report("bổ sung 2", "sha256 'xyz' (không phải hex 64 ký tự)", ok, format!("status {}, {}", r["status"], show(&r["error"])));
        send_json(&mut s, &json!({"op": "bye"}))?;
    }
    let r;
    {
        // Ca bổ sung 3: ngắt giữa chừng – chỉ gửi 100/1000 byte rồi đóng
        let mut s = t.connect();
        send_json(&mut s, &json!({"op": "put", "name": "dang_do.txt", "size": 1000, "sha256": "0".repeat(64)}))?;
        r = recv_json(&mut s)?;
        send_all(&mut s, &d[..100])?;
    }
    {
        let mut s = t.connect();
        send_json(&mut s, &json!({"op": "list"}))?;
        let r2 = recv_json(&mut s)?;
        let ok = r["status"] == 100 && r2["status"] == 200;
        let detail = format!("server vẫn phục vụ client mới, list -> {}", r2["status"]);
        t.report("bổ sung 3", "ngắt sau 100/1000 byte", ok, detail);
        send_json(&mut s, &json!({"op": "bye"}))?;
    }
    Ok(())
}
