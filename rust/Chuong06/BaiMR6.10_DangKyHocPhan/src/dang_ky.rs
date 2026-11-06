//! Bài mở rộng MR6.10 – Sinh viên đăng ký học phần (bản dòng lệnh).
//!
//! Kết nối máy chủ, in bảng lớp (mã lớp, tên, đã đăng ký/sĩ số) và nhận lệnh:
//!   dk <mã lớp>       đăng ký lớp            dk <mã lớp> x2    "bấm đúp": gửi HAI yêu cầu liên tiếp không chờ
//!   huy <mã lớp>      hủy lớp đã đăng ký     xem               in lại bảng lớp            thoat
//! Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//!
//! Chạy:  dang_ky <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

use crate::chung::*;
use crate::khach::KetNoiKhach;
use crate::may_chu::CONG;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct SinhVien {
    ten: String,
    tt: Mutex<Obj>,
    kn: Arc<KetNoiKhach>,
}

impl SinhVien {
    fn log(&self, text: &str) {
        in_dong(&self.ten, text);
    }

    fn xem(&self) {
        let tt = self.tt.lock().unwrap().clone();
        for x in tt.get("lop").and_then(Value::as_array).into_iter().flatten() {
            let co = x.get("sv").and_then(Value::as_array).is_some_and(|a| a.iter().any(|s| s.as_str() == Some(self.ten.as_str())));
            let ten = x.get("ten").map(py_str).unwrap_or_default();
            self.log(&format!("  {} {ten:<15} {:>3}/{}{}", so_us(x.get("ma")), so_us(x.get("da_dk")), so_us(x.get("si_so")), if co { " *" } else { "" }));
        }
    }

    // ------------------------------------------------------------ nhận
    fn cap_nhat(&self, tt: &Obj) {
        *self.tt.lock().unwrap() = tt.clone();
    }

    fn tin_khac(&self, tin: &Obj) {
        if lay_str(tin, "loai") == Some("su_kien") {
            self.log(&format!("* {}", lay_str(tin, "noi_dung").unwrap_or("")));
        }
    }

    // ------------------------------------------------------------ gửi
    fn gui(self: &Arc<Self>, loai: &'static str, ma: &str) {
        let (me, ma2) = (Arc::clone(self), ma.to_string());
        let kq = self.kn.gui_yc(
            loai,
            obj(json!({"ma_lop": ma})),
            Some(Box::new(move |tin: &Obj, rtt: f64| {
                if dung(tin, "ok") {
                    let viec = if loai == "dang_ky" { "ĐĂNG KÝ" } else { "HỦY" };
                    let si_so = so_us(me.tt.lock().unwrap().get("si_so"));
                    me.log(&format!("  {viec} {ma2} thành công ({}/{si_so}, {rtt:.1} ms)", so_us(tin.get("da_dk"))));
                } else {
                    me.log(&format!("  {ma2}: TỪ CHỐI – {}", tin.get("ly_do").map(py_str).unwrap_or_else(|| "None".into())));
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
            ["dk", ma] => {
                self.log(&format!("> đăng ký {ma}"));
                self.gui("dang_ky", ma);
            }
            ["dk", ma, "x2"] => {
                self.log(&format!("> đăng ký {ma} (bấm đúp: 2 yêu cầu)"));
                self.gui("dang_ky", ma);
                self.gui("dang_ky", ma);
            }
            ["huy", ma] => {
                self.log(&format!("> hủy {ma}"));
                self.gui("huy", ma);
            }
            ["xem", ..] => self.xem(),
            ["thoat", ..] => self.kn.dong_im(),
            _ => self.log("lệnh: dk <mã> [x2] | huy <mã> | xem | thoat"),
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
    let sv = Arc::new(SinhVien { ten: ten.clone(), tt: Mutex::new(tt.clone()), kn: Arc::clone(&kn) });
    sv.log(&format!("Vào cổng đăng ký ({}), sĩ số {}", lay_str(&tt, "che_do").unwrap_or(""), so_us(tt.get("si_so"))));
    sv.xem();
    let (n1, n2, n3) = (Arc::clone(&sv), Arc::clone(&sv), Arc::clone(&sv));
    kn.dat_khi_cap_nhat(Box::new(move |tin| n1.cap_nhat(tin)));
    kn.dat_khi_tin(Box::new(move |tin| n2.tin_khac(tin)));
    kn.dat_khi_dong(Box::new(move || n3.log("Mất kết nối máy chủ")));
    kn.bat_dau_doc();
    let het_stdin = Arc::new(AtomicBool::new(false));
    let (sv2, het) = (Arc::clone(&sv), Arc::clone(&het_stdin));
    std::thread::spawn(move || doc_lenh_stdin(t0, |d| sv2.lenh(d), || het.store(true, Ordering::SeqCst)));
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
