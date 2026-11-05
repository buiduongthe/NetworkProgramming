//! Bài mở rộng MR5.4 – Luồng (thread) và đa luồng trong chương trình mạng (Rust, chỉ thư viện chuẩn).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1–5.6) và Chương 6 (Bài 6.1, 6.2).
//! Khác MR5.2 (so sánh mô hình trên một khối việc), bài này đi từng bước vào chính luồng:
//!   (a) std::thread: tạo – chạy – join, truyền tham số (move), lấy kết quả (JoinHandle<T>) → luong
//!   (b) server: mỗi client một luồng / nhóm luồng + sync_channel giới hạn (503)        → server, tai
//!   (c) trạng thái dùng chung: race condition, Mutex, AtomicU64, RwLock<HashMap>         → khoa
//!   (d) nhà sản xuất – người tiêu dùng bằng sync_channel, poison pill                    → hang-doi
//!   (e) semaphore tự làm bằng Mutex + Condvar giới hạn kết nối ra ngoài                  → luong
//!   (f) bế tắc: khóa chờ có hạn (Condvar::wait_timeout), thứ tự lấy khóa                 → khoa
//!   (g) nhật ký theo luồng: tên luồng + thread_local! (nhat_ky.rs)
//!
//! Chạy:  cargo run -q --release -- dich-vu|server|luong|hang-doi|tai|khoa <host> <port>

mod dich_vu;
mod hang_doi;
mod khoa;
mod may_chu;
mod nhat_ky;

use nhat_ky::{dat_ngu_canh, doc_dong, gui_dong, log, say, spawn_ten};
use std::io::BufReader;
use std::net::TcpStream;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

const SO_SUAT: usize = 3;
const N_GOI_RA: usize = 8;
const MS_GOI_RA: u64 = 150;

/// Một lời gọi mạng kiểu chặn: kết nối, gửi một dòng, đọc một dòng trả lời.
fn goi_dich_vu(host: &str, port: u16, line: &str) -> String {
    let Ok(mut s) = TcpStream::connect((host, port)) else {
        return "LOI".into();
    };
    let _ = gui_dong(&mut s, line);
    doc_dong(&mut BufReader::new(s))
}

/// (a) tạo, chạy, chờ luồng; tham số và kết quả
fn co_ban(host: &str, port: u16) {
    say("(a) Tạo – chạy – chờ luồng: 3 luồng, mỗi luồng nhận tham số (mã, thời gian xử lý) và trả kết quả");
    let tham_so = [(1u32, 300u64), (2, 100), (3, 200)];
    log("tạo 3 luồng tải-1..tải-3 rồi cho chạy (start)");
    let t = Instant::now();
    let hs: Vec<_> = tham_so
        .iter()
        .map(|&(ma, ms)| {
            let host = host.to_string(); // `move` chuyển quyền sở hữu tham số vào luồng
            spawn_ten(&format!("tải-{ma}"), move || {
                dat_ngu_canh(&format!("yc={ma}"));
                log(&format!("kết nối dịch vụ, gửi VIEC {ma} {ms}"));
                let t = Instant::now();
                let tl = goi_dich_vu(&host, port, &format!("VIEC {ma} {ms}"));
                let dt = t.elapsed().as_secs_f64() * 1000.0;
                log(&format!("nhận {tl} sau {dt:.0} ms"));
                (tl, dt) // giá trị trả về của luồng – lấy bằng join()
            })
        })
        .collect();
    let ket_qua: Vec<(String, f64)> = hs.into_iter().map(|h| h.join().unwrap()).collect(); // join = chờ + lấy kết quả
    let tong = t.elapsed().as_secs_f64();
    let tuan_tu: u64 = tham_so.iter().map(|x| x.1).sum();
    log(&format!(
        "chờ (join) xong cả 3 luồng sau {tong:.2} s (tuần tự sẽ ≈ {:.2} s)",
        tuan_tu as f64 / 1000.0
    ));
    say("");
    say(&format!(
        "  {:<8}{:<16}{:<10}{:>10}",
        "Luồng", "Tham số", "Kết quả", "Thời gian"
    ));
    for ((ma, ms), (tl, dt)) in tham_so.iter().zip(&ket_qua) {
        say(&format!(
            "  {:<8}{:<16}{:<10}{:>7.0} ms",
            format!("tải-{ma}"),
            format!("mã {ma}, {ms} ms"),
            tl,
            dt
        ));
    }
    goi_dich_vu(host, port, "THONGKE"); // đặt lại thống kê của dịch vụ trước phần (e)
}

/// Semaphore tự làm (std chưa có): Mutex<số suất đang dùng> + Condvar đánh thức luồng chờ.
struct GioiHan {
    dang_dung: Mutex<usize>,
    con_suat: Condvar,
}

impl GioiHan {
    fn lay(&self) -> usize {
        let mut g = self.dang_dung.lock().unwrap();
        if *g == SO_SUAT {
            log("hết suất → chờ");
        }
        while *g == SO_SUAT {
            g = self.con_suat.wait(g).unwrap(); // nhả mutex và ngủ tới khi notify
        }
        *g += 1;
        *g
    }

    fn tra(&self) {
        *self.dang_dung.lock().unwrap() -= 1;
        self.con_suat.notify_one();
    }
}

/// (e) giới hạn kết nối ra ngoài
fn gioi_han(host: &str, port: u16) {
    say("");
    say(&format!("(e) Giới hạn kết nối ra ngoài: {N_GOI_RA} luồng gọi dịch vụ, tối đa {SO_SUAT} kết nối cùng lúc (semaphore)"));
    let suat = Arc::new(GioiHan {
        dang_dung: Mutex::new(0),
        con_suat: Condvar::new(),
    });
    let t = Instant::now();
    let hs: Vec<_> = (1..=N_GOI_RA)
        .map(|k| {
            let (host, suat) = (host.to_string(), Arc::clone(&suat));
            spawn_ten(&format!("gọi-{k}"), move || {
                dat_ngu_canh(&format!("yc={k}"));
                let n = suat.lay();
                log(&format!(
                    "được suất (đang dùng {n}/{SO_SUAT}), gửi VIEC {} {MS_GOI_RA}",
                    10 + k
                ));
                goi_dich_vu(&host, port, &format!("VIEC {} {MS_GOI_RA}", 10 + k));
                suat.tra(); // goi_dich_vu không panic nên trả suất ở đây là đủ (có thể dùng guard Drop)
                log("xong, trả suất");
            })
        })
        .collect();
    for h in hs {
        let _ = h.join();
    }
    let dt = t.elapsed().as_secs_f64();
    let tk = goi_dich_vu(host, port, "THONGKE");
    let p: Vec<&str> = tk.split_whitespace().collect();
    let dot = N_GOI_RA.div_ceil(SO_SUAT) as u64;
    log(&format!(
        "dịch vụ xác nhận: {} yêu cầu, đồng thời tối đa {}; tổng {dt:.2} s (≈ ⌈{N_GOI_RA}/{SO_SUAT}⌉ × {MS_GOI_RA} ms = {:.2} s)",
        p[1],
        p[2],
        (dot * MS_GOI_RA) as f64 / 1000.0
    ));
}

fn run_luong(host: &str, port: u16) {
    co_ban(host, port);
    gioi_han(host, port);
    say("");
    say("(g) Nhật ký theo luồng: mỗi dòng ghi tên luồng; ngữ cảnh \"yc=\" lấy từ biến cục bộ luồng,");
    say("    nên hàm ghi nhật ký không cần nhận mã yêu cầu qua tham số.");
    say("");
    say("Đặc thù Rust: thread::spawn đòi closure 'static + Send; dữ liệu chung bọc Arc<Mutex<T>>. Trình");
    say("  biên dịch từ chối mã có data race (Send/Sync), nên bản \"không khóa\" ở (c) phải viết bằng load/store");
    say("  rời rạc trên AtomicU64 – race condition logic vẫn xảy ra. thread_local! là biến cục bộ luồng.");
}

fn main() {
    nhat_ky::khoi_dong();
    let a: Vec<String> = std::env::args().skip(1).collect();
    let vai = ["dich-vu", "server", "luong", "hang-doi", "tai", "khoa"];
    if a.len() < 3 || !vai.contains(&a[0].as_str()) {
        eprintln!("Cách dùng: cargo run -q --release -- dich-vu|server|luong|hang-doi|tai|khoa <host> <port>");
        std::process::exit(1);
    }
    let (host, port) = (
        a[1].as_str(),
        a[2].parse::<u16>().expect("cổng không hợp lệ"),
    );
    match a[0].as_str() {
        "dich-vu" => dich_vu::run(host, port),
        "server" => may_chu::run_server(host, port),
        "luong" => run_luong(host, port),
        "hang-doi" => hang_doi::run(host, port),
        "tai" => may_chu::run_tai(host, port),
        _ => khoa::run(host, port),
    }
}
