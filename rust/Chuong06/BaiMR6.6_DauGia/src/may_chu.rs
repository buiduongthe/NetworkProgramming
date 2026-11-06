//! Bài mở rộng MR6.6 – Lõi máy chủ đấu giá: nghiệp vụ + sổ kiểm toán (khung ở chung.rs).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
//! Một món hàng, n người cùng trả giá. Thao tác ghi duy nhất:
//!     {"loai":"tra_gia","ma_yc":k,"gia":5300000[,"nguoi":"u17"]}
//! Luật: giá ≥ giá hiện tại + bước giá; phiên còn hạn. Trả giá khi phiên còn < gia_han_s giây
//! (mặc định 10 s) thì phiên được gia hạn tới "bây giờ + gia_han_s" ("đóng mềm" – soft close, chống
//! "bắn tỉa" phút cuối – anti-sniping). Hết giờ: máy chủ đóng phiên, công bố người thắng.
//!
//! Trạng thái là MỘT ô "mon" của KhoTranhChap: Mon {gia, nguoi, so_lan, het_han, ket_thuc}; mọi thay đổi
//! tạo giá trị mới (xu_ly thuần) nên cả 4 chế độ dùng chung đúng một hàm nghiệp vụ.
//!
//! Sổ kiểm toán đếm hai sai phạm:
//!   gia_thap_hon – một giá được chấp nhận mà thấp hơn (giá cao nhất đã chấp nhận trước + bước giá):
//!                  hai người cùng đọc giá cũ, cùng "thắng" kiểm tra, người ghi sau đè người ghi trước;
//!   gia_cuoi_sai – giá/người dẫn đầu cuối cùng khác giá cao nhất đã chấp nhận (giá cao bị ghi đè mất).

use crate::chung::*;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const CONG: u16 = 44700; // cổng mặc định (dòng lệnh)
pub const CONG_GUI: u16 = 44702; // cổng mặc định (giao diện)
/// Kiểu nghiệp vụ của bài – server.rs, giao diện dùng tên chung này.
pub type NV = DauGia;

/// Tùy chọn dòng lệnh của máy chủ: [--thoi-gian 0] (0 = không giới hạn) [--gia 5000000] [--buoc 100000]
/// [--gia-han 10]. Tham số đặt lại: ten_mon, gia_khoi_diem, buoc_gia, thoi_gian_s, gia_han_s.
pub fn tham_so_dong_lenh(a: &Args) -> Obj {
    obj(json!({"thoi_gian_s": a.f64("thoi-gian", 0.0), "gia_khoi_diem": a.i64("gia", 5_000_000),
               "buoc_gia": a.i64("buoc", 100_000), "gia_han_s": a.f64("gia-han", 10.0)}))
}

/// Ô "mon": trạng thái phiên đấu giá.
#[derive(Clone, Debug)]
pub struct Mon {
    pub gia: i64,
    pub nguoi: Option<String>,
    pub so_lan: i64,
    pub het_han: Option<f64>, // đồng hồ đơn điệu (bay_gio); None = không giới hạn
    pub ket_thuc: bool,
}

// ================================================================ sổ kiểm toán
pub struct GdDauGia {
    pub nguoi: String,
    pub gia: i64,
}

pub struct LuatDauGia {
    buoc: i64,
    cao_nhat: Option<i64>, // giá cao nhất đã chấp nhận (theo sổ)
    nguoi_cao_nhat: Option<String>,
}

impl LuatKiemToan for LuatDauGia {
    type GiaoDich = GdDauGia;
    type GiaTri = Mon;
    const TEN_SAI_PHAM: MaTen = &[("gia_thap_hon", "giá thấp hơn"), ("gia_cuoi_sai", "giá cuối sai")];

    fn kiem_tra(&mut self, gd: &GdDauGia, sp: &mut SaiPham) {
        if let Some(c) = self.cao_nhat {
            if gd.gia < c + self.buoc {
                sp.bao("gia_thap_hon", format!("{} {} sau {}", gd.nguoi, tien(gd.gia), tien(c)));
            }
        }
        if self.cao_nhat.is_none_or(|c| gd.gia > c) {
            self.cao_nhat = Some(gd.gia);
            self.nguoi_cao_nhat = Some(gd.nguoi.clone());
        }
    }

    fn kiem_tra_cuoi(&self, doc: &Doc<Mon>) -> Vec<(&'static str, u64)> {
        let (Some(m), Some(c)) = (doc.get("mon"), self.cao_nhat) else { return Vec::new() };
        if m.gia != c || m.nguoi != self.nguoi_cao_nhat {
            vec![("gia_cuoi_sai", 1)]
        } else {
            Vec::new()
        }
    }
}

// ================================================================ nghiệp vụ
pub struct DauGia {
    lich_su: Mutex<VecDeque<Obj>>, // giá đã chấp nhận, mới nhất trước (để hiển thị)
    dang_dong: AtomicBool,
    luc_bao_gia_han: Mutex<f64>,
}

impl NghiepVu for DauGia {
    type GiaTri = Mon;
    type Luat = LuatDauGia;
    const TEN: &'static str = "Đấu giá MR6.6";
    const LOAI_YEU_CAU: &'static [&'static str] = &["tra_gia"];

    fn tham_so_mac_dinh() -> Obj {
        // thoi_gian_s = 0: không giới hạn
        obj(json!({"ten_mon": "Bình gốm hoa lam", "gia_khoi_diem": 5_000_000, "buoc_gia": 100_000,
                   "thoi_gian_s": 0.0, "gia_han_s": 10.0}))
    }

    fn moi() -> DauGia {
        DauGia { lich_su: Mutex::new(VecDeque::new()), dang_dong: AtomicBool::new(false), luc_bao_gia_han: Mutex::new(f64::NEG_INFINITY) }
    }

    // ------------------------------------------------------------ tạo / đặt lại phiên
    fn kiem_tham_so(ts: &Obj) -> Result<(), String> {
        if lay_f64(ts, "buoc_gia").unwrap_or(1.0) <= 0.0 || lay_f64(ts, "gia_khoi_diem").unwrap_or(0.0) < 0.0 {
            return Err("bước giá > 0, giá khởi điểm ≥ 0".into());
        }
        if lay_f64(ts, "thoi_gian_s").unwrap_or(0.0) < 0.0 || lay_f64(ts, "gia_han_s").unwrap_or(0.0) < 0.0 {
            return Err("thời gian ≥ 0".into());
        }
        if let Some(t) = ts.get("ten_mon") {
            if !(1..=40).contains(&py_str(t).chars().count()) {
                return Err("tên món 1–40 ký tự".into());
            }
        }
        Ok(())
    }

    fn khoi_tao(mc: &MayChu<Self>) -> (Vec<(String, Mon)>, LuatDauGia) {
        let thoi_gian = mc.ts_f64("thoi_gian_s");
        let het_han = if thoi_gian > 0.0 { Some(bay_gio() + thoi_gian) } else { None };
        mc.nv.lich_su.lock().unwrap().clear();
        mc.nv.dang_dong.store(false, Ordering::SeqCst);
        *mc.nv.luc_bao_gia_han.lock().unwrap() = f64::NEG_INFINITY;
        let mon = Mon { gia: mc.ts_i64("gia_khoi_diem"), nguoi: None, so_lan: 0, het_han, ket_thuc: false };
        (vec![("mon".into(), mon)], LuatDauGia { buoc: mc.ts_i64("buoc_gia"), cao_nhat: None, nguoi_cao_nhat: None })
    }

    // ------------------------------------------------------------ trả giá
    fn xu_ly_yeu_cau(mc: &Arc<MayChu<Self>>, _khach: &KhachKetNoi, nguoi: &str, tin: &Obj, tra_loi: TraLoi) {
        let Some(gia) = lay_i64(tin, "gia").filter(|&g| g > 0) else {
            tra_loi(obj(json!({"ok": false, "ma_loi": "sai_du_lieu", "ly_do": "giá phải là số nguyên dương"})));
            return;
        };
        let (buoc, gia_han_s, nguoi) = (mc.ts_i64("buoc_gia"), mc.ts_f64("gia_han_s"), nguoi.to_string());
        let xu_ly: XuLy<Mon> = Box::new(move |doc| {
            // hàm THUẦN: đọc doc, trả về (kq, ghi)
            let m = &doc["mon"];
            let bay = bay_gio();
            if m.ket_thuc || m.het_han.is_some_and(|h| bay >= h) {
                return (obj(json!({"ok": false, "ma_loi": "het_phien", "ly_do": "phiên đã kết thúc", "gia": m.gia, "nguoi_dan_dau": m.nguoi})), None);
            }
            if gia < m.gia + buoc {
                return (obj(json!({"ok": false, "ma_loi": "gia_thap", "ly_do": format!("giá phải ≥ {}", tien(m.gia + buoc)), "gia": m.gia, "nguoi_dan_dau": m.nguoi})), None);
            }
            let mut moi = Mon { gia, nguoi: Some(nguoi.clone()), so_lan: m.so_lan + 1, ..m.clone() };
            let gia_han = m.het_han.is_some_and(|h| h - bay < gia_han_s);
            if gia_han {
                moi.het_han = Some(bay + gia_han_s);
            }
            (obj(json!({"ok": true, "gia": gia, "nguoi_dan_dau": nguoi, "gia_han": gia_han})), Some(vec![("mon".into(), moi)]))
        });
        let m2 = Arc::clone(mc);
        mc.kho.thuc_hien(vec!["mon".into()], xu_ly, tra_loi, Some(Box::new(move |kq: &mut Obj| da_chap_nhan(&m2, kq))));
    }

    // ------------------------------------------------------------ đồng hồ phiên (luồng nền, 100 ms)
    fn dinh_ky(mc: &Arc<MayChu<Self>>, bay: f64) {
        let m = mc.kho.doc("mon");
        if m.ket_thuc || m.het_han.is_none_or(|h| bay < h) || mc.nv.dang_dong.load(Ordering::SeqCst) {
            return;
        }
        mc.nv.dang_dong.store(true, Ordering::SeqCst);
        let xu_ly: XuLy<Mon> = Box::new(|doc| {
            // đóng phiên cũng là một thao tác ghi
            let m = &doc["mon"];
            if m.ket_thuc || m.het_han.is_none_or(|h| bay_gio() < h) {
                return (obj(json!({"ok": false})), None); // vừa được gia hạn hoặc đã đóng
            }
            (obj(json!({"ok": true, "nguoi": m.nguoi, "gia": m.gia})), Some(vec![("mon".into(), Mon { ket_thuc: true, ..m.clone() })]))
        });
        let (m1, m2) = (Arc::clone(mc), Arc::clone(mc));
        mc.kho.thuc_hien(
            vec!["mon".into()],
            xu_ly,
            Box::new(move |_| m1.nv.dang_dong.store(false, Ordering::SeqCst)),
            Some(Box::new(move |kq: &mut Obj| {
                match lay_str(kq, "nguoi") {
                    Some(ai) => m2.thong_bao(&format!("Kết thúc phiên: {ai} thắng với {}", tien(lay_i64(kq, "gia").unwrap_or(0)))),
                    None => m2.thong_bao("Kết thúc phiên: không có ai trả giá"),
                }
                m2.bao_doi();
            })),
        );
    }

    // ------------------------------------------------------------ trạng thái gửi client
    fn trang_thai(mc: &MayChu<Self>) -> Obj {
        let m = mc.kho.doc("mon");
        let con_lai = m.het_han.map(|h| round_py((h - bay_gio()) * 1000.0).max(0));
        let ls: Vec<Obj> = mc.nv.lich_su.lock().unwrap().iter().take(12).cloned().collect();
        obj(json!({"mon": {"ten": mc.ts_str("ten_mon"), "gia_khoi_diem": mc.ts_i64("gia_khoi_diem"), "buoc_gia": mc.ts_i64("buoc_gia"),
                           "gia_han_s": mc.ts_f64("gia_han_s")},
                   "gia": m.gia, "nguoi_dan_dau": m.nguoi, "so_lan": m.so_lan,
                   "con_lai_ms": con_lai, "ket_thuc": m.ket_thuc, "lich_su": ls}))
    }

    fn tom_tat(mc: &MayChu<Self>) -> String {
        let m = mc.kho.doc("mon");
        format!("Giá cuối {}, {} {}", tien(m.gia), if m.ket_thuc { "thắng" } else { "dẫn đầu" }, m.nguoi.as_deref().unwrap_or("–"))
    }
}

/// Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.
fn da_chap_nhan(mc: &MayChu<DauGia>, kq: &mut Obj) {
    let nguoi = lay_str(kq, "nguoi_dan_dau").unwrap_or("").to_string();
    let gia = lay_i64(kq, "gia").unwrap_or(0);
    let gia_han = dung(kq, "gia_han");
    mc.so().ghi(GdDauGia { nguoi: nguoi.clone(), gia });
    {
        let mut ls = mc.nv.lich_su.lock().unwrap();
        ls.push_front(obj(json!({"nguoi": nguoi, "gia": gia, "gio": gio_hien(), "gia_han": gia_han})));
        ls.truncate(50);
    }
    if gia_han {
        let bay = bay_gio();
        let mut luc = mc.nv.luc_bao_gia_han.lock().unwrap();
        if bay - *luc > 1.0 {
            *luc = bay;
            drop(luc);
            mc.thong_bao(&format!("Gia hạn: {nguoi} trả giá phút cuối, còn {} s", fmt_g(mc.ts_f64("gia_han_s"))));
        }
    }
    mc.bao_doi();
}

/// Giá trị JSON -> i64 (0 nếu thiếu) – tiện cho các tệp giao diện/dòng lệnh của bài.
pub fn so_nguyen(v: Option<&Value>) -> i64 {
    v.and_then(Value::as_i64).unwrap_or(0)
}
