
//! thêm ba chỗ nối với may_chu.rs: cờ dòng lệnh không kèm giá trị (CO_DONG_LENH, ví dụ --khoa-ngay-tho),
//! dòng báo sau khi lắng nghe (sau_khoi_dong) và lệnh bàn phím riêng của bài (lenh_rieng, ví dụ toc_do).
//!
//! Lõi ở may_chu.rs (kiểu NV, khung MayChu trong chung.rs, dùng chung với server_gui.rs); tệp này chỉ in
//! nhật ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới) và nhận lệnh bàn phím.
//!
//! Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chạy):
//!   che_do khong_khoa|khoa|lac_quan|hang_doi     tre <ms>     dat_lai [khoa=giá_trị …]     thong_ke   thoat
//!   (tham số đặt lại: xem may_chu::THAM_SO_DAT_LAI)
//!
//! Chạy:  server [host] [cổng] [--che-do khoa] [--tre-ms 2] + tùy chọn của bài (may_chu::TUY_CHON)
//! Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1).

use crate::chung::*;
use crate::may_chu::{lenh_rieng, sau_khoi_dong, tham_so_dong_lenh, CONG, CO_DONG_LENH, NV};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

fn log(text: &str) {
    in_dong("server", text);
}

pub fn run(args: &[String]) {
    let a = Args::parse(args, CO_DONG_LENH);
    let (host, cong) = a.host_cong(CONG);
    let che_do = a.str("che-do", "khoa");
    let bao: Bao = Arc::new(log);
    let sv = match MayChu::<NV>::new(&che_do, a.f64("tre-ms", 2.0), bao, tham_so_dong_lenh(&a)) {
        Ok(s) => s,
        Err(e) => {
            log(&format!("lỗi: {e}"));
            std::process::exit(2);
        }
    };
    if let Err(e) = sv.bat_dau(&host, cong) {
        log(&format!("Không khởi động được: {e}"));
        std::process::exit(1);
    }
    sau_khoi_dong(&a, &log);
    let (tx, rx) = mpsc::channel::<()>();
    let t = tx.clone();
    let _ = ctrlc::set_handler(move || {
        let _ = t.send(()); // Ctrl+C (công cụ demo dừng server bằng SIGINT)
    });
    let s = Arc::clone(&sv);
    let t0 = bay_gio();
    std::thread::spawn(move || {
        doc_lenh_stdin(
            t0,
            |dong| {
                let p: Vec<&str> = dong.split_whitespace().collect();
                log(&format!("> {dong}"));
                let kq = match p.as_slice() {
                    ["che_do", x] => s.dat_che_do(x),
                    ["tre", x] => x.parse::<f64>().map_err(|_| format!("could not convert string to float: '{x}'")).and_then(|ms| s.dat_tre(ms)),
                    ["dat_lai", ts @ ..] => doc_tham_so(&ts.iter().map(|x| x.to_string()).collect::<Vec<_>>()).and_then(|t| s.dat_lai(&t)),
                    ["thong_ke", ..] => {
                        for d in dong_thong_ke(&s.thong_ke_day_du()) {
                            log(&d);
                        }
                        Ok(())
                    }
                    ["thoat", ..] => {
                        let _ = tx.send(());
                        Ok(())
                    }
                    _ => lenh_rieng(&s, &p).unwrap_or_else(|| {
                        log("lệnh không hợp lệ");
                        Ok(())
                    }),
                };
                if let Err(e) = kq {
                    log(&format!("lỗi: {e}"));
                }
            },
            || {},
        )
    });
    let mut da_in = 0;
    loop {
        // mỗi 5 s: một dòng số liệu nếu có yêu cầu mới
        match rx.recv_timeout(Duration::from_secs(5)) {
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let tk = sv.thong_ke_day_du();
                let so_yc = tk.get("so_yc").and_then(|v| v.as_u64()).unwrap_or(0);
                if so_yc != da_in {
                    da_in = so_yc;
                    let s = |k: &str| so_us(tk.get(k));
                    log(&format!("{}: {} yc, tc {}, từ chối {}, sai phạm {}", s("che_do"), s("so_yc"), s("thanh_cong"), s("tu_choi"), s("sai_pham_tong")));
                }
            }
            _ => break,
        }
    }
    log("Tổng kết (từ lần đặt lại cuối):");
    for d in dong_thong_ke(&sv.thong_ke_day_du()) {
        log(&format!("  {d}"));
    }
    sv.dung();
}
