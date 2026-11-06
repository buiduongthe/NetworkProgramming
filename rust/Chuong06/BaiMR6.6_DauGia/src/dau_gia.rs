//! Bài mở rộng MR6.6 – Người tham gia đấu giá (bản dòng lệnh).
//!
//! Kết nối máy chủ, in diễn biến phiên (giá mới, bị trả giá cao hơn, gia hạn, người thắng) và nhận lệnh:
//!   tra <giá>       trả đúng giá này                tang [n]    trả giá hiện tại + n bước giá (mặc định 1)
//!   thoat
//! Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//!
//! Chạy:  dau_gia <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

use crate::chung::*;
use crate::khach::KetNoiKhach;
use crate::may_chu::{so_nguyen, CONG};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct NguoiThamGia {
    ten: String,
    tt: Mutex<Obj>, // cap_nhat mới nhất
    kn: Arc<KetNoiKhach>,
}

fn buoc_gia(tt: &Obj) -> i64 {
    so_nguyen(tt.get("mon").and_then(|m| m.get("buoc_gia")))
}

fn ai(tt: &Obj) -> Option<String> {
    lay_str(tt, "nguoi_dan_dau").map(str::to_string)
}

impl NguoiThamGia {
    fn log(&self, text: &str) {
        in_dong(&self.ten, text);
    }

    // ------------------------------------------------------------ nhận
    fn cap_nhat(&self, tt: &Obj) {
        let cu = std::mem::replace(&mut *self.tt.lock().unwrap(), tt.clone());
        if (lay_i64(tt, "gia"), ai(tt)) != (lay_i64(&cu, "gia"), ai(&cu)) {
            self.log(&format!("Giá mới {} – {} dẫn đầu", tien(lay_i64(tt, "gia").unwrap_or(0)), ai(tt).unwrap_or_else(|| "None".into())));
            if ai(&cu).as_deref() == Some(self.ten.as_str()) && ai(tt).as_deref() != Some(self.ten.as_str()) {
                self.log("  Bạn bị trả giá cao hơn!");
            }
        }
        if dung(tt, "ket_thuc") && !dung(&cu, "ket_thuc") {
            self.log(if ai(tt).as_deref() == Some(self.ten.as_str()) { "  BẠN THẮNG!" } else { "  Bạn không thắng phiên này" });
        }
    }

    fn tin_khac(&self, tin: &Obj) {
        if lay_str(tin, "loai") == Some("su_kien") {
            self.log(&format!("* {}", lay_str(tin, "noi_dung").unwrap_or("")));
        }
    }

    // ------------------------------------------------------------ gửi
    fn tra(self: &Arc<Self>, gia: i64, ghi_chu: &str) {
        self.log(&format!("> trả {}{ghi_chu}", tien(gia)));
        let me = Arc::clone(self);
        let kq = self.kn.gui_yc(
            "tra_gia",
            obj(serde_json::json!({"gia": gia})),
            Some(Box::new(move |tin: &Obj, rtt: f64| {
                if dung(tin, "ok") {
                    let them = if dung(tin, "gia_han") { ", phiên được GIA HẠN" } else { "" };
                    me.log(&format!("  CHẤP NHẬN ({rtt:.1} ms){them}"));
                } else {
                    me.log(&format!("  TỪ CHỐI: {}", tin.get("ly_do").map(py_str).unwrap_or_else(|| "None".into())));
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
            ["tra", g] => match g.parse::<i64>() {
                Ok(gia) => self.tra(gia, ""),
                Err(_) => self.log(&format!("lỗi: invalid literal for int() with base 10: '{g}'")),
            },
            ["tang"] | ["tang", _] => {
                let n = match p.get(1).map(|x| x.parse::<i64>()) {
                    None => 1,
                    Some(Ok(n)) => n,
                    Some(Err(_)) => {
                        self.log(&format!("lỗi: invalid literal for int() with base 10: '{}'", p[1]));
                        return;
                    }
                };
                let gia = {
                    let tt = self.tt.lock().unwrap();
                    lay_i64(&tt, "gia").unwrap_or(0) + n * buoc_gia(&tt)
                };
                self.tra(gia, &format!(" (+{n} bước)"));
            }
            ["thoat"] => self.kn.dong_im(),
            _ => self.log("lệnh: tra <giá> | tang [n] | thoat"),
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
    let ng = Arc::new(NguoiThamGia { ten: ten.clone(), tt: Mutex::new(tt.clone()), kn: Arc::clone(&kn) });
    let con = match tt.get("con_lai_ms") {
        Some(Value::Number(ms)) => format!("còn {:.1} s", ms.as_f64().unwrap_or(0.0) / 1000.0),
        _ => "không giới hạn".into(),
    };
    let mon = tt.get("mon").and_then(Value::as_object).cloned().unwrap_or_default();
    ng.log(&format!("Vào phòng: {} ({})", lay_str(&mon, "ten").unwrap_or(""), lay_str(&tt, "che_do").unwrap_or("")));
    ng.log(&format!("  giá {}, bước {}, {con}", tien(lay_i64(&tt, "gia").unwrap_or(0)), tien(buoc_gia(&tt))));
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
