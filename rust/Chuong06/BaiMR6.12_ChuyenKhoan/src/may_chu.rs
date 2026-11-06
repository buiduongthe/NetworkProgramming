//! Bài mở rộng MR6.12 – Lõi máy chủ ngân hàng: chuyển khoản + sổ kiểm toán (khung ở chung.rs).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
//! 5–20 tài khoản, mỗi tài khoản là MỘT ô của KhoTranhChap (giá trị = số dư, số nguyên đồng). Thao tác ghi:
//!     {"loai":"chuyen","ma_yc":k,"tu":"TK01","den":"TK02","so_tien":300000[,"noi_dung":"..."][,"nguoi":"u17"]}
//! Luật: trừ tài khoản nguồn, cộng tài khoản đích trong CÙNG một thao tác (hai ô); số dư nguồn không đủ thì
//! từ chối. Nhiều người chuyển chéo cùng lúc (A→B và B→A) nên một thao tác luôn đụng HAI tài nguyên:
//!   khoa            khóa hai tài khoản theo THỨ TỰ MÃ (TK01 trước TK02) -> mọi luồng xin khóa cùng thứ tự,
//!                   không thể có vòng chờ -> không khóa chết;
//!   khoa_ngay_tho=1 khóa nguồn rồi mới khóa đích: A→B giữ A chờ B, B→A giữ B chờ A -> KHÓA CHẾT (deadlock).
//!                   Khung phát hiện bằng hết thời gian chờ khóa (2 s): giao dịch "nạn nhân" bị hủy, nhả khóa,
//!                   giao dịch kia đi tiếp; máy chủ báo "khóa chết: …";
//!   lac_quan        đọc (số dư, phiên bản) của cả hai tài khoản, rồi so sánh-và-đổi CẢ HAI một cách nguyên tử.
//!
//! Sổ kiểm toán ghi mọi giao dịch được chấp nhận và tự tính số dư theo sổ; đếm ba sai phạm:
//!   so_du_am  "thấu chi"  – theo sổ, một giao dịch làm tài khoản nguồn âm (hai luồng cùng thấy "đủ tiền"
//!                          rồi cùng trừ);
//!   so_du_sai "lệch TK"   – số tài khoản có số dư cuối khác số dư theo sổ (mất cập nhật);
//!   tong_lech "tổng lệch" – tổng tiền toàn hệ thống khác tổng ban đầu (tiền tự sinh ra / mất đi).

use crate::chung::*;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const CONG: u16 = 44760; // cổng mặc định (dòng lệnh)
pub const CONG_GUI: u16 = 44762; // cổng mặc định (giao diện)
/// Kiểu nghiệp vụ của bài – server.rs, giao diện dùng tên chung này.
pub type NV = NganHang;

pub const CHU_TK: [&str; 20] = ["An", "Bình", "Chi", "Dũng", "Giang", "Hà", "Khoa", "Lan", "Minh", "Nam", "Oanh", "Phúc", "Quân", "Sơn", "Tâm", "Uyên", "Vy", "Xuân", "Yến", "Thảo"];

/// Cờ dòng lệnh không kèm giá trị của server (server.rs truyền cho Args::parse).
pub const CO_DONG_LENH: &[&str] = &["khoa-ngay-tho"];

/// Tùy chọn dòng lệnh của máy chủ: [--so-tk 10] [--so-du 5000000] [--khoa-ngay-tho]. Tham số đặt lại:
/// so_tk (5–20), so_du_dau, khoa_ngay_tho (0 = khóa theo thứ tự mã, 1 = nguồn rồi đích).
pub fn tham_so_dong_lenh(a: &Args) -> Obj {
    obj(json!({"so_tk": a.i64("so-tk", 10), "so_du_dau": a.i64("so-du", 5_000_000), "khoa_ngay_tho": a.co("khoa-ngay-tho") as i64}))
}

/// Gọi sau khi máy chủ dòng lệnh đã lắng nghe.
pub fn sau_khoi_dong(a: &Args, log: &dyn Fn(&str)) {
    if a.co("khoa-ngay-tho") {
        log("Khóa ngây thơ: nguồn rồi đích (có thể khóa chết)");
    }
}

/// Lệnh bàn phím riêng của bài cho server dòng lệnh (MR6.12: không có) – None: không phải lệnh của bài.
pub fn lenh_rieng(_sv: &Arc<MayChu<NV>>, _p: &[&str]) -> Option<Result<(), String>> {
    None
}

pub fn ma_tk(i: usize) -> String {
    format!("TK{:02}", i + 1)
}

/// +300.000 đ / -300.000 đ / 0 đ.
pub fn tien_dau(x: i64) -> String {
    format!("{}{}", if x > 0 { "+" } else { "" }, tien(x))
}

// ================================================================ sổ kiểm toán
pub struct GdChuyen {
    pub nguoi: String,
    pub tu: String,
    pub den: String,
    pub so_tien: i64,
}

pub struct LuatNganHang {
    so_du: Vec<(String, i64)>, // số dư theo sổ (cộng trừ đúng từng giao dịch đã chấp nhận)
    tong_dau: i64,
}

impl LuatNganHang {
    fn sd(&mut self, ma: &str) -> &mut i64 {
        &mut self.so_du.iter_mut().find(|(m, _)| m == ma).expect("tài khoản không có trong sổ").1
    }
}

impl LuatKiemToan for LuatNganHang {
    type GiaoDich = GdChuyen;
    type GiaTri = i64;
    const TEN_SAI_PHAM: MaTen = &[("tong_lech", "tổng lệch"), ("so_du_sai", "lệch TK"), ("so_du_am", "thấu chi")];

    fn kiem_tra(&mut self, gd: &GdChuyen, sp: &mut SaiPham) {
        *self.sd(&gd.tu) -= gd.so_tien;
        *self.sd(&gd.den) += gd.so_tien;
        let con = *self.sd(&gd.tu);
        if con < 0 {
            sp.bao("so_du_am", format!("{} {}→{}: {} còn {}", gd.nguoi, gd.tu, gd.den, gd.tu, tien(con)));
        }
    }

    fn kiem_tra_cuoi(&self, doc: &Doc<i64>) -> Vec<(&'static str, u64)> {
        if self.so_du.iter().any(|(k, _)| doc.get(k).is_none()) {
            return Vec::new();
        }
        let sai = self.so_du.iter().filter(|(k, v)| doc[k.as_str()] != *v).count() as u64;
        let tong: i64 = self.so_du.iter().map(|(k, _)| doc[k.as_str()]).sum();
        vec![("so_du_sai", sai), ("tong_lech", (tong != self.tong_dau) as u64)]
    }
}

// ================================================================ nghiệp vụ
struct KhoaChet {
    so: u64,
    chua_bao: u64,
    luc_bao: f64,
}

pub struct NganHang {
    lich_su: Mutex<VecDeque<Obj>>, // giao dịch đã chấp nhận, mới nhất trước (tối đa 50)
    ma_gd: AtomicU64,              // mã giao dịch tăng mãi, kể cả qua các lần đặt lại
    so_gd: AtomicU64,
    kc: Mutex<KhoaChet>,
}

impl NganHang {
    fn ds_tk(mc: &MayChu<Self>) -> Vec<String> {
        (0..mc.ts_i64("so_tk").clamp(0, CHU_TK.len() as i64) as usize).map(ma_tk).collect()
    }

    /// Chờ khóa quá 2 s: với khóa ngây thơ đó là khóa chết. Báo (tối đa 1 dòng/giây) và hủy giao dịch.
    fn bao_khoa_chet(mc: &MayChu<Self>, nguoi: &str, tu: &str, den: &str, mut kq: Obj) -> Obj {
        let ly_do = lay_str(&kq, "ly_do").unwrap_or("").to_string();
        let k = ly_do.split_whitespace().nth(2).unwrap_or("?").to_string(); // "chờ khóa TK02 quá 2 s (khóa chết?)"
        let ngay_tho = mc.kho.khoa_ngay_tho.load(Ordering::SeqCst);
        let (bao, them) = {
            let mut g = mc.nv.kc.lock().unwrap();
            g.so += 1;
            let bay = bay_gio();
            if bay - g.luc_bao >= 1.0 {
                g.luc_bao = bay;
                let them = g.chua_bao;
                g.chua_bao = 0;
                (true, them)
            } else {
                g.chua_bao += 1;
                (false, 0)
            }
        };
        let s = fmt_g(CHO_KHOA_TOI_DA_S);
        if bao {
            let dau = if ngay_tho { "khóa chết" } else { "chờ khóa quá hạn" };
            let cuoi = if them > 0 { format!(" (+{them} vụ)") } else { String::new() };
            mc.thong_bao(&format!("{dau}: {nguoi} {tu}→{den}, chờ {k} quá {s} s{cuoi}"));
        }
        kq.insert("ly_do".into(), json!(format!("{}chờ {k} quá {s} s, đã hủy giao dịch", if ngay_tho { "khóa chết: " } else { "" })));
        kq
    }
}

impl NghiepVu for NganHang {
    type GiaTri = i64;
    type Luat = LuatNganHang;
    const TEN: &'static str = "Ngân hàng MR6.12";
    const LOAI_YEU_CAU: &'static [&'static str] = &["chuyen"];

    fn tham_so_mac_dinh() -> Obj {
        obj(json!({"so_tk": 10, "so_du_dau": 5_000_000, "khoa_ngay_tho": 0}))
    }

    fn moi() -> NganHang {
        NganHang { lich_su: Mutex::new(VecDeque::new()), ma_gd: AtomicU64::new(1), so_gd: AtomicU64::new(0), kc: Mutex::new(KhoaChet { so: 0, chua_bao: 0, luc_bao: 0.0 }) }
    }

    // ------------------------------------------------------------ tạo / đặt lại
    fn kiem_tham_so(ts: &Obj) -> Result<(), String> {
        if !(5..=CHU_TK.len() as i64).contains(&lay_i64(ts, "so_tk").unwrap_or(10)) {
            return Err(format!("số tài khoản 5–{}", CHU_TK.len()));
        }
        if lay_i64(ts, "so_du_dau").unwrap_or(0) < 0 {
            return Err("số dư ban đầu ≥ 0".into());
        }
        if !matches!(lay_i64(ts, "khoa_ngay_tho").unwrap_or(0), 0 | 1) {
            return Err("khoa_ngay_tho = 0 hoặc 1".into());
        }
        Ok(())
    }

    fn khoi_tao(mc: &MayChu<Self>) -> (Vec<(String, i64)>, LuatNganHang) {
        let so_du = mc.ts_i64("so_du_dau");
        let ds = Self::ds_tk(mc);
        mc.kho.khoa_ngay_tho.store(mc.ts_i64("khoa_ngay_tho") != 0, Ordering::SeqCst);
        mc.nv.lich_su.lock().unwrap().clear();
        mc.nv.so_gd.store(0, Ordering::SeqCst);
        *mc.nv.kc.lock().unwrap() = KhoaChet { so: 0, chua_bao: 0, luc_bao: 0.0 };
        let o: Vec<(String, i64)> = ds.iter().map(|m| (m.clone(), so_du)).collect();
        (o.clone(), LuatNganHang { tong_dau: so_du * ds.len() as i64, so_du: o })
    }

    // ------------------------------------------------------------ chuyển khoản
    fn xu_ly_yeu_cau(mc: &Arc<MayChu<Self>>, _khach: &KhachKetNoi, nguoi: &str, tin: &Obj, tra_loi: TraLoi) {
        let Some(x) = lay_i64(tin, "so_tien").filter(|&x| x > 0) else {
            tra_loi(obj(json!({"ok": false, "ma_loi": "sai_du_lieu", "ly_do": "số tiền phải là số nguyên dương"})));
            return;
        };
        let ds = Self::ds_tk(mc);
        let (tu, den) = match (lay_str(tin, "tu"), lay_str(tin, "den")) {
            (Some(t), Some(d)) if ds.iter().any(|m| m == t) && ds.iter().any(|m| m == d) => (t.to_string(), d.to_string()),
            _ => {
                tra_loi(obj(json!({"ok": false, "ma_loi": "khong_co", "ly_do": "tài khoản không tồn tại"})));
                return;
            }
        };
        if tu == den {
            tra_loi(obj(json!({"ok": false, "ma_loi": "sai_du_lieu", "ly_do": "tài khoản nguồn và đích trùng nhau"})));
            return;
        }
        let noi_dung = match tin.get("noi_dung") {
            None => String::new(),
            Some(Value::String(s)) if s.chars().count() <= 40 => s.clone(),
            _ => {
                tra_loi(obj(json!({"ok": false, "ma_loi": "sai_du_lieu", "ly_do": "nội dung tối đa 40 ký tự"})));
                return;
            }
        };
        let (t2, d2, nguoi2) = (tu.clone(), den.clone(), nguoi.to_string());
        let xu_ly: XuLy<i64> = Box::new(move |doc| {
            // hàm THUẦN: đọc hai số dư, trả về (kq, ghi)
            let (a, b) = (doc[t2.as_str()], doc[d2.as_str()]);
            if a < x {
                return (obj(json!({"ok": false, "ma_loi": "khong_du", "ly_do": format!("không đủ số dư ({t2} còn {})", tien(a)), "so_du": a})), None);
            }
            (
                obj(json!({"ok": true, "tu": t2, "den": d2, "so_tien": x, "so_du": a - x, "nguoi": nguoi2, "noi_dung": noi_dung})),
                Some(vec![(t2.clone(), a - x), (d2.clone(), b + x)]),
            )
        });
        let (m1, m2, nguoi) = (Arc::clone(mc), Arc::clone(mc), nguoi.to_string());
        let (tu2, den2) = (tu.clone(), den.clone());
        let tra: TraLoi = Box::new(move |kq: Obj| {
            let kq = if lay_str(&kq, "ma_loi") == Some("khoa_cho_lau") { Self::bao_khoa_chet(&m1, &nguoi, &tu2, &den2, kq) } else { kq };
            tra_loi(kq);
        });
        // thứ tự [nguồn, đích]: chế độ khoa sắp xếp lại theo mã, khóa ngây thơ thì giữ nguyên
        mc.kho.thuc_hien(vec![tu, den], xu_ly, tra, Some(Box::new(move |kq: &mut Obj| da_chuyen(&m2, kq))));
    }

    // ------------------------------------------------------------ trạng thái gửi client
    fn trang_thai(mc: &MayChu<Self>) -> Obj {
        let snap = mc.kho.anh_chup();
        let tk: Vec<Value> = Self::ds_tk(mc).iter().enumerate().map(|(i, m)| json!({"ma": m, "chu": CHU_TK[i], "so_du": snap.get(m).copied().unwrap_or(0)})).collect();
        let tong: i64 = tk.iter().map(|t| t["so_du"].as_i64().unwrap_or(0)).sum();
        let ls: Vec<Obj> = mc.nv.lich_su.lock().unwrap().iter().take(20).cloned().collect();
        obj(json!({"tai_khoan": tk, "tong": tong, "tong_dau": mc.ts_i64("so_tk") * mc.ts_i64("so_du_dau"),
                   "khoa_ngay_tho": mc.kho.khoa_ngay_tho.load(Ordering::SeqCst), "khoa_chet": mc.nv.kc.lock().unwrap().so,
                   "so_gd": mc.nv.so_gd.load(Ordering::SeqCst), "lich_su": ls}))
    }

    fn tom_tat(mc: &MayChu<Self>) -> String {
        let snap = mc.kho.anh_chup();
        let ds = Self::ds_tk(mc);
        let tong: i64 = ds.iter().map(|m| snap.get(m).copied().unwrap_or(0)).sum();
        let lech = tong - mc.ts_i64("so_tk") * mc.ts_i64("so_du_dau");
        if lech != 0 {
            format!("Tổng tiền {}, LỆCH {}", tien(tong), tien_dau(lech))
        } else {
            format!("Tổng tiền {} (khớp), {} tài khoản", tien(tong), ds.len())
        }
    }
}

/// Gọi trong vùng găng ngay sau khi ghi hai số dư: ghi sổ kiểm toán đúng thứ tự ghi.
fn da_chuyen(mc: &MayChu<NganHang>, kq: &mut Obj) {
    let ma = mc.nv.ma_gd.fetch_add(1, Ordering::SeqCst);
    kq.insert("ma_gd".into(), json!(ma));
    mc.nv.so_gd.fetch_add(1, Ordering::SeqCst);
    let s = |k: &str| lay_str(kq, k).unwrap_or("").to_string();
    let x = lay_i64(kq, "so_tien").unwrap_or(0);
    mc.so().ghi(GdChuyen { nguoi: s("nguoi"), tu: s("tu"), den: s("den"), so_tien: x });
    {
        let mut ls = mc.nv.lich_su.lock().unwrap();
        ls.push_front(obj(json!({"ma": ma, "gio": gio_hien(), "nguoi": s("nguoi"), "tu": s("tu"), "den": s("den"), "so_tien": x, "noi_dung": s("noi_dung")})));
        ls.truncate(50);
    }
    mc.bao_doi();
}

/// Số dư (i64) đọc từ Doc theo mã – cho hàm hiển thị.
pub fn so_du_cua(tt: &Obj, ma: &str) -> i64 {
    tt.get("tai_khoan").and_then(Value::as_array).into_iter().flatten().find(|t| t.get("ma").and_then(Value::as_str) == Some(ma)).and_then(|t| t.get("so_du")).and_then(Value::as_i64).unwrap_or(0)
}

