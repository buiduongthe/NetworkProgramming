//! (c) Trạng thái dùng chung giữa các luồng xử lý kết nối; (f) bế tắc.
//!
//! (c) Server đếm chạy ngay trong chương trình: 4 luồng "gửi-k" mở 4 kết nối TCP, mỗi kết nối gửi 2000 dòng
//!     "DEM GET"/"DEM PUT"; mỗi kết nối một luồng "xử-lý-k" cập nhật bốn bộ đếm dùng chung:
//!     1. "không khóa": Rust KHÔNG cho biên dịch mã có data race (u64 thường không chia sẻ được giữa luồng để
//!        ghi). Muốn tái hiện lỗi phải viết tường minh load → yield_now → store trên AtomicU64: không còn hành
//!        vi không xác định nhưng vẫn là race condition logic → mất cập nhật;
//!     2. Mutex<u64>; 3. AtomicU64::fetch_add (nguyên tử);
//!     4. bảng đồng thời: std không có HashMap đồng thời → RwLock<HashMap<String, AtomicU64>>: khóa đọc
//!        (nhiều luồng cùng lúc) để tăng bộ đếm sẵn có, khóa ghi chỉ khi thêm khóa mới.
//! (f) "chuyển-AB" khóa phiên A rồi B, "chuyển-BA" khóa B rồi A. std::sync::Mutex không có chờ có hạn →
//!     KhoaCoHan tự làm bằng Mutex<bool> + Condvar::wait_timeout; sửa bằng thứ tự lấy khóa cố định.

use crate::nhat_ky::{doc_dong, log, say, spawn_ten};
use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

const N_KET_NOI: usize = 4;
const N_TIN: usize = 2000;
const GIU_MS: u64 = 50;
const HAN_CHO_MS: u64 = 500;

#[derive(Default)]
struct BoDem {
    khong_khoa: AtomicU64,
    co_khoa: Mutex<u64>,
    nguyen_tu: AtomicU64,
    bang: RwLock<HashMap<String, AtomicU64>>,
}

impl BoDem {
    fn cap_nhat(&self, loai: &str) {
        let v = self.khong_khoa.load(Ordering::Relaxed); // 1. đọc
        thread::yield_now(); //    nhường CPU: luồng khác có thể chen vào đây
        self.khong_khoa.store(v + 1, Ordering::Relaxed); //    ghi – đè lên kết quả của luồng khác
        {
            let mut g = self.co_khoa.lock().unwrap(); // 2. đoạn găng có khóa
            let v = *g;
            thread::yield_now();
            *g = v + 1;
        }
        self.nguyen_tu.fetch_add(1, Ordering::Relaxed); // 3. nguyên tử
        if let Some(c) = self.bang.read().unwrap().get(loai) {
            c.fetch_add(1, Ordering::Relaxed); // 4. khóa ĐỌC + bộ đếm nguyên tử
            return;
        }
        let mut w = self.bang.write().unwrap(); // chưa có khóa → khóa GHI để thêm
        w.entry(loai.to_string())
            .or_default()
            .fetch_add(1, Ordering::Relaxed);
    }

    fn lay(&self, loai: &str) -> u64 {
        self.bang
            .read()
            .unwrap()
            .get(loai)
            .map_or(0, |c| c.load(Ordering::Relaxed))
    }
}

fn tranh_chap(host: &str, port: u16) {
    say(&format!("(c) Trạng thái dùng chung: {N_KET_NOI} kết nối × {N_TIN} thông điệp DEM, mỗi kết nối một luồng xử lý"));
    let ln = TcpListener::bind((host, port)).expect("không mở được cổng");
    log(&format!(
        "server đếm lắng nghe {host}:{port}; {N_KET_NOI} luồng gửi bắt đầu"
    ));
    let data: Arc<String> = Arc::new(
        (0..N_TIN)
            .map(|i| if i % 2 == 0 { "DEM GET\n" } else { "DEM PUT\n" })
            .collect(),
    );
    let bd = Arc::new(BoDem::default());
    let mut hs = Vec::new();
    for k in 1..=N_KET_NOI {
        let (h, d) = (host.to_string(), Arc::clone(&data));
        hs.push(spawn_ten(&format!("gửi-{k}"), move || {
            if let Ok(mut s) = TcpStream::connect((h.as_str(), port)) {
                let _ = s.write_all(d.as_bytes());
            }
        }));
    }
    for k in 1..=N_KET_NOI {
        let Ok((s, _)) = ln.accept() else { break };
        let bd = Arc::clone(&bd);
        hs.push(spawn_ten(&format!("xử-lý-{k}"), move || {
            let mut r = BufReader::new(s);
            let mut so_tin = 0;
            loop {
                let line = doc_dong(&mut r);
                if line.is_empty() {
                    break;
                }
                let p: Vec<&str> = line.split_whitespace().collect();
                if p.len() == 2 && p[0] == "DEM" {
                    bd.cap_nhat(p[1]);
                    so_tin += 1;
                }
            }
            log(&format!("kết nối đóng: đã xử lý {so_tin} thông điệp"));
        }));
    }
    for h in hs {
        let _ = h.join();
    }
    let dung = (N_KET_NOI * N_TIN) as u64;
    let (get, put) = (bd.lay("GET"), bd.lay("PUT"));
    let kk = bd.khong_khoa.load(Ordering::Relaxed);
    let ck = *bd.co_khoa.lock().unwrap();
    let nt = bd.nguyen_tu.load(Ordering::Relaxed);
    say("");
    say(&format!(
        "  {:<32}{:>9}{:>6}{:>13}",
        "Cách cập nhật biến chung", "Giá trị", "Đúng", "Mất cập nhật"
    ));
    for (ten, gt, so) in [
        ("không khóa (đọc – nhường – ghi)", kk.to_string(), kk),
        ("khóa (mutex/lock)", ck.to_string(), ck),
        ("nguyên tử (atomic)", nt.to_string(), nt),
        (
            "bảng đồng thời: GET + PUT",
            format!("{get}+{put}"),
            get + put,
        ),
    ] {
        say(&format!("  {ten:<32}{gt:>9}{dung:>6}{:>13}", dung - so));
    }
    say("Nhận xét: \"đọc – làm việc khác – ghi\" không khóa làm mất cập nhật; khóa, thao tác nguyên tử");
    say("  hoặc cấu trúc dữ liệu đồng thời đều cho kết quả đúng.");
}

/// Khóa có chờ có hạn: Mutex<bool> (đang bị giữ?) + Condvar (đánh thức luồng chờ khi nhả).
#[derive(Default)]
struct KhoaCoHan {
    dang_giu: Mutex<bool>,
    nha: Condvar,
}

impl KhoaCoHan {
    fn lay(&self, han: Duration) -> bool {
        let het = Instant::now() + han;
        let mut g = self.dang_giu.lock().unwrap();
        while *g {
            let con = het.saturating_duration_since(Instant::now());
            if con.is_zero() {
                return false;
            }
            g = self.nha.wait_timeout(g, con).unwrap().0; // nhả mutex và ngủ tới khi notify hoặc hết giờ
        }
        *g = true;
        true
    }

    fn tra(&self) {
        *self.dang_giu.lock().unwrap() = false;
        self.nha.notify_one();
    }
}

fn be_tac() {
    say("");
    say("(f) Bế tắc: hai luồng chuyển tin giữa phiên A và phiên B, mỗi luồng cần khóa của cả hai phiên");
    let khoa: Arc<HashMap<&'static str, KhoaCoHan>> = Arc::new(
        [("A", KhoaCoHan::default()), ("B", KhoaCoHan::default())]
            .into_iter()
            .collect(),
    );
    let mut bang = Vec::new();
    for (lan, co_thu_tu) in [(1, false), (2, true)] {
        if co_thu_tu {
            say(&format!(
                "Lần {lan} – mọi luồng lấy khóa theo cùng thứ tự (A trước B):"
            ));
        } else {
            say(&format!("Lần {lan} – lấy khóa theo thứ tự ngược nhau:"));
        }
        let xong = Arc::new(AtomicUsize::new(0));
        let bo = Arc::new(AtomicUsize::new(0));
        let chuyen = |nguon: &'static str, dich: &'static str| {
            let (khoa, xong, bo) = (Arc::clone(&khoa), Arc::clone(&xong), Arc::clone(&bo));
            move || {
                let (dau, sau) = if co_thu_tu && nguon > dich {
                    (dich, nguon)
                } else {
                    (nguon, dich)
                };
                khoa[dau].lay(Duration::from_secs(3600));
                log(&format!("giữ khóa phiên {dau}, chờ khóa phiên {sau}"));
                thread::sleep(Duration::from_millis(GIU_MS)); // giả lập ghi vào socket của phiên đang giữ
                if khoa[sau].lay(Duration::from_millis(HAN_CHO_MS)) {
                    log(&format!("đã chuyển tin từ phiên {nguon} sang phiên {dich}"));
                    xong.fetch_add(1, Ordering::Relaxed);
                    khoa[sau].tra();
                } else {
                    log(&format!("chờ khóa phiên {sau} quá {HAN_CHO_MS} ms → nghi bế tắc, nhả khóa phiên {dau}, bỏ cuộc"));
                    bo.fetch_add(1, Ordering::Relaxed);
                }
                khoa[dau].tra();
            }
        };
        let t1 = spawn_ten("chuyển-AB", chuyen("A", "B"));
        thread::sleep(Duration::from_millis(10));
        let t2 = spawn_ten("chuyển-BA", chuyen("B", "A"));
        let _ = (t1.join(), t2.join());
        bang.push((
            if co_thu_tu {
                "cùng thứ tự"
            } else {
                "ngược thứ tự"
            },
            xong.load(Ordering::Relaxed),
            bo.load(Ordering::Relaxed),
        ));
    }
    say("");
    say(&format!(
        "  {:<20}{:>10}{:>24}",
        "Cách lấy khóa", "Hoàn tất", "Bỏ cuộc (hết hạn chờ)"
    ));
    for (ten, x, b) in bang {
        say(&format!("  {ten:<20}{x:>10}{b:>24}"));
    }
    say("Nhận xét: cùng một thứ tự lấy khóa cho mọi luồng loại bỏ vòng chờ – điều kiện cần của bế tắc.");
}

pub fn run(host: &str, port: u16) {
    tranh_chap(host, port);
    be_tac();
}
