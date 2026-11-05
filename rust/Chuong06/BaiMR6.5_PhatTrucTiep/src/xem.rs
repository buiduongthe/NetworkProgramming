//! Bài mở rộng MR6.5 – Người xem phát trực tiếp (bản dòng lệnh).
//!
//! Lõi ở nguoi_xem.rs (NguoiXem). Mỗi giây in: số gói tiếng nhận, tổng gói mất, trễ đầu-cuối TB,
//! số khung hiện và lệch tiếng-hình TB trong giây đó; cuối cùng in TỔNG KẾT. Không phát ra loa (bản
//! giao diện xem_gui mới phát loa nếu có thiết bị) – đồng hồ phát vẫn chạy đúng nhịp 20 ms.
//!
//! Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy):
//!   chup <nhãn>   dong_bo bat|tat   dang_ky am_thanh|video|ca_hai   thoat
//!
//! Chạy:  xem <tên> <host> <cổng_tcp> [--thoi-gian 10] [--tre 0] [--dem 5] [--khong-dong-bo]
//!            [--chi-am-thanh] [--chi-video] [--mat-goi 0.0] [--ghep ket_qua_video.png]
//!   --thoi-gian  tổng thời gian chạy (giây, tính cả thời gian chờ --tre)
//!   --tre        chờ bao nhiêu giây rồi mới vào xem (người vào muộn)
//!   --mat-goi    giả lập tỉ lệ mất gói ở chiều nhận (cả tiếng và hình)

use crate::chung::{bay_gio, chu_ms, doc_lenh_stdin, ghep_anh, ghi_png, ngu};
use crate::nguoi_xem::{anh_chup, dong_tong_ket, NguoiXem};
use crate::Args;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

static KHOA_IN: Mutex<()> = Mutex::new(());

pub fn run(a: Args) {
    if a.pos.len() < 3 {
        eprintln!("Cách dùng: xem <tên> <host> <cổng_tcp> [--thoi-gian 10] [--tre 0] [--dem 5] [--khong-dong-bo] [--chi-am-thanh] [--chi-video] [--mat-goi 0] [--ghep tệp.png]");
        std::process::exit(2);
    }
    let (ten, host) = (a.pos[0].clone(), a.pos[1].clone());
    let cong: u16 = a.pos[2].parse().unwrap_or(0);
    let thoi_gian = a.f64("thoi-gian", 10.0);
    let tre = a.f64("tre", 0.0);
    let mat_goi = a.f64("mat-goi", 0.0);
    let ten_in = ten.clone();
    let log = Arc::new(move |text: &str| {
        let _k = KHOA_IN.lock().unwrap(); // nhiều luồng cùng in: mỗi dòng in trọn vẹn
        println!("[xem {ten_in}] {text}");
    });
    let t0 = bay_gio();
    let do_sau = (a.f64("dem", 5.0) as i64).clamp(1, 25) as usize;
    let nx = NguoiXem::new(&ten, log.clone(), do_sau, !a.opt.contains_key("khong-dong-bo"), !a.opt.contains_key("chi-video"), !a.opt.contains_key("chi-am-thanh"), mat_goi, None);
    let anh_da_chup: Arc<Mutex<Vec<(usize, usize, Vec<u8>, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let xong = Arc::new(AtomicBool::new(false));
    let x = Arc::clone(&xong);
    let _ = ctrlc::set_handler(move || x.store(true, Ordering::SeqCst));
    let (n2, x2, l2, a2, t2) = (Arc::clone(&nx), Arc::clone(&xong), log.clone(), Arc::clone(&anh_da_chup), ten.clone());
    doc_lenh_stdin(t0, move |dong| {
        let p: Vec<&str> = dong.split_whitespace().collect();
        l2(&format!("> {dong}"));
        match p.as_slice() {
            ["dong_bo", b @ ("bat" | "tat")] => n2.dat_dong_bo(*b == "bat"),
            ["dang_ky", d @ ("am_thanh" | "video" | "ca_hai")] => n2.dang_ky(*d != "video", *d != "am_thanh"),
            ["chup", nhan @ ..] => {
                let Some(c) = n2.chup() else {
                    l2("chưa có hình để chụp");
                    return;
                };
                let (w, h, rgb) = anh_chup(&c);
                let _ = std::fs::create_dir_all("out");
                let mut ds = a2.lock().unwrap();
                let duong = format!("out/{t2}_chup_{}.png", ds.len() + 1);
                if let Err(e) = ghi_png(&duong, &rgb, w, h) {
                    l2(&format!("Không ghi được {duong}: {e}"));
                    return;
                }
                let nhan = if nhan.is_empty() { duong.clone() } else { nhan.join(" ") };
                ds.push((w, h, rgb, nhan));
                l2(&format!("Đã chụp {duong}: hình {:.3} s, tiếng {}", c.ts as f64 / 1000.0, c.am.map_or("–".into(), |x| format!("{:.3} s", x / 1000.0))));
            }
            ["thoat", ..] => x2.store(true, Ordering::SeqCst),
            _ => l2("lệnh không hợp lệ"),
        }
    });
    if tre > 0.0 {
        log(&format!("Chờ {tre:.1} s rồi mới vào xem (vào muộn)"));
        ngu(tre);
    }
    if let Err(e) = nx.ket_noi(&host, cong) {
        log(&format!("Không kết nối được: {e}"));
        std::process::exit(1);
    }
    let tt = nx.trang_thai();
    let nhan = match (tt.am_thanh, tt.video) {
        (true, true) => "tiếng+hình",
        (true, false) => "tiếng",
        _ => "hình",
    };
    let mat = if mat_goi > 0.0 { format!(", mất {}%", fmt_g(mat_goi * 100.0)) } else { String::new() };
    log(&format!("Nhận {nhan}, đồng bộ {}, jitter {} ms{mat}", if tt.dong_bo { "BẬT" } else { "TẮT" }, tt.do_sau * 20));
    log("Mỗi giây: gói tiếng, mất, trễ ms | khung hiện, lệch ms");
    let (mut giay, mut moc) = (0, bay_gio());
    while bay_gio() - t0 < thoi_gian && !xong.load(Ordering::SeqCst) && nx.da_ket_noi() {
        if bay_gio() - moc >= 1.0 {
            moc += 1.0;
            giay += 1;
            let r = nx.thong_ke_giay();
            log(&format!("{giay:2}s tiếng {:2} mất {} trễ {:>3} | hình {:2} lệch {:>4}", r.am, r.mat, chu_ms(r.tre, false), r.hien, chu_ms(r.lech, true)));
        }
        ngu(0.02);
    }
    let tt = nx.trang_thai();
    nx.dong();
    for d in dong_tong_ket(&tt, tre) {
        log(&d);
    }
    if mat_goi > 0.0 {
        log(&format!("giả lập mất {} gói nhận", tt.bo_gia_lap));
    }
    let ds = anh_da_chup.lock().unwrap();
    if let (Some(g), false) = (a.opt.get("ghep"), ds.is_empty()) {
        match ghep_anh(g, &ds, 2) {
            Ok((w, h)) => log(&format!("Đã ghép {} ảnh vào {g} ({w}x{h})", ds.len())),
            Err(e) => log(&format!("Không ghi được {g}: {e}")),
        }
    }
    std::process::exit(0);
}

/// Số theo kiểu "%g" của Python (3 -> "3", 2.5 -> "2.5").
fn fmt_g(x: f64) -> String {
    let s = format!("{x:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
