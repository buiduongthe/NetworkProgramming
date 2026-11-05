//! Server HTTP/1.1 nhỏ ở hai chế độ: vulnerable (không hạn tiêu đề) và protected (nhiều lớp chặn).
//! Nhóm luồng cố định: SO_LUONG_WORKER luồng cùng gọi accept() trên một listener dùng chung.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// ---------------- Tham số phòng thủ (chế độ protected) ----------------
const HAN_DOC_TIEU_DE: Duration = Duration::from_secs(3); // toàn bộ phần đầu phải xong trong hạn này
const HAN_CHOT_ROI: Duration = Duration::from_secs(2); // không nhận thêm byte trong khoảng này -> đóng
const CO_TIEU_DE_TOI_DA: usize = 8 * 1024; // tổng kích thước phần đầu tối đa
const GIOI_HAN_IP: usize = 10; // số kết nối đồng thời tối đa cho mỗi IP
const TRAN_TOAN_CUC: usize = 64; // số kết nối đồng thời tối đa toàn server (vượt -> 503)

const SO_LUONG_WORKER: usize = 40; // nhóm luồng cố định (giống MaxRequestWorkers của Apache prefork)
const TRA_LOI: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\
Connection: close\r\nContent-Length: 20\r\n\r\nXin chao tu server!\n";

#[derive(Default)]
struct ThongKe {
    chap_nhan: u64,   // số kết nối đã chấp nhận
    xong: u64,        // số yêu cầu phục vụ xong
    dong_cham: u64,   // số kết nối bị đóng vì gửi tiêu đề quá chậm/quá hạn
    tu_choi_503: u64, // số kết nối bị từ chối do vượt trần/giới hạn IP
}

/// Chế độ PROTECTED: đọc phần đầu HTTP với ba lớp chặn Slowloris.
/// (1) hạn chót tuyệt đối HAN_DOC_TIEU_DE cho toàn bộ phần đầu;
/// (2) hạn chót rỗi HAN_CHOT_ROI giữa hai lần nhận byte (nhỏ giọt -> timeout);
/// (3) tổng cỡ tiêu đề không vượt CO_TIEU_DE_TOI_DA.
/// Trả về true nếu nhận đủ phần đầu (kết thúc bằng CRLFCRLF), false nếu phải đóng sớm.
fn doc_tieu_de_co_han(conn: &mut TcpStream) -> bool {
    let mut du_lieu = Vec::new();
    let mut buf = [0u8; 1024];
    let han_tuyet_doi = Instant::now() + HAN_DOC_TIEU_DE;
    while chua_du_phan_dau(&du_lieu) {
        let con_lai = han_tuyet_doi.saturating_duration_since(Instant::now());
        if con_lai.is_zero() {
            return false; // (1) quá hạn tổng
        }
        if conn.set_read_timeout(Some(con_lai.min(HAN_CHOT_ROI))).is_err() {
            return false; // (2) hạn chót rỗi
        }
        match conn.read(&mut buf) {
            Ok(0) => return false, // client đóng giữa chừng
            Ok(n) => du_lieu.extend_from_slice(&buf[..n]),
            Err(_) => return false, // timeout hoặc lỗi
        }
        if du_lieu.len() > CO_TIEU_DE_TOI_DA {
            return false; // (3) tiêu đề quá to
        }
    }
    true
}

/// Chế độ VULNERABLE: đọc phần đầu HTTP, KHÔNG đặt hạn -> một client nhỏ giọt giữ worker mãi.
fn doc_tieu_de_khong_han(conn: &mut TcpStream) -> bool {
    let _ = conn.set_read_timeout(None);
    let mut du_lieu = Vec::new();
    let mut buf = [0u8; 1024];
    while chua_du_phan_dau(&du_lieu) {
        match conn.read(&mut buf) {
            Ok(0) | Err(_) => return false,
            Ok(n) => du_lieu.extend_from_slice(&buf[..n]),
        }
    }
    true
}

fn chua_du_phan_dau(d: &[u8]) -> bool {
    !d.windows(4).any(|w| w == b"\r\n\r\n")
}

type DemIp = Arc<Mutex<HashMap<String, usize>>>;

fn worker(listener: &TcpListener, mode: &str, stat: &Mutex<ThongKe>, dang_chay: &AtomicBool, dem_ip: &DemIp) {
    while dang_chay.load(Ordering::Relaxed) {
        let (mut conn, addr) = match listener.accept() {
            Ok(x) => x,
            Err(_) => return,
        };
        stat.lock().unwrap().chap_nhan += 1;
        let ip = addr.ip().to_string();
        if mode == "protected" {
            // Trần toàn cục và giới hạn theo IP: vượt -> 503 rồi đóng ngay (giải phóng worker).
            let qua_tai = {
                let mut m = dem_ip.lock().unwrap();
                let tong: usize = m.values().sum();
                let rieng = *m.get(&ip).unwrap_or(&0);
                let qt = tong >= TRAN_TOAN_CUC || rieng >= GIOI_HAN_IP;
                if !qt {
                    *m.entry(ip.clone()).or_insert(0) += 1;
                }
                qt
            };
            if qua_tai {
                stat.lock().unwrap().tu_choi_503 += 1;
                let _ = conn.write_all(b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
                continue;
            }
            if doc_tieu_de_co_han(&mut conn) {
                let _ = conn.write_all(TRA_LOI);
                stat.lock().unwrap().xong += 1;
            } else {
                stat.lock().unwrap().dong_cham += 1; // đóng kết nối gửi tiêu đề quá chậm
            }
            let mut m = dem_ip.lock().unwrap();
            if let Some(c) = m.get_mut(&ip) {
                *c -= 1;
                if *c == 0 {
                    m.remove(&ip);
                }
            }
        } else {
            // vulnerable: KẸT Ở ĐÂY nếu client nhỏ giọt
            if doc_tieu_de_khong_han(&mut conn) {
                let _ = conn.write_all(TRA_LOI);
                stat.lock().unwrap().xong += 1;
            }
        }
    }
}

pub fn chay_server(mode: &str, host: &str, port: &str) {
    let listener = TcpListener::bind(format!("{host}:{port}")).unwrap_or_else(|e| {
        eprintln!("[server:{mode}] Không mở được cổng {port}: {e}");
        std::process::exit(1)
    });
    let stat = Arc::new(Mutex::new(ThongKe::default()));
    let dang_chay = Arc::new(AtomicBool::new(true));
    let dem_ip: DemIp = Arc::new(Mutex::new(HashMap::new()));
    println!("[server:{mode}] Nghe tại {host}:{port}, nhóm {SO_LUONG_WORKER} worker. Ctrl+C để dừng.");
    if mode == "protected" {
        println!(
            "[server:protected] Phòng thủ: hạn đọc tiêu đề {:?}s, hạn rỗi {:?}s, tiêu đề <= {}B, <= {} kết nối/IP, trần {} kết nối.",
            HAN_DOC_TIEU_DE.as_secs_f64(), HAN_CHOT_ROI.as_secs_f64(), CO_TIEU_DE_TOI_DA, GIOI_HAN_IP, TRAN_TOAN_CUC
        );
    } else {
        println!("[server:vulnerable] KHÔNG có hạn đọc tiêu đề: mỗi kết nối chậm giữ một worker.");
    }

    // Ctrl+C (SIGINT từ tools/demo.py): in tổng kết rồi thoát, giống khối finally của bản Python.
    {
        let st = Arc::clone(&stat);
        let md = mode.to_string();
        ctrlc::set_handler(move || {
            let s = st.lock().unwrap();
            println!(
                "[server:{md}] Dừng. Tổng kết: chấp nhận {}, phục vụ xong {}, đóng vì chậm {}, từ chối 503 {}.",
                s.chap_nhan, s.xong, s.dong_cham, s.tu_choi_503
            );
            std::process::exit(0);
        })
        .expect("đặt bộ xử lý Ctrl+C");
    }

    thread::scope(|sc| {
        for _ in 0..SO_LUONG_WORKER {
            let (l, md, st, dc, di) = (&listener, mode, &stat, &*dang_chay, &dem_ip);
            sc.spawn(move || worker(l, md, st, dc, di));
        }
        loop {
            thread::sleep(Duration::from_millis(500));
        }
    });
}
