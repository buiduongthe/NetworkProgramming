//! Bài mở rộng MR6.5 – Máy phát trực tiếp (bản dòng lệnh): camera + micro -> máy chủ.
//!
//! Lõi ở may_phat.rs (MayPhat). Mặc định camera giả lập "bảng clapper" (đồng hồ, số khung, chớp
//! trắng mỗi giây tròn) và micro giả lập (440 Hz + bíp 1000 Hz mỗi giây tròn).
//!   --camera testsrc     ảnh mẫu thử của ffmpeg (cần ffmpeg)
//!   --camera <thiết bị>  camera thật qua ffmpeg (/dev/video0, "Tên camera" trên Windows, 0 trên macOS)
//!   --mic <chỉ số>       micro thật qua cpal (chỉ số trong danh sách micro của hệ điều hành, từ 0)
//!   --tep <tệp video>    phát lại hình + tiếng của tệp (mp4, mkv…) qua ffmpeg, lặp lại khi hết tệp;
//!                        thêm --mic gia_lap|<chỉ số> để thay tiếng của tệp bằng micro
//!   --tao-tep-mau        tạo trước tệp mẫu 10 s (chớp trắng + bíp mỗi giây) tại đường dẫn của --tep
//! Thiết bị thật không mở được -> tự lùi về nguồn giả lập (có báo).
//!
//! Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t):  khung_khoa   dung
//!
//! Chạy:  phat <tên> <host> <cổng_tcp> [--camera gia_lap|testsrc|<thiết bị>] [--mic gia_lap|<chỉ số>]
//!            [--tep <tệp video> [--tao-tep-mau]] [--fps 10] [--gop 20] [--thoi-gian 30] [--mat-goi 0.0]

use crate::camera::{cam_chung, co_ffmpeg, CameraFfmpeg, CameraGiaLap, GIA_LAP};
use crate::chung::{bay_gio, doc_lenh_stdin, ngu, FPS_MAC_DINH, GOP_MAC_DINH};
use crate::may_phat::{dong_tong_ket, MayPhat};
use crate::tep_video::{mo_tep, tao_tep_mau};
use crate::thiet_bi::{ds_micro, mic_chung, mo_mic_that, MicGiaLap};
use crate::Args;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

static KHOA_IN: Mutex<()> = Mutex::new(());

pub fn run(a: Args) {
    if a.pos.len() < 3 {
        eprintln!("Cách dùng: phat <tên> <host> <cổng_tcp> [--camera …] [--mic …] [--tep …] [--fps 10] [--gop 20] [--thoi-gian 30] [--mat-goi 0]");
        std::process::exit(2);
    }
    let (ten, host) = (a.pos[0].clone(), a.pos[1].clone());
    let cong: u16 = a.pos[2].parse().unwrap_or(0);
    let fps = a.f64("fps", FPS_MAC_DINH as f64) as u32;
    let gop = a.f64("gop", GOP_MAC_DINH as f64) as u32;
    let thoi_gian = a.f64("thoi-gian", 30.0);
    let mat_goi = a.f64("mat-goi", 0.0);
    let camera = a.opt.get("camera").cloned().unwrap_or_else(|| GIA_LAP.into());
    let ten_in = ten.clone();
    let log = Arc::new(move |text: &str| {
        let _k = KHOA_IN.lock().unwrap(); // nhiều luồng cùng in: mỗi dòng in trọn vẹn
        println!("[phat {ten_in}] {text}");
    });
    let mp = MayPhat::new(&ten, log.clone(), mat_goi);
    let ma = match mp.ket_noi(&host, cong) {
        Ok(m) => m,
        Err(e) => {
            log(&format!("Không kết nối được: {e}"));
            std::process::exit(1);
        }
    };
    let (mut cam, mut mic) = (None, None);
    if let Some(tep) = a.opt.get("tep") {
        let r = (|| {
            if a.opt.contains_key("tao-tep-mau") {
                if let Some(cha) = Path::new(tep).parent().filter(|p| !p.as_os_str().is_empty()) {
                    std::fs::create_dir_all(cha)?;
                }
                tao_tep_mau(tep, 10)?;
                let ten_tep = Path::new(tep).file_name().map(|t| t.to_string_lossy().into_owned()).unwrap_or_default();
                log(&format!("Đã tạo tệp mẫu {ten_tep} (10 s)"));
            }
            mo_tep(tep, fps, !a.opt.contains_key("mic"))
        })();
        match r {
            Ok((c, m, _)) => {
                cam = Some(c);
                mic = m;
            }
            Err(e) => log(&format!("Không mở được tệp video: {e}")),
        }
    }
    let cam = cam.unwrap_or_else(|| {
        if camera != GIA_LAP && co_ffmpeg() {
            if let Ok(c) = CameraFfmpeg::mo(&camera, &camera, fps) {
                return cam_chung(c);
            }
        }
        if camera != GIA_LAP {
            log("Không có ffmpeg: dùng camera giả lập");
        }
        cam_chung(CameraGiaLap::new(&ten, ma, fps))
    });
    let mut _luong_mic = None; // luồng cpal của micro thật: phải giữ tới khi kết thúc
    if mic.is_none() {
        let ten_mic = a.opt.get("mic").cloned().unwrap_or_else(|| "gia_lap".into());
        if ten_mic != "gia_lap" {
            let ds = ds_micro();
            match ten_mic.parse::<usize>().ok().and_then(|i| ds.get(i)).map(|t| mo_mic_that(t)) {
                Some(Ok((luong, m))) => {
                    _luong_mic = Some(luong);
                    mic = Some(m);
                }
                _ => log("Không mở được micro thật: dùng giả lập"),
            }
        }
    }
    let mic = mic.unwrap_or_else(|| mic_chung(MicGiaLap::new()));
    log(&format!("Mã {ma}; camera {}", cam.lock().unwrap().mo_ta()));
    log(&format!("Micro {}", mic.lock().unwrap().mo_ta()));
    let xong = Arc::new(AtomicBool::new(false));
    let x = Arc::clone(&xong);
    let _ = ctrlc::set_handler(move || x.store(true, Ordering::SeqCst));
    let t0 = bay_gio();
    let (m2, x2, l2) = (Arc::clone(&mp), Arc::clone(&xong), log.clone());
    doc_lenh_stdin(t0, move |dong| {
        l2(&format!("> {dong}"));
        match dong {
            "khung_khoa" => m2.khung_khoa_ngay("lệnh khung_khoa"),
            "dung" => x2.store(true, Ordering::SeqCst),
            _ => l2("lệnh không hợp lệ"),
        }
    });
    mp.bat_dau_phat(cam, mic, fps, gop);
    log("Mỗi giây: khung I/P, kbit/s hình, tiếng (cả header)");
    let (mut giay, mut moc) = (0, bay_gio());
    while bay_gio() - t0 < thoi_gian && !xong.load(Ordering::SeqCst) && mp.da_ket_noi() {
        if bay_gio() - moc >= 1.0 {
            moc += 1.0;
            giay += 1;
            let r = mp.thong_ke_giay();
            log(&format!("{giay:2}s khung I {} P {:2} | hình {:4.0} tiếng {:3.0} kbit/s", r.i, r.p, r.kbps_hinh, r.kbps_am));
        }
        ngu(0.02);
    }
    let tong = bay_gio() - t0;
    let tt = mp.trang_thai();
    mp.dong();
    for d in dong_tong_ket(&tt, tong) {
        log(&d);
    }
    if mat_goi > 0.0 {
        log(&format!("giả lập mất {} gói gửi", mp.bo_gia_lap()));
    }
    std::process::exit(0);
}
