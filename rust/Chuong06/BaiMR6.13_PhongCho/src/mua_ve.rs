//! Bài mở rộng MR6.13 – Người mua vé (bản dòng lệnh).
//!
//! Kết nối máy chủ, in thông tin sự kiện, vị trí trong phòng chờ (số người phía trước thay đổi theo cap_nhat),
//! lúc tới lượt và kết quả mua; nhận lệnh:
//!   xep_hang        vào phòng chờ (rồi tự "chờ dài" cho_luot tới khi tới lượt)
//!   mua <n>         mua n vé          mua2 <n>   bấm ĐÚP: gửi hai yêu cầu mua liền nhau        thoat
//! --tu-dong: vào hàng ngay (phòng chờ tắt thì mua ngay), tới lượt thì tự mua --so-ve vé.
//! Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//!
//! Chạy:  mua_ve <tên> [host] [cổng] [--tu-dong] [--so-ve 2] [--thoi-gian 0]

use crate::chung::*;
use crate::khach::KetNoiKhach;
use crate::may_chu::{so_con, CONG};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct TrangThai {
    tt: Obj,
    so: Option<i64>,       // số thứ tự trong hàng
    truoc_cu: Option<i64>,
}

struct NguoiMua {
    ten: String,
    tu_dong: bool,
    so_ve: i64,
    k: Mutex<TrangThai>,
    kn: Arc<KetNoiKhach>,
}

impl NguoiMua {
    fn log(&self, text: &str) {
        in_dong(&self.ten, text);
    }

    // ------------------------------------------------------------ nhận
    fn cap_nhat(&self, tt: &Obj) {
        let (doi, truoc, toc_do) = {
            let mut k = self.k.lock().unwrap();
            k.tt = tt.clone();
            let (Some(so), Some(cu)) = (k.so, k.truoc_cu) else { return };
            let truoc = (so - lay_i64(tt, "dang_phuc_vu").unwrap_or(0) - 1).max(0);
            k.truoc_cu = Some(truoc);
            (truoc != cu, truoc, lay_f64(tt, "toc_do").unwrap_or(1.0))
        };
        if doi && truoc > 0 {
            self.log(&format!("  (hàng chờ) còn {truoc} người phía trước, ~{:.1} s", (truoc + 1) as f64 / toc_do));
        }
    }

    fn tin_khac(&self, tin: &Obj) {
        if lay_str(tin, "loai") == Some("su_kien") {
            self.log(&format!("* {}", lay_str(tin, "noi_dung").unwrap_or("")));
        }
    }

    // ------------------------------------------------------------ gửi
    fn xep_hang(self: &Arc<Self>) {
        self.log("> vào phòng chờ");
        let me = Arc::clone(self);
        let kq = self.kn.gui_yc(
            "xep_hang",
            Obj::new(),
            Some(Box::new(move |tin: &Obj, _rtt: f64| {
                if !dung(tin, "ok") {
                    me.log(&format!("  TỪ CHỐI: {}", tin.get("ly_do").map(py_str).unwrap_or_else(|| "None".into())));
                    return;
                }
                {
                    let mut k = me.k.lock().unwrap();
                    k.so = lay_i64(tin, "so");
                    k.truoc_cu = lay_i64(tin, "truoc");
                }
                me.log(&format!("  số thứ tự #{}, còn {} người phía trước, ~{:.1} s", so_us(tin.get("so")), so_us(tin.get("truoc")), lay_f64(tin, "cho_s").unwrap_or(0.0)));
                let m2 = Arc::clone(&me);
                let _ = me.kn.gui_yc("cho_luot", Obj::new(), Some(Box::new(move |tin: &Obj, rtt: f64| m2.den_luot(tin, rtt))));
            })),
        );
        if let Err(e) = kq {
            self.log(&format!("lỗi: {e}"));
        }
    }

    fn den_luot(self: &Arc<Self>, tin: &Obj, rtt: f64) {
        self.k.lock().unwrap().truoc_cu = None;
        if !dung(tin, "ok") {
            self.log(&format!("  {}", tin.get("ly_do").map(py_str).unwrap_or_else(|| "None".into())));
            return;
        }
        self.log(&format!("  ĐẾN LƯỢT! (chờ {:.1} s) còn {:.1} s để mua", rtt / 1000.0, lay_f64(tin, "han_ms").unwrap_or(0.0) / 1000.0));
        if self.tu_dong {
            self.mua(self.so_ve, 1);
        }
    }

    fn mua(self: &Arc<Self>, n: i64, lan: u32) {
        self.log(&format!("> mua {n} vé{}", if lan > 1 { " (bấm đúp)" } else { "" }));
        for _ in 0..lan {
            // bấm đúp: hai yêu cầu, không chờ trả lời
            let me = Arc::clone(self);
            let kq = self.kn.gui_yc(
                "mua",
                obj(json!({"so_ve": n})),
                Some(Box::new(move |tin: &Obj, rtt: f64| {
                    if dung(tin, "ok") {
                        me.log(&format!("  THÀNH CÔNG ({rtt:.1} ms): đơn #{}, {} vé, {}", so_us(tin.get("ma_don")), so_us(tin.get("so_ve")), tien(lay_i64(tin, "thanh_tien").unwrap_or(0))));
                    } else {
                        me.log(&format!("  TỪ CHỐI ({rtt:.1} ms): {}", tin.get("ly_do").map(py_str).unwrap_or_else(|| "None".into())));
                    }
                })),
            );
            if let Err(e) = kq {
                self.log(&format!("lỗi: {e}"));
            }
        }
    }

    fn lenh(self: &Arc<Self>, dong: &str) {
        let p: Vec<&str> = dong.split_whitespace().collect();
        match p.as_slice() {
            ["xep_hang", ..] => self.xep_hang(),
            [l @ ("mua" | "mua2"), n] => match n.parse::<i64>() {
                Ok(n) => self.mua(n, if *l == "mua2" { 2 } else { 1 }),
                Err(_) => self.log(&format!("lỗi: invalid literal for int() with base 10: '{n}'")),
            },
            ["thoat", ..] => self.kn.dong_im(),
            _ => self.log("lệnh: xep_hang | mua <n> | mua2 <n> | thoat"),
        }
    }
}

pub fn run(args: &[String]) {
    let a = Args::parse(args, &["tu-dong"]);
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
    let ng = Arc::new(NguoiMua { ten: ten.clone(), tu_dong: a.co("tu-dong"), so_ve: a.i64("so-ve", 2), k: Mutex::new(TrangThai { tt: tt.clone(), so: None, truoc_cu: None }), kn: Arc::clone(&kn) });
    let sk = tt.get("su_kien").and_then(|v| v.as_object()).cloned().unwrap_or_default();
    ng.log(&format!("Trang bán vé: {} ({})", lay_str(&sk, "ten").unwrap_or(""), lay_str(&tt, "che_do").unwrap_or("")));
    ng.log(&format!("  còn {}/{} vé, {}/vé", so_us(tt.get("con_lai")), so_con(&tt, "su_kien", "tong_ve"), tien(so_con(&tt, "su_kien", "gia_ve"))));
    let pc = dung(&tt, "phong_cho");
    ng.log(&if pc { format!("  phòng chờ BẬT, cấp lượt {} người/s", fmt_g(lay_f64(&tt, "toc_do").unwrap_or(0.0))) } else { "  phòng chờ TẮT".into() });
    let (n1, n2, n3) = (Arc::clone(&ng), Arc::clone(&ng), Arc::clone(&ng));
    kn.dat_khi_cap_nhat(Box::new(move |tin| n1.cap_nhat(tin)));
    kn.dat_khi_tin(Box::new(move |tin| n2.tin_khac(tin)));
    kn.dat_khi_dong(Box::new(move || n3.log("Mất kết nối máy chủ")));
    kn.bat_dau_doc();
    if ng.tu_dong {
        if pc {
            ng.xep_hang();
        } else {
            ng.mua(ng.so_ve, 1);
        }
    }
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
