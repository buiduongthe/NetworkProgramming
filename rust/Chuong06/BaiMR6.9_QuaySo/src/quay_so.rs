//! Bài mở rộng MR6.9 – Người tham gia vòng quay may mắn (bản dòng lệnh).
//!
//! Kết nối máy chủ, in diễn biến (ai trúng gì, số quà còn lại) và nhận lệnh:
//!   quay        quay một lượt
//!   quay x2     "bấm đúp": gửi HAI yêu cầu quay liên tiếp không chờ (hai ma_yc khác nhau) – máy chủ phải
//!               chấp nhận đúng một (idempotency theo tên người)
//!   gui_lai [k] gửi lại ĐÚNG ma_yc k (mặc định: của lượt quay gần nhất), như khi mạng chập chờn client
//!               gửi lại – máy chủ không quay lại mà trả lại kết quả cũ kèm "lap_lai": true
//!               (idempotency theo ma_yc)
//!   thoat
//! Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//!
//! Chạy:  quay_so <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

use crate::chung::*;
use crate::khach::KetNoiKhach;
use crate::may_chu::{chu_p, CONG};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn mo_ta_kq(tin: &Obj) -> String {
    if dung(tin, "ok") {
        if dung(tin, "trung") {
            format!("TRÚNG {}!", lay_str(tin, "ten_qua").unwrap_or(""))
        } else {
            "Chúc may mắn lần sau".into()
        }
    } else {
        format!("TỪ CHỐI: {}", tin.get("ly_do").map(py_str).unwrap_or_else(|| "None".into()))
    }
}

struct NguoiQuay {
    ten: String,
    tt: Mutex<Obj>,
    ma_cuoi: Mutex<Option<i64>>, // ma_yc của lượt quay gần nhất (cho gui_lai)
    kn: Arc<KetNoiKhach>,
}

impl NguoiQuay {
    fn log(&self, text: &str) {
        in_dong(&self.ten, text);
    }

    // ------------------------------------------------------------ nhận
    fn cap_nhat(&self, tt: &Obj) {
        let cu = std::mem::replace(&mut *self.tt.lock().unwrap(), tt.clone());
        if lay_i64(tt, "con_lai") != lay_i64(&cu, "con_lai") {
            self.log(&format!("  còn {}/{} quà", so_us(tt.get("con_lai")), so_us(tt.get("so_qua"))));
        }
    }

    fn tin_khac(&self, tin: &Obj) {
        if lay_str(tin, "loai") == Some("su_kien") {
            self.log(&format!("* {}", lay_str(tin, "noi_dung").unwrap_or("")));
        } else if lay_str(tin, "loai") == Some("kq") && dung(tin, "lap_lai") {
            self.log(&format!("  (ma_yc {}, lap_lai) {}", so_us(tin.get("ma_yc")), mo_ta_kq(tin)));
        }
    }

    // ------------------------------------------------------------ gửi
    fn quay(self: &Arc<Self>) {
        let me = Arc::clone(self);
        let kq = self.kn.gui_yc(
            "quay",
            Obj::new(),
            Some(Box::new(move |tin: &Obj, rtt: f64| me.log(&format!("  ma_yc {}: {} ({rtt:.1} ms)", so_us(tin.get("ma_yc")), mo_ta_kq(tin))))),
        );
        match kq {
            Ok(ma) => *self.ma_cuoi.lock().unwrap() = Some(ma),
            Err(e) => self.log(&format!("lỗi: {e}")),
        }
    }

    fn gui_lai(&self, ma: Option<i64>) {
        let Some(ma) = ma.or(*self.ma_cuoi.lock().unwrap()) else {
            self.log("chưa quay lượt nào");
            return;
        };
        self.log(&format!("> gửi lại ma_yc {ma}"));
        // trường "ma_yc" ghi đè số mới do gui_yc đánh: gửi đúng {"loai":"quay","ma_yc":ma}; trả lời có
        // "lap_lai" nên đi tới tin_khac
        if let Err(e) = self.kn.gui_yc("quay", obj(json!({"ma_yc": ma})), None) {
            self.log(&format!("lỗi: {e}"));
        }
    }

    fn lenh(self: &Arc<Self>, dong: &str) {
        let p: Vec<&str> = dong.split_whitespace().collect();
        match p.as_slice() {
            ["quay"] => {
                self.log("> quay");
                self.quay();
            }
            ["quay", "x2"] => {
                self.log("> quay x2 (bấm đúp: 2 yêu cầu liền nhau)");
                self.quay();
                self.quay();
            }
            ["gui_lai"] => self.gui_lai(None),
            ["gui_lai", k] => match k.parse::<i64>() {
                Ok(ma) => self.gui_lai(Some(ma)),
                Err(_) => self.log(&format!("lỗi: invalid literal for int() with base 10: '{k}'")),
            },
            ["thoat", ..] => self.kn.dong_im(),
            _ => self.log("lệnh: quay | quay x2 | gui_lai [k] | thoat"),
        }
    }
}

pub fn run(args: &[String]) {
    let a = Args::parse(args, &[]);
    let ten = a.pos.first().cloned().unwrap_or_else(|| "An".into());
    let host = a.pos.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = a.pos.get(2).and_then(|p| p.parse().ok()).unwrap_or(CONG);
    let thoi_gian = a.f64("thoi-gian", 0.0);
    let t0 = bay_gio();
    let kn = Arc::new(KetNoiKhach::default());
    if let Err(e) = kn.ket_noi(&host, cong, &ten, "nguoi_dung", None, 5.0) {
        in_dong(&ten, &format!("Không kết nối được: {e}"));
        std::process::exit(1);
    }
    let tt = kn.trang_thai().unwrap_or_default();
    let ng = Arc::new(NguoiQuay { ten: ten.clone(), tt: Mutex::new(tt.clone()), ma_cuoi: Mutex::new(None), kn: Arc::clone(&kn) });
    ng.log(&format!(
        "Vào vòng quay ({}): còn {}/{} quà, p {}",
        lay_str(&tt, "che_do").unwrap_or(""),
        so_us(tt.get("con_lai")),
        so_us(tt.get("so_qua")),
        chu_p(lay_f64(&tt, "p").unwrap_or(0.0))
    ));
    let (n1, n2, n3) = (Arc::clone(&ng), Arc::clone(&ng), Arc::clone(&ng));
    kn.dat_khi_cap_nhat(Box::new(move |tin| n1.cap_nhat(tin)));
    kn.dat_khi_tin(Box::new(move |tin| n2.tin_khac(tin)));
    kn.dat_khi_dong(Box::new(move || n3.log("Mất kết nối máy chủ")));
    kn.bat_dau_doc();
    let het_stdin = Arc::new(AtomicBool::new(false));
    let (ng2, het) = (Arc::clone(&ng), Arc::clone(&het_stdin));
    std::thread::spawn(move || doc_lenh_stdin(t0, |d| ng2.lenh(d), || het.store(true, Ordering::SeqCst)));
    if thoi_gian > 0.0 {
        while kn.dang_mo() && bay_gio() - t0 < thoi_gian {
            ngu(0.1);
        }
    } else {
        while kn.dang_mo() && !het_stdin.load(Ordering::SeqCst) {
            ngu(0.1);
        }
        ngu(0.5); // chờ trả lời của lệnh cuối
    }
    kn.dong_im();
    ngu(0.05);
    std::process::exit(0);
}
