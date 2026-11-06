//! Bài mở rộng MR6.8 – Người đặt vé xem phim (bản dòng lệnh).
//!
//! Kết nối máy chủ, in diễn biến (ghế mình giữ hết hạn, bị người khác lấy) và nhận lệnh:
//!   giu E7 [E8 …]   giữ chỗ (có thời hạn)          tt [E7 …]    thanh toán (mặc định: các ghế vừa giữ)
//!   nha [E7 …]      nhả ghế đang giữ               xem          in sơ đồ ghế          thoat
//! Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//!
//! Chạy:  dat_ve <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

use crate::chung::*;
use crate::khach::KetNoiKhach;
use crate::may_chu::CONG;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Chủ của ghế g theo cap_nhat (None: trống).
pub fn chu_ghe<'a>(tt: &'a Obj, g: &str) -> Option<&'a str> {
    tt.get("chu").and_then(|c| c.get(g)).and_then(Value::as_str)
}

/// Các dòng sơ đồ ghế: . trống, g người khác giữ, b đã bán, G mình giữ, V vé của mình.
pub fn so_do_chu(tt: &Obj, ten: &str) -> Vec<String> {
    let cot = lay_i64(tt, "cot").unwrap_or(12) as usize;
    let mut ds = vec!["      [ MÀN HÌNH ]".to_string(), format!("   {}", (1..=cot).map(|c| (c % 10).to_string()).collect::<Vec<_>>().join(" "))];
    for (r, dong) in lay_str(tt, "hang").unwrap_or("").chars().zip(tt.get("so_do").and_then(Value::as_array).into_iter().flatten()) {
        let o: Vec<String> = dong
            .as_str()
            .unwrap_or("")
            .chars()
            .enumerate()
            .map(|(c, x)| {
                let cua_toi = chu_ghe(tt, &format!("{r}{}", c + 1)) == Some(ten);
                match (cua_toi, x) {
                    (true, 'g') => "G".into(),
                    (true, 'b') => "V".into(),
                    _ => x.to_string(),
                }
            })
            .collect();
        ds.push(format!("{r}  {}", o.join(" ")));
    }
    ds
}

fn ds_chuoi(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default()
}

struct TrangThai {
    tt: Obj,               // cap_nhat mới nhất
    gio_hang: Vec<String>, // các ghế vừa giữ được (để "tt" không cần gõ lại)
    phien_gio: i64,        // số thứ tự lần ghi (kq) của lần giữ chỗ gần nhất
    da_bao: HashSet<String>, // ghế trong giỏ đã báo hết hạn / bị lấy
}

struct NguoiDatVe {
    ten: String,
    k: Mutex<TrangThai>,
    kn: Arc<KetNoiKhach>,
}

impl NguoiDatVe {
    fn log(&self, text: &str) {
        in_dong(&self.ten, text);
    }

    // ------------------------------------------------------------ nhận
    /// Ghế trong giỏ không còn mang tên mình: hết hạn (trống) hoặc bị người khác lấy. Chỉ tin cap_nhat
    /// chụp SAU lần giữ của mình (tt["phien"] ≥ phien của kq). Giỏ vẫn giữ nguyên – lệnh "tt" sau đó sẽ bị
    /// máy chủ từ chối, đúng như khi trang thanh toán mở từ trước bấm "Thanh toán" quá hạn.
    fn cap_nhat(&self, tt: &Obj) {
        let gio: Vec<String> = {
            let mut k = self.k.lock().unwrap();
            k.tt = tt.clone();
            if lay_i64(tt, "phien").unwrap_or(0) < k.phien_gio {
                return;
            }
            k.gio_hang.iter().filter(|g| !k.da_bao.contains(*g)).cloned().collect()
        };
        for g in gio {
            match chu_ghe(tt, &g) {
                Some(ai) if ai == self.ten => continue,
                None => self.log(&format!("  Hết thời gian giữ chỗ: {g} đã được nhả")),
                Some(ai) => self.log(&format!("  Ghế {g} đã bị {ai} lấy mất!")),
            }
            self.k.lock().unwrap().da_bao.insert(g);
        }
    }

    fn tin_khac(&self, tin: &Obj) {
        if lay_str(tin, "loai") == Some("su_kien") {
            self.log(&format!("* {}", lay_str(tin, "noi_dung").unwrap_or("")));
        }
    }

    // ------------------------------------------------------------ gửi
    fn gui(self: &Arc<Self>, loai: &str, ghe: Vec<String>, xu_ly: fn(&NguoiDatVe, &Obj, f64)) {
        let me = Arc::clone(self);
        if let Err(e) = self.kn.gui_yc(loai, obj(json!({"ghe": ghe})), Some(Box::new(move |tin: &Obj, rtt: f64| xu_ly(&me, tin, rtt)))) {
            self.log(&format!("lỗi: {e}"));
        }
    }

    fn giu(self: &Arc<Self>, ghe: Vec<String>) {
        self.log(&format!("> giữ {}", ghe.join(", ")));
        self.gui("giu_cho", ghe, |me, tin, rtt| {
            if dung(tin, "ok") {
                {
                    let mut k = me.k.lock().unwrap();
                    k.gio_hang = ds_chuoi(tin.get("ghe"));
                    k.phien_gio = lay_i64(tin, "phien").unwrap_or(0);
                    k.da_bao.clear();
                }
                me.log(&format!("  ĐÃ GIỮ ({rtt:.1} ms), thanh toán trong {:.0} s", lay_f64(tin, "het_han_ms").unwrap_or(0.0) / 1000.0));
            } else {
                me.log(&format!("  TỪ CHỐI: {}", so_us(tin.get("ly_do"))));
            }
        });
    }

    fn thanh_toan(self: &Arc<Self>, ghe: Vec<String>) {
        self.log(&format!("> thanh toán {}", ghe.join(", ")));
        self.gui("thanh_toan", ghe, |me, tin, rtt| {
            if dung(tin, "ok") {
                let da = ds_chuoi(tin.get("ghe"));
                me.k.lock().unwrap().gio_hang.retain(|g| !da.contains(g));
                me.log(&format!("  ĐẶT VÉ THÀNH CÔNG ({rtt:.1} ms): {}", tien(lay_i64(tin, "tien").unwrap_or(0))));
                me.log(&format!("  mã vé {}", ds_chuoi(tin.get("ma_ve")).join(", ")));
            } else {
                me.log(&format!("  TỪ CHỐI: {}", so_us(tin.get("ly_do"))));
            }
        });
    }

    fn nha(self: &Arc<Self>, ghe: Vec<String>) {
        self.log(&format!("> nhả {}", ghe.join(", ")));
        self.gui("nha", ghe, |me, tin, _| {
            if dung(tin, "ok") {
                let da = ds_chuoi(tin.get("ghe"));
                me.k.lock().unwrap().gio_hang.retain(|g| !da.contains(g));
                me.log(&format!("  Đã nhả {}", da.join(", ")));
            } else {
                me.log(&format!("  TỪ CHỐI: {}", so_us(tin.get("ly_do"))));
            }
        });
    }

    fn lenh(self: &Arc<Self>, dong: &str) {
        let p: Vec<&str> = dong.split_whitespace().collect();
        let gio = self.k.lock().unwrap().gio_hang.clone();
        let hoa: Vec<String> = p.iter().skip(1).map(|g| g.to_uppercase()).collect();
        match p.first().copied() {
            Some("giu") if p.len() >= 2 => self.giu(hoa),
            Some(l @ ("tt" | "nha")) => {
                let ghe = if hoa.is_empty() { gio } else { hoa };
                if ghe.is_empty() {
                    self.log("chưa giữ ghế nào");
                } else if l == "tt" {
                    self.thanh_toan(ghe);
                } else {
                    self.nha(ghe);
                }
            }
            Some("xem") => {
                let tt = self.k.lock().unwrap().tt.clone();
                for d in so_do_chu(&tt, &self.ten) {
                    self.log(&d);
                }
            }
            Some("thoat") => self.kn.dong_im(),
            _ => self.log("lệnh: giu E7 [E8 …] | tt [ghế…] | nha [ghế…] | xem | thoat"),
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
    let ng = Arc::new(NguoiDatVe { ten: ten.clone(), k: Mutex::new(TrangThai { tt: tt.clone(), gio_hang: Vec::new(), phien_gio: 0, da_bao: HashSet::new() }), kn: Arc::clone(&kn) });
    let d = tt.get("dem").cloned().unwrap_or(Value::Null);
    ng.log(&format!("Vào rạp: {}, suất {} ({})", so_us(tt.get("phim")), so_us(tt.get("suat")), so_us(tt.get("che_do"))));
    ng.log(&format!(
        "  trống {}, giữ {}, bán {}; vé {}, giữ {} s",
        so_us(d.get("trong")),
        so_us(d.get("giu")),
        so_us(d.get("ban")),
        tien(lay_i64(&tt, "gia_ve").unwrap_or(0)),
        fmt_g(lay_f64(&tt, "ttl_s").unwrap_or(0.0))
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
