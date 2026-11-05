//! Dịch vụ thử (máy chủ phụ trợ) cho các vai trò luong, hang-doi. Giao thức giống bản Python/Go:
//!   VIEC <mã> <ms>  → chờ <ms> rồi trả "XONG <mã>"
//!   PHAT <n>        → gửi n dòng "GOI <k> <dữ liệu>", đọc n dòng "KQ <k> <tổng byte>", trả "NHAN <đúng>/<n>"
//!   THONGKE         → trả "TK <số yêu cầu VIEC> <đồng thời tối đa>", in tóm tắt "đợt", đặt lại

use crate::nhat_ky::{doc_dong, gui_dong, say};
use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Dữ liệu của gói thứ k (giống nhau ở mọi bản ngôn ngữ).
pub fn du_lieu(k: usize) -> String {
    format!("goi-so-{k}-").repeat(3)
}

/// Kết quả xử lý một gói: tổng giá trị các byte UTF-8, lấy dư 65536.
pub fn tong_byte(s: &str) -> u32 {
    s.bytes().map(u32::from).sum::<u32>() % 65536
}

#[derive(Default)]
struct ThongKe {
    tong: usize,
    dang_chay: usize,
    toi_da: usize,
}

fn phuc_vu(mut s: TcpStream, tk: Arc<Mutex<ThongKe>>) {
    let mut r = BufReader::new(s.try_clone().unwrap());
    let line = doc_dong(&mut r);
    let p: Vec<&str> = line.split_whitespace().collect();
    match p.first().copied() {
        Some("VIEC") if p.len() == 3 => {
            {
                let mut t = tk.lock().unwrap();
                t.tong += 1;
                t.dang_chay += 1;
                t.toi_da = t.toi_da.max(t.dang_chay);
            }
            thread::sleep(Duration::from_millis(p[2].parse().unwrap_or(0)));
            tk.lock().unwrap().dang_chay -= 1;
            let _ = gui_dong(&mut s, &format!("XONG {}", p[1]));
        }
        Some("PHAT") => {
            let n: usize = p[1].parse().unwrap_or(0);
            let mut buf = String::new();
            for k in 1..=n {
                buf += &format!("GOI {k} {}\n", du_lieu(k));
            }
            let _ = s.write_all(buf.as_bytes());
            let mut dung = 0;
            for _ in 0..n {
                let kq = doc_dong(&mut r);
                let q: Vec<&str> = kq.split_whitespace().collect();
                if q.len() == 3 {
                    let k: usize = q[1].parse().unwrap_or(0);
                    if q[2].parse::<u32>().ok() == Some(tong_byte(&du_lieu(k))) {
                        dung += 1;
                    }
                }
            }
            let _ = gui_dong(&mut s, &format!("NHAN {dung}/{n}"));
            say(&format!(
                "[dịch vụ] PHAT {n}: nhận {n} kết quả, đúng {dung}/{n}"
            ));
        }
        Some("THONGKE") => {
            let (a, b) = {
                let mut t = tk.lock().unwrap();
                let kq = (t.tong, t.toi_da);
                t.tong = 0;
                t.toi_da = 0;
                kq
            };
            let _ = gui_dong(&mut s, &format!("TK {a} {b}"));
            say(&format!(
                "[dịch vụ] đợt: {a} yêu cầu VIEC, đồng thời tối đa {b}"
            ));
        }
        _ => {}
    }
}

pub fn run(host: &str, port: u16) {
    let ln = TcpListener::bind((host, port)).expect("không mở được cổng");
    say(&format!(
        "[dịch vụ] lắng nghe {host}:{port} (mỗi kết nối một luồng)"
    ));
    let tk = Arc::new(Mutex::new(ThongKe::default()));
    for s in ln.incoming().flatten() {
        let tk = Arc::clone(&tk);
        thread::spawn(move || phuc_vu(s, tk));
    }
}
