//! Bài mở rộng MR6.11 – Lõi máy chủ flash sale: nghiệp vụ + sổ kiểm toán (khung ở chung.rs).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
//! "Giờ vàng" flash sale: kho có 100 sản phẩm, mở bán lúc T (đếm ngược), hàng nghìn người cùng bấm
//! "MUA NGAY" đúng lúc mở. Thao tác ghi duy nhất:
//!     {"loai":"mua","ma_yc":k,"so_luong":1|2[,"nguoi":"u17"]}
//! Luật: chưa tới giờ mở bán -> "chua_mo" (kèm con_ms); mỗi đơn 1–toi_da (mặc định 2) sản phẩm; tồn kho
//! phải đủ (còn 1 mà mua 2 -> "khong_du"); hết -> "het_hang".
//!
//! Trạng thái là MỘT ô "kho" của KhoTranhChap: Kho {ton, da_ban, so_don} – điểm nóng mà mọi đơn hàng cùng
//! tranh. Hai việc KHÔNG cần đi qua vùng găng: kiểm tra giờ mở bán (chỉ đọc đồng hồ) và "hết hàng nhanh"
//! (đọc tồn kho không khóa: tồn chỉ giảm, nên đã thấy 0 thì chắc chắn là 0) – giúp hàng nghìn yêu cầu đến
//! sau khi cháy hàng được trả lời ngay mà không phải xếp hàng chờ khóa.
//!
//! Sổ kiểm toán (tổng đã bán theo sổ, ghi trong vùng găng) đếm hai sai phạm:
//!   ton_am  – một đơn được chấp nhận làm tổng đã bán vượt tồn đầu kỳ (tồn kho thật bị âm – bán vượt);
//!   lech_so – cuối buổi: tổng đã bán theo sổ ≠ tồn đầu kỳ − tồn hiện tại (đếm số sản phẩm lệch).

use crate::chung::*;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const CONG: u16 = 44750; // cổng mặc định (dòng lệnh)
pub const CONG_GUI: u16 = 44752; // cổng mặc định (giao diện)
/// Kiểu nghiệp vụ của bài – server.rs, giao diện dùng tên chung này.
pub type NV = FlashSale;

/// Tùy chọn dòng lệnh của máy chủ: [--ton 100] [--mo-sau 30]. Tham số đặt lại: ten_sp, gia_goc, gia_sale,
/// ton_dau, mo_sau_s (mở bán sau bao nhiêu giây), toi_da.
pub fn tham_so_dong_lenh(a: &Args) -> Obj {
    obj(json!({"ton_dau": a.i64("ton", 100), "mo_sau_s": a.f64("mo-sau", 30.0)}))
}

/// Ô "kho".
#[derive(Clone, Debug)]
pub struct Kho {
    pub ton: i64,
    pub da_ban: i64,
    pub so_don: i64,
}

// ================================================================ sổ kiểm toán
#[allow(dead_code)] // ma_don: lưu trong sổ như bản Python (để tra cứu), luật kiểm toán không dùng
pub struct GdMua {
    pub nguoi: String,
    pub so_luong: i64,
    pub ma_don: String,
}

pub struct LuatFlashSale {
    ton_dau: i64,
    pub tong_ban: i64,
    pub so_gd: u64,
}

impl LuatKiemToan for LuatFlashSale {
    type GiaoDich = GdMua;
    type GiaTri = Kho;
    const TEN_SAI_PHAM: MaTen = &[("ton_am", "tồn âm"), ("lech_so", "lệch sổ")];

    fn kiem_tra(&mut self, gd: &GdMua, sp: &mut SaiPham) {
        self.so_gd += 1;
        self.tong_ban += gd.so_luong;
        if self.tong_ban > self.ton_dau {
            sp.bao("ton_am", format!("{} mua {}: bán {}/{}", gd.nguoi, gd.so_luong, self.tong_ban, self.ton_dau));
        }
    }

    fn kiem_tra_cuoi(&self, doc: &Doc<Kho>) -> Vec<(&'static str, u64)> {
        let Some(k) = doc.get("kho") else { return Vec::new() };
        let lech = ((self.ton_dau - k.ton) - self.tong_ban).unsigned_abs();
        if lech > 0 {
            vec![("lech_so", lech)]
        } else {
            Vec::new()
        }
    }
}

// ================================================================ nghiệp vụ
pub struct FlashSale {
    mo_luc: Mutex<f64>,             // thời điểm mở bán (đồng hồ đơn điệu)
    don_gan: Mutex<VecDeque<Obj>>,  // đơn mới nhất trước (để hiển thị), tối đa 30
    da_bao_mo: AtomicBool,
    da_bao_het: AtomicBool,
}

impl FlashSale {
    fn mo_luc(mc: &MayChu<Self>) -> f64 {
        *mc.nv.mo_luc.lock().unwrap()
    }
}

impl NghiepVu for FlashSale {
    type GiaTri = Kho;
    type Luat = LuatFlashSale;
    const TEN: &'static str = "Flash sale MR6.11";
    const LOAI_YEU_CAU: &'static [&'static str] = &["mua"];

    fn tham_so_mac_dinh() -> Obj {
        obj(json!({"ten_sp": "Tai nghe không dây TWS Pro", "gia_goc": 1_990_000, "gia_sale": 199_000,
                   "ton_dau": 100, "mo_sau_s": 30.0, "toi_da": 2}))
    }

    fn moi() -> FlashSale {
        FlashSale { mo_luc: Mutex::new(0.0), don_gan: Mutex::new(VecDeque::new()), da_bao_mo: AtomicBool::new(false), da_bao_het: AtomicBool::new(false) }
    }

    // ------------------------------------------------------------ tạo / đặt lại đợt sale
    fn kiem_tham_so(ts: &Obj) -> Result<(), String> {
        if !(1..=1_000_000).contains(&lay_i64(ts, "ton_dau").unwrap_or(1)) {
            return Err("tồn đầu kỳ 1–1 000 000".into());
        }
        if lay_f64(ts, "mo_sau_s").unwrap_or(0.0) < 0.0 {
            return Err("mo_sau_s ≥ 0".into());
        }
        if !(1..=10).contains(&lay_i64(ts, "toi_da").unwrap_or(1)) {
            return Err("tối đa mỗi đơn 1–10".into());
        }
        if lay_f64(ts, "gia_sale").unwrap_or(1.0) <= 0.0 || lay_f64(ts, "gia_goc").unwrap_or(1.0) <= 0.0 {
            return Err("giá > 0".into());
        }
        Ok(())
    }

    fn khoi_tao(mc: &MayChu<Self>) -> (Vec<(String, Kho)>, LuatFlashSale) {
        let (ton_dau, mo_sau) = (mc.ts_i64("ton_dau"), mc.ts_f64("mo_sau_s"));
        *mc.nv.mo_luc.lock().unwrap() = bay_gio() + mo_sau;
        mc.nv.don_gan.lock().unwrap().clear();
        mc.nv.da_bao_mo.store(mo_sau <= 0.0, Ordering::SeqCst);
        mc.nv.da_bao_het.store(false, Ordering::SeqCst);
        (vec![("kho".into(), Kho { ton: ton_dau, da_ban: 0, so_don: 0 })], LuatFlashSale { ton_dau, tong_ban: 0, so_gd: 0 })
    }

    // ------------------------------------------------------------ mua
    fn xu_ly_yeu_cau(mc: &Arc<MayChu<Self>>, khach: &KhachKetNoi, nguoi: &str, tin: &Obj, tra_loi: TraLoi) {
        let toi_da = mc.ts_i64("toi_da");
        let Some(sl) = lay_i64(tin, "so_luong").filter(|s| (1..=toi_da).contains(s)) else {
            tra_loi(obj(json!({"ok": false, "ma_loi": "sai_du_lieu", "ly_do": format!("số lượng 1–{toi_da}")})));
            return;
        };
        let con = Self::mo_luc(mc) - bay_gio();
        if con > 0.0 {
            // chưa tới giờ: chỉ đọc đồng hồ, không chạm kho
            tra_loi(obj(json!({"ok": false, "ma_loi": "chua_mo", "ly_do": format!("chưa mở bán, còn {con:.1} s"), "con_ms": round_py(con * 1000.0)})));
            return;
        }
        if mc.kho.doc("kho").ton <= 0 {
            // hết hàng nhanh: tồn chỉ giảm, đọc không khóa
            tra_loi(obj(json!({"ok": false, "ma_loi": "het_hang", "ly_do": "hết hàng", "ton": 0})));
            return;
        }
        let (gia_sale, nguoi) = (mc.ts_i64("gia_sale"), nguoi.to_string());
        let xu_ly: XuLy<Kho> = Box::new(move |doc| {
            // hàm THUẦN: đọc doc, trả về (kq, ghi)
            let k = &doc["kho"];
            if k.ton <= 0 {
                return (obj(json!({"ok": false, "ma_loi": "het_hang", "ly_do": "hết hàng", "ton": 0})), None);
            }
            if k.ton < sl {
                return (obj(json!({"ok": false, "ma_loi": "khong_du", "ly_do": format!("chỉ còn {} sản phẩm", k.ton), "ton": k.ton})), None);
            }
            let moi = Kho { ton: k.ton - sl, da_ban: k.da_ban + sl, so_don: k.so_don + 1 };
            (
                obj(json!({"ok": true, "ma_don": format!("FS{:05}", moi.so_don), "so_luong": sl, "ton": moi.ton, "nguoi": nguoi, "thanh_tien": sl * gia_sale})),
                Some(vec![("kho".into(), moi)]),
            )
        });
        let gop = khach.so_nguoi.is_some();
        let m2 = Arc::clone(mc);
        mc.kho.thuc_hien(vec!["kho".into()], xu_ly, tra_loi, Some(Box::new(move |kq: &mut Obj| da_ban(&m2, kq, gop))));
    }

    // ------------------------------------------------------------ đồng hồ mở bán (luồng nền, 100 ms)
    fn dinh_ky(mc: &Arc<MayChu<Self>>, bay: f64) {
        if !mc.nv.da_bao_mo.load(Ordering::SeqCst) && bay >= Self::mo_luc(mc) {
            mc.nv.da_bao_mo.store(true, Ordering::SeqCst);
            mc.thong_bao(&format!("MỞ BÁN! {} sản phẩm", mc.ts_i64("ton_dau")));
            mc.bao_doi();
        }
    }

    // ------------------------------------------------------------ trạng thái gửi client
    fn trang_thai(mc: &MayChu<Self>) -> Obj {
        let k = mc.kho.doc("kho");
        let con = Self::mo_luc(mc) - bay_gio();
        let don: Vec<Obj> = mc.nv.don_gan.lock().unwrap().iter().take(10).cloned().collect();
        obj(json!({"sp": {"ten": mc.ts_str("ten_sp"), "gia_goc": mc.ts_i64("gia_goc"), "gia_sale": mc.ts_i64("gia_sale"), "toi_da": mc.ts_i64("toi_da")},
                   "ton_dau": mc.ts_i64("ton_dau"), "ton": k.ton, "da_ban": k.da_ban, "so_don": k.so_don,
                   "ban_theo_so": mc.so().xem(|l| l.tong_ban), "mo_ban": con <= 0.0, "mo_con_ms": round_py(con * 1000.0).max(0),
                   "don_gan": don}))
    }

    fn tom_tat(mc: &MayChu<Self>) -> String {
        let k = mc.kho.doc("kho");
        let (ban, don) = mc.so().xem(|l| (l.tong_ban, l.so_gd));
        format!("Bán {ban}/{} sp ({don} đơn), tồn {}", mc.ts_i64("ton_dau"), k.ton)
    }
}

/// Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.
fn da_ban(mc: &MayChu<FlashSale>, kq: &mut Obj, gop: bool) {
    let nguoi = lay_str(kq, "nguoi").unwrap_or("").to_string();
    let (sl, ton) = (lay_i64(kq, "so_luong").unwrap_or(0), lay_i64(kq, "ton").unwrap_or(0));
    let ma_don = lay_str(kq, "ma_don").unwrap_or("").to_string();
    mc.so().ghi(GdMua { nguoi: nguoi.clone(), so_luong: sl, ma_don: ma_don.clone() });
    {
        let mut d = mc.nv.don_gan.lock().unwrap();
        d.push_front(obj(json!({"nguoi": nguoi, "so_luong": sl, "gio": gio_hien()})));
        d.truncate(30);
    }
    if !gop {
        mc.thong_bao(&format!("{nguoi} mua {sl} – đơn {ma_don}, còn {ton}"));
    }
    if ton == 0 && !mc.nv.da_bao_het.swap(true, Ordering::SeqCst) {
        mc.thong_bao(&format!("HẾT HÀNG sau {:.2} s mở bán", bay_gio() - FlashSale::mo_luc(mc)));
    }
    mc.bao_doi();
}

// Ba chỗ nối với server.rs (bài này không dùng): cờ dòng lệnh không kèm giá trị, dòng báo sau khi lắng nghe,
// lệnh bàn phím riêng của bài (MR6.12, MR6.13 có dùng).
pub const CO_DONG_LENH: &[&str] = &[];

pub fn sau_khoi_dong(_a: &Args, _log: &dyn Fn(&str)) {}

pub fn lenh_rieng(_sv: &Arc<MayChu<NV>>, _p: &[&str]) -> Option<Result<(), String>> {
    None
}
