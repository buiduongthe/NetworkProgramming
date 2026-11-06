//! Bài mở rộng MR6.8 – Lõi máy chủ đặt vé xem phim: nghiệp vụ + sổ kiểm toán (khung ở chung.rs).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
//! Phòng chiếu 8 hàng (A–H) × 12 ghế; n người cùng chọn ghế. Ba thao tác ghi:
//!     {"loai":"giu_cho","ma_yc":k,"ghe":["E7","E8"][,"nguoi":"u17"]}    giữ chỗ có thời hạn (TTL)
//!     {"loai":"thanh_toan","ma_yc":k,"ghe":["E7","E8"]}                  ghế mình đang giữ, còn hạn -> vé
//!     {"loai":"nha","ma_yc":k,"ghe":["E7"]}                              trả lại ghế đang giữ
//! Luật: giữ được khi MỌI ghế trong yêu cầu đều trống (hoặc lượt giữ cũ đã hết hạn, hoặc đã là của mình);
//! lượt giữ hết hạn sau ttl_s giây (mặc định 30 s) – luồng nền của máy chủ (100 ms/lần) "dọn" ghế quá hạn,
//! và một lượt giữ quá hạn cũng bị coi là trống ngay cả khi chưa kịp dọn; thanh toán chỉ nhận ghế mình
//! đang giữ và còn hạn (quá hạn -> "het_han"). Mỗi ghế là MỘT ô của KhoTranhChap:
//!     Ghe {tt: trong|giu|ban, nguoi, het_han (đồng hồ đơn điệu), ma_ve, nguoi_het_han}
//! nên khóa theo từng ghế: giữ nhiều ghế = khóa nhiều ô theo thứ tự tên ô (không khóa chết).
//!
//! Sổ kiểm toán đếm hai sai phạm:
//!   trung_ghe – một ghế được giữ/bán cho người thứ hai khi đang được người khác giữ (còn hạn) hoặc đã bán;
//!   tt_sai    – thanh toán được chấp nhận cho ghế không do mình giữ hoặc lượt giữ đã hết hạn.

use crate::chung::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const CONG: u16 = 44720; // cổng mặc định (dòng lệnh)
pub const CONG_GUI: u16 = 44722; // cổng mặc định (giao diện)
/// Kiểu nghiệp vụ của bài – server.rs, giao diện dùng tên chung này.
pub type NV = DatVe;

/// Tùy chọn dòng lệnh của máy chủ: [--ttl 30] [--ghe-nong E7] ('' = ngẫu nhiên). Tham số đặt lại: ttl_s –
/// thời hạn giữ chỗ, ghe_nong – ghế mọi người ảo nhắm tới đầu tiên (rỗng = chọn ngẫu nhiên), gia_ve, phim, suat.
pub fn tham_so_dong_lenh(a: &Args) -> Obj {
    obj(json!({"ttl_s": a.f64("ttl", 30.0), "ghe_nong": a.str("ghe-nong", "E7")}))
}

pub const HANG: &str = "ABCDEFGH";
pub const COT: usize = 12;
pub const TOI_DA_GHE: usize = 8; // một yêu cầu tối đa 8 ghế

pub fn tat_ca_ghe() -> Vec<String> {
    HANG.chars().flat_map(|r| (1..=COT).map(move |c| format!("{r}{c}"))).collect()
}

pub fn ghe_hop_le(g: &str) -> bool {
    let mut c = g.chars();
    matches!(c.next(), Some(r) if HANG.contains(r)) && c.as_str().parse::<usize>().is_ok_and(|n| (1..=COT).contains(&n) && !c.as_str().starts_with('0'))
}

/// Kiểm tra danh sách ghế: 1–8 mã ghế hợp lệ, không trùng; trả về Vec hoặc None.
pub fn ds_ghe(x: Option<&Value>) -> Option<Vec<String>> {
    let a = x?.as_array()?;
    if !(1..=TOI_DA_GHE).contains(&a.len()) {
        return None;
    }
    let ds: Vec<String> = a.iter().map(|g| g.as_str().map(str::to_string)).collect::<Option<_>>()?;
    let khac: HashSet<&String> = ds.iter().collect();
    (khac.len() == ds.len() && ds.iter().all(|g| ghe_hop_le(g))).then_some(ds)
}

/// Ô ghế.
#[derive(Clone, Debug, PartialEq)]
pub struct Ghe {
    pub tt: &'static str, // "trong" | "giu" | "ban"
    pub nguoi: Option<String>,
    pub het_han: Option<f64>,
    pub ma_ve: Option<String>,
    pub nguoi_het_han: Option<String>, // người vừa để lượt giữ hết hạn (để báo "đã hết hạn")
}

fn trong(lan_cuoi: Option<String>) -> Ghe {
    Ghe { tt: "trong", nguoi: None, het_han: None, ma_ve: None, nguoi_het_han: lan_cuoi }
}

impl Ghe {
    fn giu_con_han(&self, luc: f64) -> bool {
        self.tt == "giu" && self.het_han.is_some_and(|h| luc < h)
    }
}

// ================================================================ sổ kiểm toán
pub struct GdDatVe {
    pub loai: &'static str, // giu | ban | nha | het_han
    pub ghe: Vec<String>,
    pub nguoi: String,
    pub luc: f64,
    pub het_han: Option<f64>,
}

struct GheSo {
    tt: &'static str,
    nguoi: String,
    het_han: Option<f64>,
}

pub struct LuatDatVe {
    ghe: HashMap<String, GheSo>, // theo sổ: ghế -> {tt: giu|ban, nguoi, het_han}
    pub so_ve: u64,              // số vé đã bán theo sổ
}

impl LuatKiemToan for LuatDatVe {
    type GiaoDich = GdDatVe;
    type GiaTri = Ghe;
    const TEN_SAI_PHAM: MaTen = &[("trung_ghe", "ghế trùng"), ("tt_sai", "thanh toán sai")];

    fn kiem_tra(&mut self, gd: &GdDatVe, sp: &mut SaiPham) {
        let (nguoi, luc) = (&gd.nguoi, gd.luc);
        for g in &gd.ghe {
            let cu = self.ghe.get(g);
            match gd.loai {
                "giu" => {
                    if let Some(c) = cu.filter(|c| c.tt == "ban") {
                        sp.bao("trung_ghe", format!("{g} giữ cho {nguoi} khi đã bán cho {}", c.nguoi));
                        continue;
                    }
                    if let Some(c) = cu.filter(|c| c.nguoi != *nguoi && c.het_han.is_some_and(|h| luc < h)) {
                        sp.bao("trung_ghe", format!("{g} giữ cho {nguoi} khi {} đang giữ", c.nguoi));
                    }
                    self.ghe.insert(g.clone(), GheSo { tt: "giu", nguoi: nguoi.clone(), het_han: gd.het_han });
                }
                "ban" => {
                    self.so_ve += 1;
                    if let Some(c) = cu.filter(|c| c.tt == "ban") {
                        sp.bao("trung_ghe", format!("{g} bán cho {nguoi} khi đã bán cho {}", c.nguoi));
                        continue;
                    }
                    match cu {
                        Some(c) if c.nguoi == *nguoi => {
                            if c.het_han.is_some_and(|h| luc >= h) {
                                sp.bao("tt_sai", format!("{g}: {nguoi} trả tiền khi đã hết hạn giữ"));
                            }
                        }
                        _ => sp.bao("tt_sai", format!("{g}: {nguoi} trả tiền ghế không do mình giữ")),
                    }
                    self.ghe.insert(g.clone(), GheSo { tt: "ban", nguoi: nguoi.clone(), het_han: None });
                }
                _ => {
                    // nhả / hết hạn
                    if cu.is_some_and(|c| c.tt == "giu" && c.nguoi == *nguoi) {
                        self.ghe.remove(g);
                    }
                }
            }
        }
    }
}

// ================================================================ nghiệp vụ
pub struct DatVe {
    dang_don: Mutex<HashSet<String>>,   // ghế quá hạn đang được luồng nền dọn
    nguoi_that: Mutex<HashSet<String>>, // tên người dùng thật (in nhật ký cho họ)
    dem_ghi: AtomicU64,                 // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
    phien: AtomicU64,
}

fn tu_choi(ma_loi: &str, ly_do: String) -> Obj {
    obj(json!({"ok": false, "ma_loi": ma_loi, "ly_do": ly_do}))
}

fn noi(ds: &[String]) -> String {
    ds.join(", ")
}

impl NghiepVu for DatVe {
    type GiaTri = Ghe;
    type Luat = LuatDatVe;
    const TEN: &'static str = "Đặt vé xem phim MR6.8";
    const LOAI_YEU_CAU: &'static [&'static str] = &["giu_cho", "thanh_toan", "nha"];

    fn tham_so_mac_dinh() -> Obj {
        obj(json!({"ttl_s": 30.0, "ghe_nong": "E7", "gia_ve": 90_000, "phim": "Hành tinh xanh", "suat": "19:30"}))
    }

    fn moi() -> DatVe {
        DatVe { dang_don: Mutex::new(HashSet::new()), nguoi_that: Mutex::new(HashSet::new()), dem_ghi: AtomicU64::new(0), phien: AtomicU64::new(0) }
    }

    // ------------------------------------------------------------ tạo / đặt lại
    fn kiem_tham_so(ts: &Obj) -> Result<(), String> {
        if !(1.0..=600.0).contains(&lay_f64(ts, "ttl_s").unwrap_or(30.0)) {
            return Err("thời hạn giữ chỗ 1–600 s".into());
        }
        let nong = ts.get("ghe_nong").map(py_str).unwrap_or_default();
        if !nong.is_empty() && !ghe_hop_le(&nong) {
            return Err("ghế nóng phải là mã ghế A1–H12 hoặc rỗng".into());
        }
        if lay_f64(ts, "gia_ve").unwrap_or(1.0) <= 0.0 {
            return Err("giá vé > 0".into());
        }
        if let Some(p) = ts.get("phim") {
            if !(1..=40).contains(&py_str(p).chars().count()) {
                return Err("tên phim 1–40 ký tự".into());
            }
        }
        Ok(())
    }

    fn khoi_tao(mc: &MayChu<Self>) -> (Vec<(String, Ghe)>, LuatDatVe) {
        mc.nv.dang_don.lock().unwrap().clear();
        mc.nv.nguoi_that.lock().unwrap().clear();
        (tat_ca_ghe().into_iter().map(|g| (g, trong(None))).collect(), LuatDatVe { ghe: HashMap::new(), so_ve: 0 })
    }

    // ------------------------------------------------------------ giữ chỗ / thanh toán / nhả
    fn xu_ly_yeu_cau(mc: &Arc<MayChu<Self>>, khach: &KhachKetNoi, nguoi: &str, tin: &Obj, tra_loi: TraLoi) {
        let Some(ghe) = ds_ghe(tin.get("ghe")) else {
            tra_loi(tu_choi("sai_du_lieu", format!("ghế: danh sách 1–{TOI_DA_GHE} mã A1–H12")));
            return;
        };
        let that = khach.so_nguoi.is_none();
        if that {
            mc.nv.nguoi_that.lock().unwrap().insert(nguoi.to_string());
        }
        let (ttl, gia, suat) = (mc.ts_f64("ttl_s"), mc.ts_i64("gia_ve"), mc.ts_str("suat"));
        let tra: TraLoi = Box::new(move |mut kq: Obj| {
            // bỏ các trường nội bộ ("_…") trước khi gửi
            kq.retain(|k, _| !k.starts_with('_'));
            tra_loi(kq)
        });
        let (nguoi, ds, m2) = (nguoi.to_string(), ghe.clone(), Arc::clone(mc));
        match lay_str(tin, "loai") {
            Some("giu_cho") => {
                let xu_ly: XuLy<Ghe> = Box::new(move |doc| {
                    // hàm THUẦN: mọi ghế phải trống / quá hạn / của mình
                    let luc = bay_gio();
                    let (mut moi, mut con_lai) = (Vec::new(), ttl);
                    for g in &ds {
                        let v = &doc[g.as_str()];
                        if v.tt == "ban" {
                            return (obj(json!({"ok": false, "ma_loi": "da_ban", "ly_do": format!("ghế {g} đã bán"), "ghe_ban": [g]})), None);
                        }
                        if v.giu_con_han(luc) {
                            if v.nguoi.as_deref() != Some(nguoi.as_str()) {
                                return (obj(json!({"ok": false, "ma_loi": "dang_giu", "ly_do": format!("ghế {g} đang có người giữ"), "ghe_ban": [g]})), None);
                            }
                            con_lai = con_lai.min(v.het_han.unwrap_or(luc) - luc);
                        } else {
                            moi.push(g.clone());
                        }
                    }
                    let het_han = luc + ttl;
                    let ghi: Ghi<Ghe> = moi.iter().map(|g| (g.clone(), Ghe { tt: "giu", nguoi: Some(nguoi.clone()), het_han: Some(het_han), ma_ve: None, nguoi_het_han: None })).collect();
                    let kq = obj(json!({"ok": true, "ghe": ds, "moi": moi, "het_han_ms": round_py(con_lai * 1000.0), "nguoi": nguoi, "_luc": luc, "_het_han": het_han}));
                    (kq, Some(ghi))
                });
                mc.kho.thuc_hien(ghe, xu_ly, tra, Some(Box::new(move |kq: &mut Obj| da_ghi(&m2, "giu", kq, that))));
            }
            Some("thanh_toan") => {
                let ve_cua = move |g: &String| format!("S{}-{g}", suat.replace(':', ""));
                let xu_ly: XuLy<Ghe> = Box::new(move |doc| {
                    let luc = bay_gio();
                    let la_toi = |v: &Ghe| v.nguoi.as_deref() == Some(nguoi.as_str());
                    for g in &ds {
                        let v = &doc[g.as_str()];
                        if v.giu_con_han(luc) && la_toi(v) {
                            continue;
                        }
                        if (v.tt == "giu" && la_toi(v)) || (v.tt == "trong" && v.nguoi_het_han.as_deref() == Some(nguoi.as_str())) {
                            return (tu_choi("het_han", format!("giữ chỗ {g} đã hết hạn")), None);
                        }
                        return (tu_choi("khong_giu", format!("ghế {g} không do bạn giữ")), None);
                    }
                    let ve: Vec<String> = ds.iter().map(&ve_cua).collect();
                    let ghi: Ghi<Ghe> = ds.iter().zip(&ve).map(|(g, v)| (g.clone(), Ghe { tt: "ban", nguoi: Some(nguoi.clone()), het_han: None, ma_ve: Some(v.clone()), nguoi_het_han: None })).collect();
                    (obj(json!({"ok": true, "ghe": ds, "ma_ve": ve, "tien": gia * ds.len() as i64, "nguoi": nguoi, "_luc": luc})), Some(ghi))
                });
                mc.kho.thuc_hien(ghe, xu_ly, tra, Some(Box::new(move |kq: &mut Obj| da_ghi(&m2, "ban", kq, that))));
            }
            _ => {
                let xu_ly: XuLy<Ghe> = Box::new(move |doc| {
                    let nha: Vec<String> = ds.iter().filter(|g| doc[g.as_str()].tt == "giu" && doc[g.as_str()].nguoi.as_deref() == Some(nguoi.as_str())).cloned().collect();
                    if nha.is_empty() {
                        return (tu_choi("khong_giu", "bạn không giữ ghế nào trong số này".into()), None);
                    }
                    let ghi: Ghi<Ghe> = nha.iter().map(|g| (g.clone(), trong(None))).collect();
                    (obj(json!({"ok": true, "ghe": nha, "nguoi": nguoi})), Some(ghi))
                });
                mc.kho.thuc_hien(ghe, xu_ly, tra, Some(Box::new(move |kq: &mut Obj| da_ghi(&m2, "nha", kq, that))));
            }
        }
    }

    // ------------------------------------------------------------ dọn ghế quá hạn (luồng nền, 100 ms)
    fn dinh_ky(mc: &Arc<MayChu<Self>>, bay: f64) {
        for g in tat_ca_ghe() {
            let v = mc.kho.doc(&g);
            if v.tt != "giu" || v.het_han.is_some_and(|h| bay < h) || mc.nv.dang_don.lock().unwrap().contains(&g) {
                continue;
            }
            mc.nv.dang_don.lock().unwrap().insert(g.clone());
            let g2 = g.clone();
            let xu_ly: XuLy<Ghe> = Box::new(move |doc| {
                // nhả ghế quá hạn cũng là một thao tác ghi
                let v = &doc[g2.as_str()];
                if v.tt != "giu" || v.het_han.is_some_and(|h| bay_gio() < h) {
                    return (obj(json!({"ok": false})), None); // vừa được thanh toán / giữ lại
                }
                (obj(json!({"ok": true, "ghe": [g2], "nguoi": v.nguoi})), Some(vec![(g2.clone(), trong(v.nguoi.clone()))]))
            });
            let (m1, m2, g3) = (Arc::clone(mc), Arc::clone(mc), g.clone());
            mc.kho.thuc_hien(
                vec![g],
                xu_ly,
                Box::new(move |_| {
                    m1.nv.dang_don.lock().unwrap().remove(&g3);
                }),
                Some(Box::new(move |kq: &mut Obj| da_ghi(&m2, "het_han", kq, false))),
            );
        }
    }

    // ------------------------------------------------------------ trạng thái gửi client
    fn trang_thai(mc: &MayChu<Self>) -> Obj {
        let phien = mc.nv.phien.load(Ordering::SeqCst); // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
        let bay = bay_gio();
        let doc = mc.kho.anh_chup();
        let (mut so_do, mut chu, mut con_lai) = (Vec::new(), Obj::new(), Obj::new());
        let (mut n_trong, mut n_giu, mut n_ban) = (0, 0, 0);
        for r in HANG.chars() {
            let mut dong = String::new();
            for c in 1..=COT {
                let g = format!("{r}{c}");
                let Some(v) = doc.get(&g) else { continue };
                if v.giu_con_han(bay) {
                    dong.push('g');
                    chu.insert(g.clone(), json!(v.nguoi));
                    con_lai.insert(g, json!(round_py((v.het_han.unwrap_or(bay) - bay) * 1000.0)));
                    n_giu += 1;
                } else if v.tt == "ban" {
                    dong.push('b');
                    chu.insert(g, json!(v.nguoi));
                    n_ban += 1;
                } else {
                    dong.push('.');
                    n_trong += 1;
                }
            }
            so_do.push(dong);
        }
        obj(json!({"phien": phien, "phim": mc.ts_str("phim"), "suat": mc.ts_str("suat"), "gia_ve": mc.ts_i64("gia_ve"), "ttl_s": mc.ts_f64("ttl_s"),
                   "ghe_nong": mc.ts_str("ghe_nong"), "hang": HANG, "cot": COT, "so_do": so_do, "chu": chu,
                   "con_lai_ms": con_lai, "dem": {"trong": n_trong, "giu": n_giu, "ban": n_ban}}))
    }

    fn tom_tat(mc: &MayChu<Self>) -> String {
        let bay = bay_gio();
        let doc = mc.kho.anh_chup();
        let ban = doc.values().filter(|v| v.tt == "ban").count();
        let giu = doc.values().filter(|v| v.giu_con_han(bay)).count();
        format!("Đã bán {ban}/{} ghế, đang giữ {giu}, sổ {} vé", doc.ds.len(), mc.so().xem(|l| l.so_ve))
    }
}

/// khi_ghi: gọi trong vùng găng ngay sau khi ghi – ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ
/// tự lần ghi; cap_nhat mang "phien" lúc chụp -> client biết cap_nhat chụp trước hay sau lần ghi của mình.
fn da_ghi(mc: &MayChu<DatVe>, loai: &'static str, kq: &mut Obj, that: bool) {
    let phien = mc.nv.dem_ghi.fetch_add(1, Ordering::SeqCst) + 1;
    mc.nv.phien.store(phien, Ordering::SeqCst);
    kq.insert("phien".into(), json!(phien));
    let ds = |k: &str| -> Vec<String> { kq.get(k).and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default() };
    let ghe = ds(if loai == "giu" { "moi" } else { "ghe" });
    let nguoi = lay_str(kq, "nguoi").unwrap_or("").to_string();
    mc.so().ghi(GdDatVe { loai, ghe: ghe.clone(), nguoi: nguoi.clone(), luc: lay_f64(kq, "_luc").unwrap_or(0.0), het_han: lay_f64(kq, "_het_han") });
    match loai {
        "giu" if that => mc.thong_bao(&format!("{nguoi} giữ {} ({} s)", noi(&ghe), fmt_g(mc.ts_f64("ttl_s")))),
        "ban" if that => mc.thong_bao(&format!("{nguoi} mua {} vé {} ({})", ghe.len(), noi(&ghe), tien(lay_i64(kq, "tien").unwrap_or(0)))),
        "nha" if that => mc.thong_bao(&format!("{nguoi} nhả {}", noi(&ghe))),
        "het_han" if mc.nv.nguoi_that.lock().unwrap().contains(&nguoi) => mc.thong_bao(&format!("Hết hạn giữ {} ({nguoi}), ghế trống lại", ghe.first().cloned().unwrap_or_default())),
        _ => {}
    }
    mc.bao_doi();
}
