//! Bài mở rộng MR6.11 – Người mua flash sale (bản dòng lệnh).
//!
//! Kết nối máy chủ, in diễn biến (đếm ngược, mở bán, tồn kho, hết hàng) và nhận lệnh:
//!   mua [n]     đặt một đơn n sản phẩm (mặc định 1)          thoat
//! Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//!
//! Chạy:  flash_sale <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

use crate::chung::*;
use crate::khach::KetNoiKhach;
use crate::may_chu::CONG;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct NguoiMua {
    ten: String,
    tt: Mutex<Obj>,
    kn: Arc<KetNoiKhach>,
}

impl NguoiMua {
    fn log(&self, text: &str) {
        in_dong(&self.ten, text);
    }

    // ------------------------------------------------------------ nhận
    fn cap_nhat(&self, tt: &Obj) {
        let cu = std::mem::replace(&mut *self.tt.lock().unwrap(), tt.clone());
        if lay_i64(tt, "ton") == Some(0) && lay_i64(&cu, "ton") != Some(0) {
            self.log("  kho: HẾT HÀNG");
        }
    }

    fn tin_khac(&self, tin: &Obj) {
        if lay_str(tin, "loai") == Some("su_kien") {
            self.log(&format!("* {}", lay_str(tin, "noi_dung").unwrap_or("")));
        }
    }

    // ------------------------------------------------------------ gửi
    fn mua(self: &Arc<Self>, sl: i64) {
        self.log(&format!("> MUA NGAY {sl} sản phẩm"));
        let me = Arc::clone(self);
        let kq = self.kn.gui_yc(
            "mua",
            obj(json!({"so_luong": sl})),
            Some(Box::new(move |tin: &Obj, rtt: f64| {
                if dung(tin, "ok") {
                    me.log(&format!("  ĐẶT HÀNG OK: đơn {}, {} ({rtt:.1} ms)", so_us(tin.get("ma_don")), tien(lay_i64(tin, "thanh_tien").unwrap_or(0))));
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
            ["mua"] => self.mua(1),
            ["mua", n] => match n.parse::<i64>() {
                Ok(sl) => self.mua(sl),
                Err(_) => self.log(&format!("lỗi: invalid literal for int() with base 10: '{n}'")),
            },
            ["thoat", ..] => self.kn.dong_im(),
            _ => self.log("lệnh: mua [n] | thoat"),
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
    let ng = Arc::new(NguoiMua { ten: ten.clone(), tt: Mutex::new(tt.clone()), kn: Arc::clone(&kn) });
    let sp = tt.get("sp").and_then(Value::as_object).cloned().unwrap_or_default();
    ng.log(&format!("Vào trang sale ({}): {}", lay_str(&tt, "che_do").unwrap_or(""), lay_str(&sp, "ten").unwrap_or("")));
    ng.log(&format!(
        "  {} -> {}, kho {}/{}",
        tien(lay_i64(&sp, "gia_goc").unwrap_or(0)),
        tien(lay_i64(&sp, "gia_sale").unwrap_or(0)),
        so_us(tt.get("ton")),
        so_us(tt.get("ton_dau"))
    ));
    if !dung(&tt, "mo_ban") {
        ng.log(&format!("  mở bán sau {:.1} s", lay_f64(&tt, "mo_con_ms").unwrap_or(0.0) / 1000.0));
    }
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
