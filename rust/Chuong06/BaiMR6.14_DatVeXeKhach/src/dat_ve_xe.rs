//! Bài mở rộng MR6.14 – Người đặt vé xe khách (bản dòng lệnh).
//!
//! Kết nối máy chủ, in diễn biến (chỗ mình giữ hết hạn, bị người khác lấy) và nhận lệnh:
//!   giu SG DL D01 [D02 …]   giữ chỗ đoạn Sài Gòn → Đà Lạt (mã trạm SG, BL, DL, NT; có thời hạn)
//!   tt                      thanh toán các chỗ vừa giữ          nha       nhả các chỗ vừa giữ
//!   xem                     in sơ đồ xe (mỗi chỗ 3 ký tự, một ký tự mỗi chặng)            thoat
//! Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//!
//! Chạy:  dat_ve_xe <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

use crate::chung::*;
use crate::khach::KetNoiKhach;
use crate::may_chu::{o_cua, ten_doan, CONG, MA_TRAM, SO_CHANG};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Chủ của ô o ("D01#1") theo cap_nhat (None: trống).
pub fn chu_o<'a>(tt: &'a Obj, o: &str) -> Option<&'a str> {
    tt.get("chu").and_then(|c| c.get(o)).and_then(Value::as_str)
}

fn ds_chuoi(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default()
}

/// Các dòng sơ đồ xe. Mỗi chỗ: mã rồi một ký tự mỗi chặng (. trống, g người khác giữ, b đã bán, G mình giữ,
/// V vé của mình).
pub fn so_do_chu(tt: &Obj, ten: &str) -> Vec<String> {
    let ghe = ds_chuoi(tt.get("ghe"));
    let so_do = ds_chuoi(tt.get("so_do"));
    let bo_tri: Vec<[usize; 3]> = tt
        .get("bo_tri")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|x| { let v = x.as_array().cloned().unwrap_or_default(); let n = |i: usize| v.get(i).and_then(Value::as_u64).unwrap_or(0) as usize; [n(0), n(1), n(2)] }).collect())
        .unwrap_or_default();
    let o: Vec<String> = ghe
        .iter()
        .zip(&so_do)
        .map(|(g, dong)| {
            let v: String = dong.chars().enumerate().map(|(k, x)| if chu_o(tt, &format!("{g}#{k}")) == Some(ten) { match x { 'g' => 'G', 'b' => 'V', c => c } } else { x }).collect();
            format!("{g}:{v}")
        })
        .collect();
    let rong = o.iter().map(|x| x.chars().count()).max().unwrap_or(0);
    let so_tang = bo_tri.iter().map(|b| b[0] + 1).max().unwrap_or(1);
    let so_cot = bo_tri.iter().map(|b| b[2] + 1).max().unwrap_or(1);
    let chang: Vec<String> = (0..SO_CHANG).map(|k| format!("{}-{}", MA_TRAM[k], MA_TRAM[k + 1])).collect();
    let mut ds = vec![format!("  {} – chặng: {}", so_us(tt.get("ten_xe")), chang.join(" | "))];
    for tang in 0..so_tang {
        if so_tang > 1 {
            ds.push(if tang == 0 { "  Tầng dưới:".into() } else { "  Tầng trên:".into() });
        }
        let mut hang: BTreeMap<usize, BTreeMap<usize, String>> = BTreeMap::new();
        for (i, b) in bo_tri.iter().enumerate() {
            if b[0] == tang && i < o.len() {
                hang.entry(b[1]).or_default().insert(b[2], o[i].clone());
            }
        }
        for h in hang.values() {
            let p: Vec<String> = (0..so_cot).map(|c| { let x = h.get(&c).cloned().unwrap_or_default(); let n = x.chars().count(); x + &" ".repeat(rong - n) }).collect();
            ds.push(format!("   {}", p.join(" ")).trim_end().to_string());
        }
    }
    ds
}

#[derive(Clone)]
struct Gio {
    ghe: Vec<String>,
    tu: usize,
    den: usize,
}

struct TrangThai {
    tt: Obj,                 // cap_nhat mới nhất
    gio_hang: Option<Gio>,   // chỗ vừa giữ được (để "tt"/"nha" không cần gõ lại)
    phien_gio: i64,          // số thứ tự lần ghi (kq) của lần giữ chỗ gần nhất
    da_bao: HashSet<String>, // ô trong giỏ đã báo hết hạn / bị lấy
}

struct NguoiDatVeXe {
    ten: String,
    k: Mutex<TrangThai>,
    kn: Arc<KetNoiKhach>,
}

impl NguoiDatVeXe {
    fn log(&self, text: &str) {
        in_dong(&self.ten, text);
    }

    // ------------------------------------------------------------ nhận
    /// Ô trong giỏ không còn mang tên mình: hết hạn (trống) hoặc bị người khác lấy. Chỉ tin cap_nhat chụp SAU
    /// lần giữ của mình (tt["phien"] ≥ phien của kq).
    fn cap_nhat(&self, tt: &Obj) {
        let o: Vec<String> = {
            let mut k = self.k.lock().unwrap();
            k.tt = tt.clone();
            let Some(g) = k.gio_hang.clone() else { return };
            if lay_i64(tt, "phien").unwrap_or(0) < k.phien_gio {
                return;
            }
            o_cua(&g.ghe, g.tu, g.den).into_iter().filter(|x| !k.da_bao.contains(x)).collect()
        };
        let mut bao: Vec<(String, Option<String>)> = Vec::new();
        for x in o {
            let ai = chu_o(tt, &x);
            if ai == Some(self.ten.as_str()) {
                continue;
            }
            let g = x.split_once('#').map(|p| p.0.to_string()).unwrap_or_default();
            if !bao.iter().any(|(c, _)| *c == g) {
                bao.push((g, ai.map(str::to_string)));
            }
            self.k.lock().unwrap().da_bao.insert(x);
        }
        for (g, ai) in bao {
            match ai {
                None => self.log(&format!("  Hết thời gian giữ chỗ: {g} đã được nhả")),
                Some(ai) => self.log(&format!("  Chỗ {g} đã bị {ai} lấy mất!")),
            }
        }
    }

    fn tin_khac(&self, tin: &Obj) {
        if lay_str(tin, "loai") == Some("su_kien") {
            self.log(&format!("* {}", lay_str(tin, "noi_dung").unwrap_or("")));
        }
    }

    // ------------------------------------------------------------ gửi
    fn gui(self: &Arc<Self>, loai: &str, g: &Gio, xu_ly: Box<dyn Fn(&NguoiDatVeXe, &Obj, f64) + Send>) {
        let me = Arc::clone(self);
        if let Err(e) = self.kn.gui_yc(loai, obj(json!({"ghe": g.ghe, "tu": g.tu, "den": g.den})), Some(Box::new(move |tin: &Obj, rtt: f64| xu_ly(&me, tin, rtt)))) {
            self.log(&format!("lỗi: {e}"));
        }
    }

    fn giu(self: &Arc<Self>, g: Gio) {
        self.log(&format!("> giữ {} {}", g.ghe.join(", "), ten_doan(g.tu, g.den)));
        let (tu, den) = (g.tu, g.den);
        self.gui(
            "giu_cho",
            &g,
            Box::new(move |me, tin, rtt| {
                if dung(tin, "ok") {
                    {
                        let mut k = me.k.lock().unwrap();
                        k.gio_hang = Some(Gio { ghe: ds_chuoi(tin.get("ghe")), tu, den });
                        k.phien_gio = lay_i64(tin, "phien").unwrap_or(0);
                        k.da_bao.clear();
                    }
                    me.log(&format!("  ĐÃ GIỮ ({rtt:.1} ms), {}, thanh toán trong {:.0} s", tien(lay_i64(tin, "tien").unwrap_or(0)), lay_f64(tin, "het_han_ms").unwrap_or(0.0) / 1000.0));
                } else {
                    me.log(&format!("  TỪ CHỐI: {}", so_us(tin.get("ly_do"))));
                }
            }),
        );
    }

    fn thanh_toan(self: &Arc<Self>, g: Gio) {
        self.log(&format!("> thanh toán {} {}", g.ghe.join(", "), ten_doan(g.tu, g.den)));
        self.gui(
            "thanh_toan",
            &g,
            Box::new(|me, tin, rtt| {
                if dung(tin, "ok") {
                    me.k.lock().unwrap().gio_hang = None;
                    me.log(&format!("  ĐẶT VÉ THÀNH CÔNG ({rtt:.1} ms): {}", tien(lay_i64(tin, "tien").unwrap_or(0))));
                    me.log(&format!("  mã vé {}", ds_chuoi(tin.get("ma_ve")).join(", ")));
                } else {
                    me.log(&format!("  TỪ CHỐI: {}", so_us(tin.get("ly_do"))));
                }
            }),
        );
    }

    fn nha(self: &Arc<Self>, g: Gio) {
        self.log(&format!("> nhả {} {}", g.ghe.join(", "), ten_doan(g.tu, g.den)));
        self.gui(
            "nha",
            &g,
            Box::new(|me, tin, _| {
                if dung(tin, "ok") {
                    me.k.lock().unwrap().gio_hang = None;
                    me.log(&format!("  Đã nhả {}", ds_chuoi(tin.get("ghe")).join(", ")));
                } else {
                    me.log(&format!("  TỪ CHỐI: {}", so_us(tin.get("ly_do"))));
                }
            }),
        );
    }

    fn lenh(self: &Arc<Self>, dong: &str) {
        let p: Vec<&str> = dong.split_whitespace().collect();
        let gio = self.k.lock().unwrap().gio_hang.clone();
        let tram = |s: &str| MA_TRAM.iter().position(|m| *m == s.to_uppercase());
        match p.first().copied() {
            Some("giu") if p.len() >= 4 && tram(p[1]).is_some() && tram(p[2]).is_some() => {
                let ghe: Vec<String> = p[3..].iter().map(|g| g.to_uppercase()).collect();
                self.giu(Gio { ghe, tu: tram(p[1]).unwrap_or(0), den: tram(p[2]).unwrap_or(0) });
            }
            Some(l @ ("tt" | "nha")) => match gio {
                None => self.log("chưa giữ chỗ nào"),
                Some(g) if l == "tt" => self.thanh_toan(g),
                Some(g) => self.nha(g),
            },
            Some("xem") => {
                let tt = self.k.lock().unwrap().tt.clone();
                for d in so_do_chu(&tt, &self.ten) {
                    self.log(&d);
                }
            }
            Some("thoat") => self.kn.dong_im(),
            _ => self.log(&format!("lệnh: giu SG DL D01 [D02 …] | tt | nha | xem | thoat   (trạm: {})", MA_TRAM.join(", "))),
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
    let ng = Arc::new(NguoiDatVeXe { ten: ten.clone(), k: Mutex::new(TrangThai { tt: tt.clone(), gio_hang: None, phien_gio: 0, da_bao: HashSet::new() }), kn: Arc::clone(&kn) });
    let d = tt.get("dem").cloned().unwrap_or(Value::Null);
    ng.log(&format!("Vào {}: chuyến {}, {} ({})", so_us(tt.get("nha_xe")), so_us(tt.get("chuyen")), so_us(tt.get("ten_xe")).to_lowercase(), so_us(tt.get("che_do"))));
    ng.log(&format!(
        "  tuyến {}; chỗ-chặng trống {}, giữ {}, bán {}; giữ chỗ {} s",
        ds_chuoi(tt.get("tram")).join(" – "),
        so_us(d.get("trong")),
        so_us(d.get("giu")),
        so_us(d.get("ban")),
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
