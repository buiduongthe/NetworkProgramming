//! Bài mở rộng MR6.12 – Khách hàng ngân hàng (bản dòng lệnh).
//!
//! Đăng nhập một tài khoản (mặc định: tài khoản mang tên mình, không có thì TK01), in số dư, báo tiền đến
//! và nhận lệnh:
//!   chuyen <TK> <số tiền> [nội dung]     chuyển khoản        so_du     in số dư        thoat
//! Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//!
//! Chạy:  ngan_hang <tên> [host] [cổng] [--tk TK01] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

use crate::chung::*;
use crate::khach::KetNoiKhach;
use crate::may_chu::{so_du_cua, CONG};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct KhachHang {
    ten: String,
    tk: String,
    tt: Mutex<(Obj, HashSet<i64>)>, // cap_nhat mới nhất + ma_gd đã biết (không báo tiền đến hai lần)
    kn: Arc<KetNoiKhach>,
}

fn chu_cua(tt: &Obj, ma: &str) -> String {
    tt.get("tai_khoan").and_then(Value::as_array).into_iter().flatten().find(|t| t.get("ma").and_then(Value::as_str) == Some(ma)).and_then(|t| t.get("chu")).map(py_str).unwrap_or_else(|| "?".into())
}

fn ds_gd(tt: &Obj) -> Vec<Obj> {
    tt.get("lich_su").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_object().cloned()).collect()).unwrap_or_default()
}

impl KhachHang {
    fn log(&self, text: &str) {
        in_dong(&self.ten, text);
    }

    // ------------------------------------------------------------ nhận
    fn cap_nhat(&self, tt: &Obj) {
        let moi: Vec<Obj> = {
            let mut g = self.tt.lock().unwrap();
            g.0 = tt.clone();
            let moi: Vec<Obj> = ds_gd(tt).into_iter().rev().filter(|x| !g.1.contains(&lay_i64(x, "ma").unwrap_or(-1))).collect();
            for x in &moi {
                g.1.insert(lay_i64(x, "ma").unwrap_or(-1));
            }
            moi
        };
        for x in moi {
            if lay_str(&x, "den") == Some(self.tk.as_str()) {
                let tu = lay_str(&x, "tu").unwrap_or("").to_string();
                self.log(&format!("+ Nhận {} từ {tu} ({}), số dư {}", tien(lay_i64(&x, "so_tien").unwrap_or(0)), chu_cua(tt, &tu), tien(so_du_cua(tt, &self.tk))));
            }
        }
    }

    fn tin_khac(&self, tin: &Obj) {
        if lay_str(tin, "loai") == Some("su_kien") {
            self.log(&format!("* {}", lay_str(tin, "noi_dung").unwrap_or("")));
        }
    }

    // ------------------------------------------------------------ gửi
    fn chuyen(self: &Arc<Self>, den: &str, so_tien: i64, noi_dung: &str) {
        self.log(&format!("> chuyển {} {}→{den}", tien(so_tien), self.tk));
        let me = Arc::clone(self);
        let kq = self.kn.gui_yc(
            "chuyen",
            obj(json!({"tu": self.tk, "den": den, "so_tien": so_tien, "noi_dung": noi_dung})),
            Some(Box::new(move |tin: &Obj, rtt: f64| {
                if dung(tin, "ok") {
                    me.tt.lock().unwrap().1.insert(lay_i64(tin, "ma_gd").unwrap_or(-1));
                    me.log(&format!("  THÀNH CÔNG ({rtt:.1} ms), số dư {}", tien(lay_i64(tin, "so_du").unwrap_or(0))));
                } else {
                    me.log(&format!("  TỪ CHỐI ({rtt:.0} ms): {}", tin.get("ly_do").map(py_str).unwrap_or_else(|| "None".into())));
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
            ["chuyen", den, x, nd @ ..] => match x.parse::<i64>() {
                Ok(so_tien) => self.chuyen(den, so_tien, &nd.join(" ")),
                Err(_) => self.log(&format!("lỗi: invalid literal for int() with base 10: '{x}'")),
            },
            ["so_du", ..] => {
                let tt = self.tt.lock().unwrap().0.clone();
                self.log(&format!("Số dư {}: {}", self.tk, tien(so_du_cua(&tt, &self.tk))));
            }
            ["thoat", ..] => self.kn.dong_im(),
            _ => self.log("lệnh: chuyen <TK> <số tiền> [nội dung] | so_du | thoat"),
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
    let ds: Vec<Obj> = tt.get("tai_khoan").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_object().cloned()).collect()).unwrap_or_default();
    // tài khoản mang tên mình, không có thì TK01
    let tk = a.opt.get("tk").cloned().unwrap_or_else(|| {
        ds.iter().find(|t| lay_str(t, "chu") == Some(ten.as_str())).or(ds.first()).and_then(|t| lay_str(t, "ma")).unwrap_or("TK01").to_string()
    });
    let da_thay: HashSet<i64> = ds_gd(&tt).iter().filter_map(|g| lay_i64(g, "ma")).collect();
    let kh = Arc::new(KhachHang { ten: ten.clone(), tk: tk.clone(), tt: Mutex::new((tt.clone(), da_thay)), kn: Arc::clone(&kn) });
    kh.log(&format!("Đăng nhập {tk} – {} ({})", chu_cua(&tt, &tk), lay_str(&tt, "che_do").unwrap_or("")));
    kh.log(&format!("  số dư {}", tien(so_du_cua(&tt, &tk))));
    let (n1, n2, n3) = (Arc::clone(&kh), Arc::clone(&kh), Arc::clone(&kh));
    kn.dat_khi_cap_nhat(Box::new(move |tin| n1.cap_nhat(tin)));
    kn.dat_khi_tin(Box::new(move |tin| n2.tin_khac(tin)));
    kn.dat_khi_dong(Box::new(move || n3.log("Mất kết nối máy chủ")));
    kn.bat_dau_doc();
    let het_stdin = Arc::new(AtomicBool::new(false));
    let (kh2, het) = (Arc::clone(&kh), Arc::clone(&het_stdin));
    std::thread::spawn(move || doc_lenh_stdin(t0, |d| kh2.lenh(d), || het.store(true, Ordering::SeqCst)));
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
