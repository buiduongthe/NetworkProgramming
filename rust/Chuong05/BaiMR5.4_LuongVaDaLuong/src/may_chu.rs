//! (b) Server TCP đa luồng: mỗi client một luồng và nhóm luồng cố định.
//!   cổng P   – máy chủ A: luồng "nhận-A" accept(), mỗi kết nối một luồng MỚI "kết-nối-k";
//!   cổng P+1 – máy chủ B: 2 luồng cố định "nhóm-1", "nhóm-2" nhận việc từ mpsc::sync_channel(2) (kênh có
//!              giới hạn); "nhận-B" try_send() – Err(Full) → trả "503 BAN <mã>" ngay.
//!              Receiver không chia sẻ được giữa nhiều luồng → bọc Arc<Mutex<Receiver>>.
//! Vai trò tai: 6 luồng "khách-k" (cách nhau 20 ms) gửi "VIEC k 300" tới từng máy chủ.

use crate::nhat_ky::{doc_dong, gui_dong, log, say, spawn_ten, ten_luong};
use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const N_KHACH: usize = 6;
const MS_VIEC: u64 = 300;
const CACH_MS: u64 = 20;
const NHOM: usize = 2;
const HANG_DOI: usize = 2;

/// Phần việc giống nhau ở hai máy chủ: chờ <ms> rồi trả lời kèm tên luồng đang chạy.
fn xu_ly(s: &mut TcpStream, p: &[String]) {
    log(&format!("xử lý VIEC {} {}", p[1], p[2]));
    thread::sleep(Duration::from_millis(p[2].parse().unwrap_or(0)));
    let _ = gui_dong(s, &format!("XONG {} {}", p[1], ten_luong()));
}

fn doc_yeu_cau(s: &TcpStream) -> Option<Vec<String>> {
    let mut r = BufReader::new(s.try_clone().ok()?);
    let p: Vec<String> = doc_dong(&mut r)
        .split_whitespace()
        .map(String::from)
        .collect();
    (p.len() == 3 && p[0] == "VIEC").then_some(p)
}

fn may_chu_a(ln: TcpListener) {
    for (i, s) in ln.incoming().flatten().enumerate() {
        let dem = i + 1;
        log(&format!("kết nối mới → tạo luồng kết-nối-{dem}"));
        spawn_ten(&format!("kết-nối-{dem}"), move || {
            let mut s = s;
            if let Some(p) = doc_yeu_cau(&s) {
                xu_ly(&mut s, &p);
            }
        });
    }
}

fn may_chu_b(ln: TcpListener) {
    let (tx, rx) = mpsc::sync_channel::<(TcpStream, Vec<String>)>(HANG_DOI); // kênh có giới hạn
    let rx = Arc::new(Mutex::new(rx));
    for i in 1..=NHOM {
        let rx = Arc::clone(&rx);
        spawn_ten(&format!("nhóm-{i}"), move || loop {
            // luồng thợ: giữ khóa chỉ trong lúc lấy việc, xử lý xong mới lấy việc tiếp
            let viec = rx.lock().unwrap().recv();
            match viec {
                Ok((mut s, p)) => xu_ly(&mut s, &p),
                Err(_) => return,
            }
        });
    }
    for s in ln.incoming().flatten() {
        let Some(p) = doc_yeu_cau(&s) else { continue };
        let ma = p[1].clone();
        match tx.try_send((s, p)) {
            Ok(()) => log(&format!("VIEC {ma} vào hàng đợi")),
            Err(TrySendError::Full((mut s, _))) => {
                log(&format!(
                    "hàng đợi đầy ({HANG_DOI}/{HANG_DOI}) → trả 503 cho VIEC {ma}"
                ));
                let _ = gui_dong(&mut s, &format!("503 BAN {ma}"));
            }
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
}

pub fn run_server(host: &str, pa: u16) {
    let a = TcpListener::bind((host, pa)).expect("không mở được cổng A");
    let b = TcpListener::bind((host, pa + 1)).expect("không mở được cổng B");
    say(&format!(
        "[server] cổng {pa}: máy chủ A – mỗi client một luồng"
    ));
    say(&format!("[server] cổng {}: máy chủ B – nhóm {NHOM} luồng cố định, hàng đợi tối đa {HANG_DOI} (đầy → 503)", pa + 1));
    spawn_ten("nhận-A", move || may_chu_a(a));
    let _ = spawn_ten("nhận-B", move || may_chu_b(b)).join();
}

// ---------------- Máy tạo tải ----------------
fn khach(host: String, port: u16, k: usize) -> String {
    thread::sleep(Duration::from_millis((k as u64 - 1) * CACH_MS)); // khách k xuất phát muộn (k-1)×20 ms
    let t = Instant::now();
    let Ok(mut s) = TcpStream::connect((host.as_str(), port)) else {
        return String::new();
    };
    let _ = gui_dong(&mut s, &format!("VIEC {k} {MS_VIEC}"));
    let line = doc_dong(&mut BufReader::new(s));
    let dt = t.elapsed().as_secs_f64();
    let p: Vec<&str> = line.split_whitespace().collect();
    if p.first() == Some(&"XONG") {
        log(&format!("XONG {k} bởi {} sau {dt:.2} s", p[2]));
    } else {
        log(&format!("{line} (bị từ chối) sau {dt:.2} s"));
    }
    line
}

fn mot_dot(host: &str, port: u16) -> (usize, usize, f64) {
    let t = Instant::now();
    let hs: Vec<_> = (1..=N_KHACH)
        .map(|k| {
            let h = host.to_string();
            spawn_ten(&format!("khách-{k}"), move || khach(h, port, k))
        })
        .collect();
    // JoinHandle<String>::join() trả về giá trị mà luồng return
    let ok = hs
        .into_iter()
        .filter_map(|h| h.join().ok())
        .filter(|x| x.starts_with("XONG"))
        .count();
    (ok, N_KHACH - ok, t.elapsed().as_secs_f64())
}

pub fn run_tai(host: &str, pa: u16) {
    say(&format!("(b) Máy chủ đa luồng: {N_KHACH} client đồng thời (cách nhau {CACH_MS} ms), mỗi yêu cầu xử lý {MS_VIEC} ms"));
    say(&format!("Máy chủ A – mỗi client một luồng (cổng {pa}):"));
    let a = mot_dot(host, pa);
    say(&format!(
        "Máy chủ B – nhóm {NHOM} luồng, hàng đợi {HANG_DOI} (cổng {}):",
        pa + 1
    ));
    let b = mot_dot(host, pa + 1);
    say("");
    say(&format!(
        "  {:<29}{:>11}{:>9}{:>10}",
        "Máy chủ", "Thành công", "Từ chối", "Thời gian"
    ));
    for (ten, (ok, tc, dt)) in [
        ("A. mỗi client một luồng".to_string(), a),
        (format!("B. nhóm {NHOM} luồng + hàng đợi {HANG_DOI}"), b),
    ] {
        say(&format!("  {ten:<29}{ok:>11}{tc:>9}{dt:>8.2} s"));
    }
    say("Nhận xét: A tạo luồng không giới hạn – nhanh khi ít client nhưng quá tải sẽ cạn bộ nhớ;");
    say(&format!(
        "  B giữ số luồng cố định, chỉ nhận {} việc cùng lúc, phần vượt bị từ chối ngay (503)",
        NHOM + HANG_DOI
    ));
    say("  thay vì xếp hàng vô hạn – client biết để thử lại sau.");
}
