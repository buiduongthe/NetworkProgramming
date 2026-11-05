//! (d) Nhà sản xuất – người tiêu dùng trên một kết nối TCP.
//!   luồng "nhận"     : gửi "PHAT 9", đọc từng dòng "GOI k dữ-liệu", gửi vào mpsc::sync_channel(4) – đầy thì
//!                      send() CHẶN: áp lực ngược, không đọc thêm từ socket;
//!   3 luồng "xử-lý-k": nhận gói (Receiver dùng chung bọc Arc<Mutex<_>>), xử lý 80 ms + tính tổng byte;
//!   luồng "gửi"      : ghi "KQ k tổng" vào bản sao của CÙNG TcpStream (try_clone: một luồng đọc, một ghi).
//! Kết thúc êm bằng "viên thuốc độc": None. (Cách khác trong Rust: thả mọi Sender – recv() trả Err khi kênh
//! rỗng và không còn Sender; hoặc cờ AtomicBool khi cần dừng NGAY.)

use crate::dich_vu::tong_byte;
use crate::nhat_ky::{dat_ngu_canh, doc_dong, gui_dong, log, say, spawn_ten};
use std::io::BufReader;
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const N_GOI: usize = 9;
const N_THO: usize = 3;
const SUC_CHUA: usize = 4;
const MS_XU_LY: u64 = 80;

type Goi = Option<(usize, String)>; // None = viên thuốc độc

pub fn run(host: &str, port: u16) {
    say(&format!("(d) Nhà sản xuất – người tiêu dùng: luồng nhận đọc socket → hàng đợi (tối đa {SUC_CHUA}) → {N_THO} luồng xử lý → luồng gửi"));
    let s = TcpStream::connect((host, port)).expect("không kết nối được dịch vụ");
    let (tx_viec, rx_viec) = mpsc::sync_channel::<Goi>(SUC_CHUA);
    let rx_viec = Arc::new(Mutex::new(rx_viec));
    let (tx_kq, rx_kq) = mpsc::channel::<Option<(usize, u32)>>();
    let so_lan_cho = Arc::new(AtomicUsize::new(0));
    let t = Instant::now();
    let mut hs = Vec::new();

    let (mut ghi, doc) = (s.try_clone().unwrap(), s.try_clone().unwrap());
    let cho = Arc::clone(&so_lan_cho);
    hs.push(spawn_ten("nhận", move || {
        let mut r = BufReader::new(doc);
        log(&format!("kết nối dịch vụ, gửi PHAT {N_GOI}"));
        let _ = gui_dong(&mut ghi, &format!("PHAT {N_GOI}"));
        for _ in 0..N_GOI {
            let line = doc_dong(&mut r);
            let p: Vec<&str> = line.split_whitespace().collect();
            let k: usize = p[1].parse().unwrap();
            if let Err(TrySendError::Full(g)) = tx_viec.try_send(Some((k, p[2].to_string()))) {
                cho.fetch_add(1, Ordering::Relaxed);
                log(&format!(
                    "hàng đợi đầy ({SUC_CHUA}/{SUC_CHUA}) → chờ chỗ trống (áp lực ngược)"
                ));
                let _ = tx_viec.send(g); // chặn tới khi một luồng xử lý lấy bớt
            }
            log(&format!("GOI {k} → hàng đợi"));
        }
        log(&format!(
            "hết dữ liệu → đặt {N_THO} viên thuốc độc (poison pill) vào hàng đợi"
        ));
        for _ in 0..N_THO {
            let _ = tx_viec.send(None);
        }
        r // trả lại bộ đọc để luồng chính đọc dòng NHAN cuối cùng
    }));

    let mut tho = Vec::new();
    for i in 1..=N_THO {
        let (rx, tx) = (Arc::clone(&rx_viec), tx_kq.clone());
        tho.push(spawn_ten(&format!("xử-lý-{i}"), move || {
            let mut dem = 0;
            loop {
                let g = rx.lock().unwrap().recv().unwrap_or(None);
                let Some((k, text)) = g else {
                    dat_ngu_canh("");
                    log("gặp viên thuốc độc → dừng");
                    let _ = tx.send(None);
                    return dem; // JoinHandle trả về số gói đã xử lý
                };
                dat_ngu_canh(&format!("yc={k}")); // ngữ cảnh cục bộ luồng: log() tự ghi "yc=k"
                thread::sleep(Duration::from_millis(MS_XU_LY)); // giả lập xử lý tốn thời gian
                let v = tong_byte(&text);
                log(&format!("xử lý GOI {k}: tổng byte = {v}"));
                dem += 1;
                let _ = tx.send(Some((k, v)));
            }
        }));
    }
    drop(tx_kq);

    let mut ghi = s.try_clone().unwrap();
    let gui = spawn_ten("gửi", move || {
        let mut con_lai = N_THO;
        while con_lai > 0 {
            match rx_kq.recv() {
                Ok(Some((k, v))) => {
                    let _ = gui_dong(&mut ghi, &format!("KQ {k} {v}"));
                    log(&format!("gửi KQ {k} {v}"));
                }
                _ => con_lai -= 1,
            }
        }
        log(&format!("cả {N_THO} luồng xử lý đã dừng → hết việc gửi"));
    });

    let mut r = hs.pop().unwrap().join().unwrap();
    let dem: Vec<usize> = tho.into_iter().map(|h| h.join().unwrap()).collect();
    let _ = gui.join();
    let tra_loi = doc_dong(&mut r);
    let dt = t.elapsed().as_secs_f64();
    log(&format!("dịch vụ trả lời: {tra_loi}"));
    say("");
    say(&format!("  {:<12}{:>16}", "Luồng", "Số gói đã xử lý"));
    for (i, d) in dem.iter().enumerate() {
        say(&format!("  {:<12}{:>16}", format!("xử-lý-{}", i + 1), d));
    }
    say(&format!(
        "  Luồng nhận phải chờ vì hàng đợi đầy: {} lần; tổng {dt:.2} s (1 luồng xử lý ≈ {:.2} s)",
        so_lan_cho.load(Ordering::Relaxed),
        (N_GOI as u64 * MS_XU_LY) as f64 / 1000.0
    ));
}
