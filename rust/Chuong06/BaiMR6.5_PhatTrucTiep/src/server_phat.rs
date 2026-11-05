//! Bài mở rộng MR6.5 – Máy chủ trung chuyển phát trực tiếp 1→n (bản dòng lệnh).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp MR6.2 (âm thanh RTP), MR6.3 (video chia mảnh),
//! Bài 6.1 (server đa luồng), Bài 6.6 (một nguồn – nhiều người nhận). Lõi nằm ở may_chu.rs (MayChuPhat,
//! dùng chung với server_gui.rs); tệp này chỉ in sự kiện và nhận lệnh bàn phím.
//!
//! Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi server chạy):
//!   trang_thai      bo_dem bat|tat      ngat <tên>      thoat
//!
//! Chạy:  server_phat [host] [cổng_tcp] [cổng_udp]      (mặc định cổng_udp = cổng_tcp + 1)

use crate::chung::{bay_gio, doc_lenh_stdin, ngu};
use crate::may_chu::{bang_tong_ket, bang_trang_thai, MayChuPhat};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

static KHOA_IN: Mutex<()> = Mutex::new(());

fn log(text: &str) {
    let _k = KHOA_IN.lock().unwrap(); // nhiều luồng cùng in: mỗi dòng in trọn vẹn
    println!("[server] {text}");
}

pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong_tcp: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(41650);
    let cong_udp: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(cong_tcp + 1);
    let sv = MayChuPhat::new(Arc::new(log));
    if let Err(e) = sv.bat_dau(&host, cong_tcp, cong_udp) {
        log(&format!("Không mở được cổng: {e}"));
        std::process::exit(1);
    }
    let xong = Arc::new(AtomicBool::new(false));
    let x = Arc::clone(&xong);
    let _ = ctrlc::set_handler(move || x.store(true, Ordering::SeqCst)); // Ctrl+C (công cụ demo dừng server bằng SIGINT)
    let (s, x) = (Arc::clone(&sv), Arc::clone(&xong));
    doc_lenh_stdin(bay_gio(), move |dong| {
        let p: Vec<&str> = dong.split_whitespace().collect();
        log(&format!("> {dong}"));
        match p.as_slice() {
            ["trang_thai", ..] => {
                for d in bang_trang_thai(&s.trang_thai()) {
                    log(&d);
                }
            }
            ["bo_dem", b @ ("bat" | "tat")] => s.dat_bo_dem(*b == "bat"),
            ["ngat", ten] => log(&format!("Đã ngắt {} người xem", s.ngat(Some(ten), None))),
            ["thoat", ..] => x.store(true, Ordering::SeqCst),
            _ => log("lệnh không hợp lệ"),
        }
    });
    while !xong.load(Ordering::SeqCst) {
        ngu(0.1);
    }
    for d in bang_tong_ket(&sv.trang_thai()) {
        log(&d);
    }
    sv.dung();
    std::process::exit(0);
}
