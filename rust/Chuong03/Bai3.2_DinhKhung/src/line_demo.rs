//! Minh họa READ_LINE (Mã giả 3.5, Mục 3.4.3): định khung bằng ký tự xuống dòng.
//!
//! Chương trình tự mở một kết nối loopback: luồng "gửi" chia dữ liệu thành các mảnh cắt ngang
//! dòng (kể cả cắt giữa hai byte của chữ "à"), luồng chính đọc bằng READ_LINE và in từng dòng
//! cùng số byte còn dư trong pending. Mảnh cuối là 100 byte không có '\n' để thấy lỗi "Dòng quá dài".

use crate::framing::{read_line, send_all, FrameError, Reader};
use crate::netutil::parse_endpoint;
use std::net::{TcpListener, TcpStream};
use std::process;
use std::thread;
use std::time::Duration;

const MAX_LEN: usize = 64;

/// Các mảnh gửi đi; b"\xc3" và b"\xa0" là hai nửa của chữ "à" trong UTF-8.
fn pieces() -> Vec<Vec<u8>> {
    vec![b"HELLO\nWOR".to_vec(), b"LD\r\nXin ch\xc3".to_vec(), b"\xa0o\n".to_vec(), vec![b'A'; 100]]
}

/// Hiển thị dãy byte: ký tự ASCII in được giữ nguyên, còn lại dạng \xNN.
fn show(data: &[u8]) -> String {
    let mut out = String::new();
    for &b in data {
        match b {
            0x0A => out.push_str("\\n"),
            0x0D => out.push_str("\\r"),
            0x20..=0x7E => out.push(b as char),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    if out.chars().count() <= 24 {
        out
    } else {
        out.chars().take(21).collect::<String>() + "..."
    }
}

fn sender(host: String, port: u16) {
    let mut s = match TcpStream::connect((host.as_str(), port)) {
        Ok(s) => s,
        Err(e) => {
            println!("[gửi] Không kết nối được: {e}");
            return;
        }
    };
    s.set_nodelay(true).ok(); // NODELAY: mỗi mảnh đi riêng
    for p in pieces() {
        println!("[gửi] {} byte: {}", p.len(), show(&p));
        if send_all(&mut s, &p).is_err() {
            return;
        }
        thread::sleep(Duration::from_millis(300));
    }
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "127.0.0.1", 9000);
    let srv = match TcpListener::bind((host.as_str(), port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[đọc] Không mở được {host}:{port}: {e}");
            process::exit(1);
        }
    };
    println!("[đọc] Đang lắng nghe tại {host}:{port}, max_len = {MAX_LEN}");
    let t = {
        let h = host.clone();
        thread::spawn(move || sender(h, port))
    };
    let (conn, _) = match srv.accept() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[đọc] Lỗi accept: {e}");
            process::exit(1);
        }
    };
    let mut r = Reader::new(conn); // Reader { sock, pending ← EMPTY_BYTES }
    loop {
        match read_line(&mut r, MAX_LEN) {
            Ok(Some(line)) => println!("[đọc] READ_LINE -> '{line}' (pending còn {} byte)", r.pending.len()),
            Ok(None) => {
                println!("[đọc] Đối tác đã đóng kết nối");
                break;
            }
            Err(FrameError::Protocol(m)) => {
                println!("[đọc] ProtocolError: {m} (pending {} byte > {MAX_LEN})", r.pending.len());
                break;
            }
            Err(e) => {
                println!("[đọc] Lỗi: {e}");
                break;
            }
        }
    }
    drop(r); // CLOSE(conn)
    drop(srv);
    t.join().ok();
}
