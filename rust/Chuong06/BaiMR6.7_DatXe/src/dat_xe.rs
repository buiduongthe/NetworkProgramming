//! Bài mở rộng MR6.7 – Người gọi xe (bản dòng lệnh).
//!
//! Kết nối máy chủ, in diễn biến (số xe trống, xe của mình bị "nhận khách khác") và nhận lệnh:
//!   dat            đặt xe từ điểm đón mặc định của mình (theo tên) tới điểm đến mặc định
//!   huy            hủy chuyến đã đặt                       thoat
//! Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//!
//! Chạy:  dat_xe <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

use crate::chung::*;
use crate::khach::KetNoiKhach;
use crate::may_chu::{cuoc, json_diem, km, nga_tu, ten_diem, Diem, CONG};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Điểm đón và điểm đến cố định theo tên (để mỗi người một chỗ đứng khác nhau).
pub fn diem_mac_dinh(ten: &str) -> (Diem, Diem) {
    let s: usize = ten.chars().map(|c| c as usize).sum();
    let nt = nga_tu();
    let don = nt[(s * 7) % nt.len()];
    let mut den = nt[(s * 11 + 9) % nt.len()];
    if km(don, den) < 5.0 {
        // quá gần: chọn góc xa nhất
        den = (if don.0 < 0.5 { 0.9 } else { 0.1 }, if don.1 < 0.5 { 0.9 } else { 0.1 });
    }
    (don, den)
}

/// Xe có mã `ma` trong cap_nhat.
pub fn xe_ma(tt: &Obj, ma: i64) -> Option<&Value> {
    tt.get("xe")?.as_array()?.iter().find(|x| x.get("ma").and_then(Value::as_i64) == Some(ma))
}

struct TrangThai {
    tt: Obj,             // cap_nhat mới nhất
    xe_toi: Option<i64>, // mã xe mình được báo đặt thành công
    phien_toi: i64,      // số thứ tự lần ghi (kq) của chuyến mình đặt
    dang_huy: bool,
}

struct NguoiGoiXe {
    ten: String,
    don: Diem,
    den: Diem,
    k: Mutex<TrangThai>,
    kn: Arc<KetNoiKhach>,
}

impl NguoiGoiXe {
    fn log(&self, text: &str) {
        in_dong(&self.ten, text);
    }

    // ------------------------------------------------------------ nhận
    fn cap_nhat(&self, tt: &Obj) {
        let mut k = self.k.lock().unwrap();
        let cu = std::mem::replace(&mut k.tt, tt.clone());
        if lay_i64(tt, "trong") != lay_i64(&cu, "trong") {
            self.log(&format!("Xe trống: {}/{}", so_us(tt.get("trong")), so_us(tt.get("so_xe"))));
        }
        if let Some(ma) = k.xe_toi {
            // bỏ qua ảnh chụp trước lần ghi của mình
            if !k.dang_huy && lay_i64(tt, "phien").unwrap_or(0) >= k.phien_toi {
                let xe = xe_ma(tt, ma);
                let khach = xe.and_then(|x| x.get("khach")).and_then(Value::as_str);
                if xe.is_some() && khach.is_some_and(|c| c != self.ten) {
                    self.log(&format!("  Tài xế {} đã nhận khách khác ({})!", so_us(xe.and_then(|x| x.get("tai_xe"))), khach.unwrap_or("")));
                    k.xe_toi = None;
                } else if khach.is_none() {
                    self.log("  Chuyến đã kết thúc (máy chủ đặt lại)");
                    k.xe_toi = None;
                }
            }
        }
    }

    fn tin_khac(&self, tin: &Obj) {
        if lay_str(tin, "loai") == Some("su_kien") {
            self.log(&format!("* {}", lay_str(tin, "noi_dung").unwrap_or("")));
        }
    }

    // ------------------------------------------------------------ gửi
    fn dat(self: &Arc<Self>) {
        self.log(&format!("> Đặt xe – đang tìm tài xế… (dự kiến {})", tien(cuoc(self.don, self.den))));
        let me = Arc::clone(self);
        let kq = self.kn.gui_yc(
            "dat_xe",
            obj(json!({"don": json_diem(self.don), "den": json_diem(self.den)})),
            Some(Box::new(move |tin: &Obj, rtt: f64| {
                let s = |k: &str| so_us(tin.get(k));
                if dung(tin, "ok") {
                    {
                        let mut k = me.k.lock().unwrap();
                        k.xe_toi = lay_i64(tin, "xe");
                        k.phien_toi = lay_i64(tin, "phien").unwrap_or(0);
                    }
                    me.log(&format!("  ĐẶT THÀNH CÔNG – tài xế {} ({rtt:.1} ms)", s("tai_xe")));
                    me.log(&format!("  {}, xe {}, đến sau {} phút, cước {}", s("bien_so"), s("mau"), s("phut"), tien(lay_i64(tin, "cuoc").unwrap_or(0))));
                } else if lay_str(tin, "ma_loi") == Some("het_cho") {
                    me.log(&format!("  HẾT CHỖ: {}", s("ly_do")));
                } else {
                    me.log(&format!("  TỪ CHỐI: {}", s("ly_do")));
                }
            })),
        );
        if let Err(e) = kq {
            self.log(&format!("lỗi: {e}"));
        }
    }

    fn huy(self: &Arc<Self>) {
        self.log("> Hủy chuyến");
        self.k.lock().unwrap().dang_huy = true;
        let me = Arc::clone(self);
        let kq = self.kn.gui_yc(
            "huy",
            Obj::new(),
            Some(Box::new(move |tin: &Obj, _rtt: f64| {
                me.k.lock().unwrap().dang_huy = false;
                if dung(tin, "ok") {
                    me.k.lock().unwrap().xe_toi = None;
                    me.log(&format!("  Đã hủy, xe {} ({}) trống", so_us(tin.get("xe")), so_us(tin.get("tai_xe"))));
                } else {
                    me.log(&format!("  TỪ CHỐI: {}", so_us(tin.get("ly_do"))));
                }
            })),
        );
        if let Err(e) = kq {
            self.log(&format!("lỗi: {e}"));
        }
    }

    fn lenh(self: &Arc<Self>, dong: &str) {
        let p: Vec<&str> = dong.split_whitespace().collect();
        match p.as_slice() {
            ["dat"] => self.dat(),
            ["huy"] => self.huy(),
            ["thoat"] => self.kn.dong_im(),
            _ => self.log("lệnh: dat | huy | thoat"),
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
    let (don, den) = diem_mac_dinh(&ten);
    let ng = Arc::new(NguoiGoiXe { ten: ten.clone(), don, den, k: Mutex::new(TrangThai { tt: tt.clone(), xe_toi: None, phien_toi: 0, dang_huy: false }), kn: Arc::clone(&kn) });
    ng.log(&format!("Mở ứng dụng ({}): {}/{} xe trống", so_us(tt.get("che_do")), so_us(tt.get("trong")), so_us(tt.get("so_xe"))));
    ng.log(&format!("  đón: {}", ten_diem(don)));
    ng.log(&format!("  đến: {}", ten_diem(den)));
    let (n1, n2, n3) = (Arc::clone(&ng), Arc::clone(&ng), Arc::clone(&ng));
    kn.dat_khi_cap_nhat(Box::new(move |tin| n1.cap_nhat(tin)));
    kn.dat_khi_tin(Box::new(move |tin| n2.tin_khac(tin)));
    kn.dat_khi_dong(Box::new(move || n3.log("Mất kết nối máy chủ")));
    kn.bat_dau_doc();
    let het_stdin = Arc::new(AtomicBool::new(false));
    let (ng2, het) = (Arc::clone(&ng), Arc::clone(&het_stdin));
    std::thread::spawn(move || doc_lenh_stdin(t0, |d| ng2.lenh(d), || het.store(true, Ordering::SeqCst)));
    if thoi_gian > 0.0 {
        // chạy đúng --thoi-gian giây (hoặc tới khi thoat)
        while kn.dang_mo() && bay_gio() - t0 < thoi_gian {
            ngu(0.1);
        }
    } else {
        // tới khi hết stdin / thoat
        while kn.dang_mo() && !het_stdin.load(Ordering::SeqCst) {
            ngu(0.1);
        }
        ngu(0.3); // chờ trả lời của lệnh cuối
    }
    kn.dong_im();
    ngu(0.05);
    std::process::exit(0);
}
