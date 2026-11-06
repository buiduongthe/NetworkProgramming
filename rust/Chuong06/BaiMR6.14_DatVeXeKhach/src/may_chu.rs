//! Bài mở rộng MR6.14 – Lõi máy chủ đặt vé xe khách theo chặng: nghiệp vụ + sổ kiểm toán (khung ở chung.rs).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối) và MR6.8.
//! Chuyến xe qua các trạm Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang (3 chặng). Hai loại xe:
//!     giuong – xe giường nằm 2 tầng, 40 giường: tầng dưới D01–D20, tầng trên T01–T20 (3 dãy × 7 hàng, hàng cuối 2 giường);
//!     ghe    – xe ghế ngồi 50 chỗ 01–50 (11 hàng × 4 ghế, lối đi ở giữa, hàng cuối 6 ghế liền).
//! Khách đặt một ĐOẠN [tu, den) (chỉ số trạm, tu < den), nên cùng một giường bán được cho nhiều người nếu các
//! đoạn không chồng nhau. Ba thao tác ghi:
//!     {"loai":"giu_cho","ma_yc":k,"ghe":["D01"],"tu":0,"den":2[,"nguoi":"u17"]}   giữ chỗ có thời hạn (TTL)
//!     {"loai":"thanh_toan","ma_yc":k,"ghe":["D01"],"tu":0,"den":2}                 chỗ mình đang giữ -> vé
//!     {"loai":"nha","ma_yc":k,"ghe":["D01"],"tu":0,"den":2}                        trả lại chỗ đang giữ
//! Mỗi cặp (ghế, chặng) là MỘT ô của KhoTranhChap, tên ô "D01#1" (chặng 1 = Bảo Lộc → Đà Lạt):
//!     Cho {tt: trong|giu|ban, nguoi, het_han (đồng hồ đơn điệu), ma_ve, nguoi_het_han}
//! Giữ 2 giường cho đoạn 3 chặng = khóa 6 ô theo thứ tự tên ô (không khóa chết). Luật như MR6.8: giữ được khi
//! MỌI ô đều trống (hoặc lượt giữ cũ đã hết hạn, hoặc đã là của mình); hết hạn sau ttl_s giây – luồng nền dọn ô
//! quá hạn mỗi 100 ms; thanh toán chỉ nhận ô mình giữ, còn hạn.
//!
//! Sổ kiểm toán đếm hai sai phạm:
//!   trung_cho – một (ghế, chặng) được giữ/bán cho người thứ hai khi người khác đang giữ (còn hạn) hoặc đã mua;
//!   tt_sai    – thanh toán được chấp nhận cho chỗ không do mình giữ hoặc lượt giữ đã hết hạn.

use crate::chung::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const CONG: u16 = 44780; // cổng mặc định (dòng lệnh)
pub const CONG_GUI: u16 = 44782; // cổng mặc định (giao diện)
/// Kiểu nghiệp vụ của bài – server.rs, giao diện dùng tên chung này.
pub type NV = DatVeXe;

/// Tùy chọn dòng lệnh của máy chủ: [--ttl 30] [--loai-xe giuong] [--ghe-nong D01] ('' = ngẫu nhiên; mặc định chỗ
/// đầu của loại xe). Tham số đặt lại: ttl_s – thời hạn giữ chỗ, loai_xe – giuong | ghe, ghe_nong – chỗ mọi người
/// ảo nhắm tới đầu tiên (rỗng = chọn ngẫu nhiên), chuyen – giờ chạy, nha_xe.
pub fn tham_so_dong_lenh(a: &Args) -> Obj {
    let mut ts = obj(json!({"ttl_s": a.f64("ttl", 30.0), "loai_xe": a.str("loai-xe", "giuong")}));
    let nong = a.str("ghe-nong", "\u{0}");
    if nong != "\u{0}" {
        ts.insert("ghe_nong".into(), json!(nong.to_uppercase()));
    }
    ts
}

pub const TRAM: [&str; 4] = ["Sài Gòn", "Bảo Lộc", "Đà Lạt", "Nha Trang"];
pub const MA_TRAM: [&str; 4] = ["SG", "BL", "DL", "NT"];
pub const SO_CHANG: usize = TRAM.len() - 1;
pub const TOI_DA_GHE: usize = 6; // một yêu cầu tối đa 6 chỗ

/// Giá minh họa mỗi chặng (đồng/chỗ).
pub fn gia_chang(loai_xe: &str) -> [i64; 3] {
    if loai_xe == "giuong" {
        [220_000, 130_000, 170_000]
    } else {
        [170_000, 100_000, 130_000]
    }
}

pub fn ten_xe(loai_xe: &str) -> &'static str {
    if loai_xe == "giuong" {
        "Xe giường nằm 40 chỗ"
    } else {
        "Xe ghế ngồi 50 chỗ"
    }
}

/// Một chỗ trên sơ đồ xe.
#[derive(Clone, Debug)]
pub struct ChoXe {
    pub ma: String,
    pub tang: usize,
    pub hang: usize,
    pub cot: usize,
}

/// Sơ đồ xe theo thứ tự hiển thị. giuong: tầng 0 = dưới (D), 1 = trên (T); cột 0, 1, 2; hàng 6 chỉ có cột 0 và 2.
/// ghe: tầng 0; 6 cột: hàng 0–10 dùng cột 0, 1 | lối đi | 4, 5; hàng cuối (11) đủ 6 cột.
pub fn bo_tri(loai_xe: &str) -> Vec<ChoXe> {
    let mut ds = Vec::new();
    if loai_xe == "giuong" {
        for (tang, chu) in ["D", "T"].iter().enumerate() {
            let mut n = 0;
            for hang in 0..7 {
                let cot: &[usize] = if hang < 6 { &[0, 1, 2] } else { &[0, 2] };
                for &c in cot {
                    n += 1;
                    ds.push(ChoXe { ma: format!("{chu}{n:02}"), tang, hang, cot: c });
                }
            }
        }
    } else {
        let mut n = 0;
        for hang in 0..12 {
            let cot: &[usize] = if hang < 11 { &[0, 1, 4, 5] } else { &[0, 1, 2, 3, 4, 5] };
            for &c in cot {
                n += 1;
                ds.push(ChoXe { ma: format!("{n:02}"), tang: 0, hang, cot: c });
            }
        }
    }
    ds
}

pub fn ds_ma_ghe(loai_xe: &str) -> Vec<String> {
    bo_tri(loai_xe).into_iter().map(|x| x.ma).collect()
}

pub fn ten_chang(k: usize) -> String {
    format!("{}–{}", TRAM[k], TRAM[k + 1])
}

pub fn ten_doan(tu: usize, den: usize) -> String {
    format!("{} → {}", TRAM[tu], TRAM[den])
}

fn ten_o(o: &str) -> String {
    let (g, k) = o.split_once('#').unwrap_or((o, "0"));
    format!("{g} ({})", ten_chang(k.parse().unwrap_or(0)))
}

pub fn o_cua(ghe: &[String], tu: usize, den: usize) -> Vec<String> {
    ghe.iter().flat_map(|g| (tu..den).map(move |k| format!("{g}#{k}"))).collect()
}

/// Ô (chỗ, chặng).
#[derive(Clone, Debug, PartialEq)]
pub struct Cho {
    pub tt: &'static str, // "trong" | "giu" | "ban"
    pub nguoi: Option<String>,
    pub het_han: Option<f64>,
    pub ma_ve: Option<String>,
    pub nguoi_het_han: Option<String>, // người vừa để lượt giữ hết hạn (để báo "đã hết hạn")
}

fn trong(lan_cuoi: Option<String>) -> Cho {
    Cho { tt: "trong", nguoi: None, het_han: None, ma_ve: None, nguoi_het_han: lan_cuoi }
}

impl Cho {
    fn giu_con_han(&self, luc: f64) -> bool {
        self.tt == "giu" && self.het_han.is_some_and(|h| luc < h)
    }
}

// Loại xe hiện tại của máy chủ – để kiem_tham_so (hàm không có trạng thái của khung) kiểm ghế nóng theo đúng loại
// xe khi lần đặt lại không đổi loại xe. Mỗi tiến trình chỉ chạy một máy chủ đặt vé xe.
static LOAI_HIEN_TAI: Mutex<String> = Mutex::new(String::new());

// ================================================================ sổ kiểm toán
pub struct GdDatVeXe {
    pub loai: &'static str, // giu | ban | nha | het_han
    pub o: Vec<String>,
    pub nguoi: String,
    pub luc: f64,
    pub het_han: Option<f64>,
    pub so_ve: u64,
}

struct OSo {
    tt: &'static str,
    nguoi: String,
    het_han: Option<f64>,
}

pub struct LuatDatVeXe {
    o: HashMap<String, OSo>, // theo sổ: ô -> {tt: giu|ban, nguoi, het_han}
    pub so_ve: u64,          // số vé (ghế × đoạn) đã bán theo sổ
}

impl LuatKiemToan for LuatDatVeXe {
    type GiaoDich = GdDatVeXe;
    type GiaTri = Cho;
    const TEN_SAI_PHAM: MaTen = &[("trung_cho", "trùng chỗ"), ("tt_sai", "thanh toán sai")];

    fn kiem_tra(&mut self, gd: &GdDatVeXe, sp: &mut SaiPham) {
        let (nguoi, luc) = (&gd.nguoi, gd.luc);
        if gd.loai == "ban" {
            self.so_ve += gd.so_ve;
        }
        for o in &gd.o {
            let cu = self.o.get(o);
            match gd.loai {
                "giu" => {
                    if let Some(c) = cu.filter(|c| c.tt == "ban") {
                        sp.bao("trung_cho", format!("{} giữ cho {nguoi} khi đã bán cho {}", ten_o(o), c.nguoi));
                        continue;
                    }
                    if let Some(c) = cu.filter(|c| c.nguoi != *nguoi && c.het_han.is_some_and(|h| luc < h)) {
                        sp.bao("trung_cho", format!("{} giữ cho {nguoi} khi {} đang giữ", ten_o(o), c.nguoi));
                    }
                    self.o.insert(o.clone(), OSo { tt: "giu", nguoi: nguoi.clone(), het_han: gd.het_han });
                }
                "ban" => {
                    if let Some(c) = cu.filter(|c| c.tt == "ban") {
                        sp.bao("trung_cho", format!("{} bán cho {nguoi} khi đã bán cho {}", ten_o(o), c.nguoi));
                        continue;
                    }
                    match cu {
                        Some(c) if c.nguoi == *nguoi => {
                            if c.het_han.is_some_and(|h| luc >= h) {
                                sp.bao("tt_sai", format!("{}: {nguoi} trả tiền khi đã hết hạn giữ", ten_o(o)));
                            }
                        }
                        _ => sp.bao("tt_sai", format!("{}: {nguoi} trả tiền chỗ không do mình giữ", ten_o(o))),
                    }
                    self.o.insert(o.clone(), OSo { tt: "ban", nguoi: nguoi.clone(), het_han: None });
                }
                _ => {
                    // nhả / hết hạn
                    if cu.is_some_and(|c| c.tt == "giu" && c.nguoi == *nguoi) {
                        self.o.remove(o);
                    }
                }
            }
        }
    }
}

// ================================================================ nghiệp vụ
pub struct DatVeXe {
    dang_don: Mutex<HashSet<String>>,   // ô quá hạn đang được luồng nền dọn
    nguoi_that: Mutex<HashSet<String>>, // tên người dùng thật (in nhật ký cho họ)
    bo_tri: Mutex<Vec<ChoXe>>,          // sơ đồ xe hiện tại (khoi_tao đặt)
    dem_ghi: AtomicU64,                 // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
    phien: AtomicU64,
}

fn tu_choi(ma_loi: &str, ly_do: String) -> Obj {
    obj(json!({"ok": false, "ma_loi": ma_loi, "ly_do": ly_do}))
}

fn noi(ds: &[String]) -> String {
    ds.join(", ")
}

impl DatVeXe {
    pub fn ds_ghe(&self) -> Vec<String> {
        self.bo_tri.lock().unwrap().iter().map(|x| x.ma.clone()).collect()
    }
}

/// Ghế nóng đang dùng: tham số ghe_nong nếu là chỗ của loại xe hiện tại, không thì chỗ đầu (D01 / 01) – đổi loại
/// xe mà không đặt ghe_nong thì ghế nóng tự về chỗ đầu; rỗng = không có ghế nóng.
pub fn ghe_nong(mc: &MayChu<DatVeXe>) -> String {
    let g = mc.ts_str("ghe_nong");
    let ds = mc.nv.ds_ghe();
    if g.is_empty() || ds.contains(&g) {
        g
    } else {
        ds.first().cloned().unwrap_or_default()
    }
}

impl NghiepVu for DatVeXe {
    type GiaTri = Cho;
    type Luat = LuatDatVeXe;
    const TEN: &'static str = "Đặt vé xe khách MR6.14";
    const LOAI_YEU_CAU: &'static [&'static str] = &["giu_cho", "thanh_toan", "nha"];

    fn tham_so_mac_dinh() -> Obj {
        obj(json!({"ttl_s": 30.0, "loai_xe": "giuong", "ghe_nong": "D01", "chuyen": "07:30", "nha_xe": "Xe khách Cao Nguyên"}))
    }

    fn moi() -> DatVeXe {
        DatVeXe {
            dang_don: Mutex::new(HashSet::new()),
            nguoi_that: Mutex::new(HashSet::new()),
            bo_tri: Mutex::new(Vec::new()),
            dem_ghi: AtomicU64::new(0),
            phien: AtomicU64::new(0),
        }
    }

    // ------------------------------------------------------------ tạo / đặt lại
    fn kiem_tham_so(ts: &Obj) -> Result<(), String> {
        if !(1.0..=600.0).contains(&lay_f64(ts, "ttl_s").unwrap_or(30.0)) {
            return Err("thời hạn giữ chỗ 1–600 s".into());
        }
        let hien_tai = LOAI_HIEN_TAI.lock().unwrap().clone();
        let loai = ts.get("loai_xe").map(py_str).unwrap_or(if hien_tai.is_empty() { "giuong".into() } else { hien_tai });
        if loai != "giuong" && loai != "ghe" {
            return Err("loại xe: giuong hoặc ghe".into());
        }
        let nong = ts.get("ghe_nong").map(py_str).unwrap_or_default();
        if !nong.is_empty() && !ds_ma_ghe(&loai).contains(&nong) {
            return Err(format!("ghế nóng phải là mã chỗ của {} hoặc rỗng", ten_xe(&loai).to_lowercase()));
        }
        if let Some(p) = ts.get("nha_xe") {
            if !(1..=40).contains(&py_str(p).chars().count()) {
                return Err("tên nhà xe 1–40 ký tự".into());
            }
        }
        Ok(())
    }

    fn khoi_tao(mc: &MayChu<Self>) -> (Vec<(String, Cho)>, LuatDatVeXe) {
        mc.nv.dang_don.lock().unwrap().clear();
        mc.nv.nguoi_that.lock().unwrap().clear();
        let loai = mc.ts_str("loai_xe");
        *LOAI_HIEN_TAI.lock().unwrap() = loai.clone();
        let bt = bo_tri(&loai);
        let o: Vec<(String, Cho)> = bt.iter().flat_map(|x| (0..SO_CHANG).map(move |k| (format!("{}#{k}", x.ma), trong(None)))).collect();
        *mc.nv.bo_tri.lock().unwrap() = bt;
        (o, LuatDatVeXe { o: HashMap::new(), so_ve: 0 })
    }

    // ------------------------------------------------------------ giữ chỗ / thanh toán / nhả
    fn xu_ly_yeu_cau(mc: &Arc<MayChu<Self>>, khach: &KhachKetNoi, nguoi: &str, tin: &Obj, tra_loi: TraLoi) {
        let loai_xe = mc.ts_str("loai_xe");
        let hop_le: HashSet<String> = mc.nv.ds_ghe().into_iter().collect();
        let ghe: Option<Vec<String>> = tin.get("ghe").and_then(Value::as_array).and_then(|a| {
            if !(1..=TOI_DA_GHE).contains(&a.len()) {
                return None;
            }
            let ds: Vec<String> = a.iter().map(|g| g.as_str().map(str::to_string)).collect::<Option<_>>()?;
            let khac: HashSet<&String> = ds.iter().collect();
            (khac.len() == ds.len() && ds.iter().all(|g| hop_le.contains(g))).then_some(ds)
        });
        let Some(ghe) = ghe else {
            tra_loi(tu_choi("sai_du_lieu", format!("ghế: danh sách 1–{TOI_DA_GHE} mã chỗ của {}", ten_xe(&loai_xe).to_lowercase())));
            return;
        };
        let (tu, den) = match (lay_i64(tin, "tu"), lay_i64(tin, "den")) {
            (Some(a), Some(b)) if 0 <= a && a < b && b <= SO_CHANG as i64 => (a as usize, b as usize),
            _ => {
                tra_loi(tu_choi("sai_du_lieu", format!("đoạn đường: 0 ≤ tu < den ≤ {SO_CHANG} (chỉ số trạm)")));
                return;
            }
        };
        let o_yc = o_cua(&ghe, tu, den);
        let that = khach.so_nguoi.is_none();
        if that {
            mc.nv.nguoi_that.lock().unwrap().insert(nguoi.to_string());
        }
        let (ttl, chuyen) = (mc.ts_f64("ttl_s"), mc.ts_str("chuyen"));
        let gia: i64 = gia_chang(&loai_xe)[tu..den].iter().sum::<i64>() * ghe.len() as i64;
        let tra: TraLoi = Box::new(move |mut kq: Obj| {
            // bỏ các trường nội bộ ("_…") trước khi gửi
            kq.retain(|k, _| !k.starts_with('_'));
            tra_loi(kq)
        });
        let (nguoi, ds, oo, m2) = (nguoi.to_string(), ghe.clone(), o_yc.clone(), Arc::clone(mc));
        match lay_str(tin, "loai") {
            Some("giu_cho") => {
                let xu_ly: XuLy<Cho> = Box::new(move |doc| {
                    // hàm THUẦN: mọi ô phải trống / quá hạn / của mình
                    let luc = bay_gio();
                    let (mut moi, mut con_lai) = (Vec::new(), ttl);
                    for o in &oo {
                        let v = &doc[o.as_str()];
                        let (g, k) = o.split_once('#').unwrap_or((o, "0"));
                        let k: usize = k.parse().unwrap_or(0);
                        if v.tt == "ban" {
                            return (obj(json!({"ok": false, "ma_loi": "da_ban", "ly_do": format!("chỗ {g} chặng {} đã bán", ten_chang(k)),
                                               "ghe_ban": [g], "tu": tu, "den": den})), None);
                        }
                        if v.giu_con_han(luc) {
                            if v.nguoi.as_deref() != Some(nguoi.as_str()) {
                                return (obj(json!({"ok": false, "ma_loi": "dang_giu", "ly_do": format!("chỗ {g} chặng {} đang có người giữ", ten_chang(k)),
                                                   "ghe_ban": [g], "tu": tu, "den": den})), None);
                            }
                            con_lai = con_lai.min(v.het_han.unwrap_or(luc) - luc);
                        } else {
                            moi.push(o.clone());
                        }
                    }
                    let het_han = luc + ttl;
                    let ghi: Ghi<Cho> = moi.iter().map(|o| (o.clone(), Cho { tt: "giu", nguoi: Some(nguoi.clone()), het_han: Some(het_han), ma_ve: None, nguoi_het_han: None })).collect();
                    let kq = obj(json!({"ok": true, "ghe": ds, "tu": tu, "den": den, "nguoi": nguoi, "het_han_ms": round_py(con_lai * 1000.0), "tien": gia,
                                        "_moi": moi, "_luc": luc, "_het_han": het_han}));
                    (kq, if ghi.is_empty() { None } else { Some(ghi) })
                });
                mc.kho.thuc_hien(o_yc, xu_ly, tra, Some(Box::new(move |kq: &mut Obj| da_ghi(&m2, "giu", kq, that))));
            }
            Some("thanh_toan") => {
                let ma = format!("XK{}", chuyen.replace(':', ""));
                let xu_ly: XuLy<Cho> = Box::new(move |doc| {
                    let luc = bay_gio();
                    let la_toi = |v: &Cho| v.nguoi.as_deref() == Some(nguoi.as_str());
                    let (mut het_han, mut khong_giu): (Option<String>, Option<String>) = (None, None); // ưu tiên báo "hết hạn"
                    for o in &oo {
                        let v = &doc[o.as_str()];
                        let g = o.split_once('#').map(|x| x.0).unwrap_or(o).to_string();
                        if v.giu_con_han(luc) && la_toi(v) {
                            continue;
                        }
                        if (v.tt == "giu" && la_toi(v)) || (v.tt == "trong" && v.nguoi_het_han.as_deref() == Some(nguoi.as_str())) {
                            het_han.get_or_insert(g);
                        } else {
                            khong_giu.get_or_insert(g);
                        }
                    }
                    if let Some(g) = het_han {
                        return (tu_choi("het_han", format!("giữ chỗ {g} đã hết hạn")), None);
                    }
                    if let Some(g) = khong_giu {
                        return (tu_choi("khong_giu", format!("chỗ {g} không do bạn giữ")), None);
                    }
                    let ve: Vec<String> = ds.iter().map(|g| format!("{ma}-{g}-{}{}", MA_TRAM[tu], MA_TRAM[den])).collect();
                    let ghi: Ghi<Cho> = oo
                        .iter()
                        .map(|o| {
                            let g = o.split_once('#').map(|x| x.0).unwrap_or(o);
                            let i = ds.iter().position(|x| x == g).unwrap_or(0);
                            (o.clone(), Cho { tt: "ban", nguoi: Some(nguoi.clone()), het_han: None, ma_ve: Some(ve[i].clone()), nguoi_het_han: None })
                        })
                        .collect();
                    (obj(json!({"ok": true, "ghe": ds, "tu": tu, "den": den, "nguoi": nguoi, "ma_ve": ve, "tien": gia, "_luc": luc})), Some(ghi))
                });
                mc.kho.thuc_hien(o_yc, xu_ly, tra, Some(Box::new(move |kq: &mut Obj| da_ghi(&m2, "ban", kq, that))));
            }
            _ => {
                let xu_ly: XuLy<Cho> = Box::new(move |doc| {
                    let nha: Vec<String> = oo.iter().filter(|o| doc[o.as_str()].tt == "giu" && doc[o.as_str()].nguoi.as_deref() == Some(nguoi.as_str())).cloned().collect();
                    if nha.is_empty() {
                        return (tu_choi("khong_giu", "bạn không giữ chỗ nào trong số này".into()), None);
                    }
                    let ghi: Ghi<Cho> = nha.iter().map(|o| (o.clone(), trong(None))).collect();
                    (obj(json!({"ok": true, "ghe": ds, "tu": tu, "den": den, "nguoi": nguoi, "_o": nha})), Some(ghi))
                });
                mc.kho.thuc_hien(o_yc, xu_ly, tra, Some(Box::new(move |kq: &mut Obj| da_ghi(&m2, "nha", kq, that))));
            }
        }
    }

    // ------------------------------------------------------------ dọn ô quá hạn (luồng nền, 100 ms)
    fn dinh_ky(mc: &Arc<MayChu<Self>>, bay: f64) {
        let tat_ca: Vec<String> = mc.nv.ds_ghe().iter().flat_map(|g| (0..SO_CHANG).map(move |k| format!("{g}#{k}"))).collect();
        for o in tat_ca {
            let v = mc.kho.doc(&o);
            if v.tt != "giu" || v.het_han.is_some_and(|h| bay < h) || mc.nv.dang_don.lock().unwrap().contains(&o) {
                continue;
            }
            mc.nv.dang_don.lock().unwrap().insert(o.clone());
            let o2 = o.clone();
            let xu_ly: XuLy<Cho> = Box::new(move |doc| {
                // nhả ô quá hạn cũng là một thao tác ghi
                let v = &doc[o2.as_str()];
                if v.tt != "giu" || v.het_han.is_some_and(|h| bay_gio() < h) {
                    return (obj(json!({"ok": false})), None); // vừa được thanh toán / giữ lại
                }
                (obj(json!({"ok": true, "o": [o2], "nguoi": v.nguoi})), Some(vec![(o2.clone(), trong(v.nguoi.clone()))]))
            });
            let (m1, m2, o3) = (Arc::clone(mc), Arc::clone(mc), o.clone());
            mc.kho.thuc_hien(
                vec![o],
                xu_ly,
                Box::new(move |_| {
                    m1.nv.dang_don.lock().unwrap().remove(&o3);
                }),
                Some(Box::new(move |kq: &mut Obj| da_ghi(&m2, "het_han", kq, false))),
            );
        }
    }

    // ------------------------------------------------------------ trạng thái gửi client
    /// so_do[i] = chuỗi SO_CHANG ký tự cho chỗ ghe[i]: '.' trống, 'g' đang giữ, 'b' đã bán (từng chặng);
    /// chu / con_lai_ms theo tên ô "D01#1".
    fn trang_thai(mc: &MayChu<Self>) -> Obj {
        let phien = mc.nv.phien.load(Ordering::SeqCst); // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
        let bay = bay_gio();
        let doc = mc.kho.anh_chup();
        let chi_muc: HashMap<&str, &Cho> = doc.ds.iter().map(|(k, v)| (k.as_str(), &**v)).collect();
        let bt = mc.nv.bo_tri.lock().unwrap().clone();
        let (mut so_do, mut chu, mut con_lai) = (Vec::new(), Obj::new(), Obj::new());
        let (mut n_trong, mut n_giu, mut n_ban) = (0, 0, 0);
        for x in &bt {
            let mut dong = String::new();
            for k in 0..SO_CHANG {
                let o = format!("{}#{k}", x.ma);
                let Some(v) = chi_muc.get(o.as_str()) else {
                    dong.push('.');
                    continue;
                };
                if v.giu_con_han(bay) {
                    dong.push('g');
                    chu.insert(o.clone(), json!(v.nguoi));
                    con_lai.insert(o, json!(round_py((v.het_han.unwrap_or(bay) - bay) * 1000.0)));
                    n_giu += 1;
                } else if v.tt == "ban" {
                    dong.push('b');
                    chu.insert(o, json!(v.nguoi));
                    n_ban += 1;
                } else {
                    dong.push('.');
                    n_trong += 1;
                }
            }
            so_do.push(dong);
        }
        let loai = mc.ts_str("loai_xe");
        let ghe: Vec<&str> = bt.iter().map(|x| x.ma.as_str()).collect();
        let bo_tri_j: Vec<[usize; 3]> = bt.iter().map(|x| [x.tang, x.hang, x.cot]).collect();
        obj(json!({"phien": phien, "nha_xe": mc.ts_str("nha_xe"), "chuyen": mc.ts_str("chuyen"), "loai_xe": loai, "ten_xe": ten_xe(&loai),
                   "tram": TRAM, "gia_chang": gia_chang(&loai), "ttl_s": mc.ts_f64("ttl_s"), "ghe_nong": ghe_nong(mc), "ghe": ghe,
                   "bo_tri": bo_tri_j, "so_do": so_do, "chu": chu, "con_lai_ms": con_lai, "dem": {"trong": n_trong, "giu": n_giu, "ban": n_ban}}))
    }

    fn tom_tat(mc: &MayChu<Self>) -> String {
        let bay = bay_gio();
        let doc = mc.kho.anh_chup();
        let ban = doc.values().filter(|v| v.tt == "ban").count();
        let giu = doc.values().filter(|v| v.giu_con_han(bay)).count();
        format!("Bán {ban}/{} chỗ-chặng, giữ {giu}, sổ {} vé", doc.ds.len(), mc.so().xem(|l| l.so_ve))
    }
}

/// khi_ghi: gọi trong vùng găng ngay sau khi ghi – ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ
/// tự lần ghi; cap_nhat mang "phien" lúc chụp -> client biết cap_nhat chụp trước hay sau lần ghi của mình.
fn da_ghi(mc: &MayChu<DatVeXe>, loai: &'static str, kq: &mut Obj, that: bool) {
    let phien = mc.nv.dem_ghi.fetch_add(1, Ordering::SeqCst) + 1;
    mc.nv.phien.store(phien, Ordering::SeqCst);
    kq.insert("phien".into(), json!(phien));
    let ds = |k: &str| -> Vec<String> { kq.get(k).and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default() };
    let ghe = ds("ghe");
    let (tu, den) = (lay_i64(kq, "tu").unwrap_or(0) as usize, lay_i64(kq, "den").unwrap_or(0) as usize);
    let o = match loai {
        "giu" => ds("_moi"),
        "ban" => o_cua(&ghe, tu, den),
        "nha" => ds("_o"),
        _ => ds("o"),
    };
    let nguoi = lay_str(kq, "nguoi").unwrap_or("").to_string();
    let so_ve = if loai == "ban" { ghe.len() as u64 } else { 0 };
    mc.so().ghi(GdDatVeXe { loai, o: o.clone(), nguoi: nguoi.clone(), luc: lay_f64(kq, "_luc").unwrap_or(0.0), het_han: lay_f64(kq, "_het_han"), so_ve });
    match loai {
        "giu" if that => mc.thong_bao(&format!("{nguoi} giữ {} ({}, {} s)", noi(&ghe), ten_doan(tu, den), fmt_g(mc.ts_f64("ttl_s")))),
        "ban" if that => mc.thong_bao(&format!("{nguoi} mua {} vé {} {} ({})", ghe.len(), noi(&ghe), ten_doan(tu, den), tien(lay_i64(kq, "tien").unwrap_or(0)))),
        "nha" if that => mc.thong_bao(&format!("{nguoi} nhả {} ({})", noi(&ghe), ten_doan(tu, den))),
        "het_han" if mc.nv.nguoi_that.lock().unwrap().contains(&nguoi) => {
            mc.thong_bao(&format!("Hết hạn giữ {} ({nguoi}), chỗ trống lại", o.first().map(|x| ten_o(x)).unwrap_or_default()))
        }
        _ => {}
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
