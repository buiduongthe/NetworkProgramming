//! Bài mở rộng MR6.14 – Ứng dụng đặt vé xe khách theo chặng (người dùng, egui).
//!
//! Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở gui_chung::CuaSoNguoiDung; tệp này vẽ ứng
//! dụng đặt vé: băng rôn nhà xe, chọn "Đi từ" – "Đến" (thanh tuyến Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang tô đoạn
//! đã chọn), sơ đồ xe (giường nằm 2 tầng hoặc ghế ngồi 50 chỗ, có vô lăng ở đầu xe). Mỗi chỗ tô màu theo ĐOẠN
//! đang chọn: trống / đang chọn / người khác giữ / đã bán / bạn giữ / vé của bạn, và có dải nhỏ ở chân chỗ, mỗi
//! chặng một vạch, cho thấy chỗ đó bận ở chặng nào. Bấm chỗ để chọn, "GIỮ CHỖ" gửi giu_cho, đồng hồ giữ chỗ đếm
//! ngược (đỏ khi < 10 s), "THANH TOÁN" và "Nhả chỗ". Sơ đồ cập nhật trực tiếp từ cap_nhat (≤ 10 lần/giây).
//!
//! Chạy:  dat_ve_xe_gui <tên> [host] [cổng]       (mặc định 127.0.0.1 44782)

use crate::chung::*;
use crate::dat_ve_xe::chu_o;
use crate::gui_chung::*;
use crate::may_chu::{bo_tri, ten_doan, CONG_GUI, MA_TRAM, SO_CHANG, TOI_DA_GHE, TRAM};
use eframe::egui::{self, Color32, Pos2, RichText};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

pub const TRONG: Color32 = Color32::WHITE;
pub const CHON: Color32 = Color32::from_rgb(0xb9, 0xf6, 0xca);
pub const KHAC_GIU: Color32 = Color32::from_rgb(0xff, 0xa7, 0x26);
pub const DA_BAN: Color32 = Color32::from_rgb(0x9e, 0x9e, 0x9e);
pub const TOI_GIU: Color32 = Color32::from_rgb(0x2e, 0x7d, 0x32);
pub const VE_TOI: Color32 = Color32::from_rgb(0x15, 0x65, 0xc0);
pub const MAU_XE: Color32 = Color32::from_rgb(0x0b, 0x5d, 0x3b);
pub const NEN_XE: Color32 = Color32::from_rgb(0xe9, 0xef, 0xeb);
const VIEN: Color32 = Color32::from_rgb(0x60, 0x7d, 0x74);
const CHU_TOI: Color32 = Color32::from_rgb(0x26, 0x32, 0x38);
const CHU_GIAI: [(Color32, &str); 6] = [(TRONG, "Trống"), (CHON, "Đang chọn"), (KHAC_GIU, "Người khác giữ"), (DA_BAN, "Đã bán"), (TOI_GIU, "Bạn giữ"), (VE_TOI, "Vé của bạn")];

fn mau_vach(x: char) -> Color32 {
    match x {
        'g' => KHAC_GIU,
        'b' => DA_BAN,
        'G' => TOI_GIU,
        'V' => VE_TOI,
        _ => Color32::from_rgb(0xdf, 0xe6, 0xe2),
    }
}

pub fn dong_ho(giay: f64) -> String {
    let giay = (giay + 0.999).max(0.0) as i64;
    format!("{:02}:{:02}", giay / 60, giay % 60)
}

pub fn dong_cua(tt: &Obj, g: &str) -> String {
    let ghe = tt.get("ghe").and_then(Value::as_array);
    let i = ghe.and_then(|a| a.iter().position(|x| x.as_str() == Some(g)));
    i.and_then(|i| tt.get("so_do")?.get(i)?.as_str().map(str::to_string)).unwrap_or_else(|| ".".repeat(SO_CHANG))
}

/// Trạng thái chỗ g cho đoạn [tu, den): "trong" | "giu" | "ban" | "toi_giu" | "ve".
pub fn trang_cho(tt: &Obj, g: &str, ten: Option<&str>, tu: usize, den: usize) -> &'static str {
    let dong: Vec<char> = dong_cua(tt, g).chars().collect();
    let mut kq = "trong";
    for k in tu..den {
        let x = dong.get(k).copied().unwrap_or('.');
        let toi = ten.is_some() && chu_o(tt, &format!("{g}#{k}")) == ten;
        match (x, toi) {
            ('b', false) => return "ban",
            ('g', false) => kq = "giu",
            ('g', true) if kq == "trong" => kq = "toi_giu",
            ('b', true) if kq == "trong" || kq == "toi_giu" => kq = "ve",
            _ => {}
        }
    }
    kq
}

/// Một ký tự mỗi chặng: . g b (người khác) G V (của mình).
pub fn vach(tt: &Obj, g: &str, ten: Option<&str>) -> String {
    dong_cua(tt, g)
        .chars()
        .enumerate()
        .map(|(k, x)| if ten.is_some() && chu_o(tt, &format!("{g}#{k}")) == ten { match x { 'g' => 'G', 'b' => 'V', c => c } } else { x })
        .collect()
}

/// Sơ đồ xe; dùng ở ứng dụng đặt vé (k = 1) và bảng điều khiển máy chủ (thu nhỏ, k < 1).
pub struct SoDoXe {
    pub loai_xe: String,
    k: f32,
    chu: bool,
    pub rong: f32,
    pub cao: f32,
    tang: Vec<(f32, f32, &'static str)>,
    vi_tri: Vec<(String, egui::Rect)>, // tọa độ cục bộ
}

impl SoDoXe {
    pub fn new(loai_xe: &str, k: f32, chu: bool) -> SoDoXe {
        let (w, h, khe, tang, rong, cao);
        if loai_xe == "giuong" {
            (w, h, khe) = (46.0 * k, 30.0 * k, 6.0 * k);
            let rt = 3.0 * w + 2.0 * khe;
            tang = vec![(12.0 * k, 12.0 * k + rt, "TẦNG DƯỚI"), (12.0 * k + rt + 26.0 * k, 12.0 * k + 2.0 * rt + 26.0 * k, "TẦNG TRÊN")];
            rong = 24.0 * k + 2.0 * rt + 26.0 * k;
            cao = 40.0 * k + 7.0 * (h + khe) + 4.0 * k;
        } else {
            (w, h, khe) = (34.0 * k, 26.0 * k, 5.0 * k);
            tang = vec![(12.0 * k, 12.0 * k + 6.0 * (w + khe) - khe, "")];
            rong = 24.0 * k + 6.0 * (w + khe) - khe;
            cao = 40.0 * k + 12.0 * (h + khe) + 4.0 * k;
        }
        let vi_tri = bo_tri(loai_xe)
            .into_iter()
            .map(|x| {
                let px = tang[if loai_xe == "giuong" { x.tang } else { 0 }].0 + x.cot as f32 * (w + khe);
                let py = 40.0 * k + x.hang as f32 * (h + khe);
                (x.ma, egui::Rect::from_min_size(egui::pos2(px, py), egui::vec2(w, h)))
            })
            .collect();
        SoDoXe { loai_xe: loai_xe.to_string(), k, chu, rong, cao, tang, vi_tri }
    }

    /// Vẽ thân xe, vô lăng và mọi chỗ theo đoạn [tu, den) kèm dải vạch theo chặng; vien {chỗ: màu viền} (ghế nóng).
    pub fn ve(&self, pt: &egui::Painter, g: Pos2, tt: Option<&Obj>, ten: Option<&str>, tu: usize, den: usize, chon: &BTreeSet<String>, vien: &HashMap<String, Color32>) {
        let k = self.k;
        for &(x0, x1, ten_tang) in &self.tang {
            let than = egui::Rect::from_min_max(p(g, x0 - 6.0 * k, 8.0 * k), p(g, x1 + 6.0 * k, self.cao));
            pt.rect_filled(than, 0.0, mau("#f7faf8"));
            pt.rect_stroke(than, 0.0, egui::Stroke::new((2.0 * k).max(1.0), MAU_XE));
            if self.chu && !ten_tang.is_empty() {
                pt.text(p(g, (x0 + x1) / 2.0, 20.0 * k), egui::Align2::CENTER_CENTER, ten_tang, egui::FontId::proportional(10.0), MAU_XE);
            }
        }
        if self.chu || k >= 0.5 {
            // vô lăng ở đầu xe (bên trái, tầng dưới)
            let r = 9.0 * k;
            let c = p(g, self.tang[0].0 + r + 2.0 * k, 22.0 * k);
            pt.circle_stroke(c, r, egui::Stroke::new((2.0 * k).max(1.0), CHU_TOI));
            pt.line_segment([egui::pos2(c.x - r, c.y), egui::pos2(c.x + r, c.y)], egui::Stroke::new((2.0 * k).max(1.0), CHU_TOI));
        }
        let co: Vec<String> = tt.and_then(|t| t.get("ghe")).and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default();
        for (ma, o) in &self.vi_tri {
            let r = o.translate(g.to_vec2());
            let (trang, vc) = match tt {
                Some(t) if co.contains(ma) => (trang_cho(t, ma, ten, tu, den), vach(t, ma, ten)),
                _ => ("trong", ".".repeat(SO_CHANG)),
            };
            let m = match trang {
                "giu" => KHAC_GIU,
                "ban" => DA_BAN,
                "toi_giu" => TOI_GIU,
                "ve" => VE_TOI,
                _ if chon.contains(ma) => CHON,
                _ => TRONG,
            };
            pt.rect_filled(r, 0.0, m);
            let (v, day) = match vien.get(ma) {
                Some(c) => (*c, 2.0_f32),
                None => (VIEN, 1.0),
            };
            pt.rect_stroke(r, 0.0, egui::Stroke::new(day, v));
            let n = vc.chars().count().max(1) as f32;
            let cao_vach = (5.0 * k).max(2.0);
            for (i, x) in vc.chars().enumerate() {
                // dải vạch: mỗi chặng một ô ở chân chỗ
                let a = r.min.x + 2.0 + (r.width() - 4.0) * i as f32 / n;
                let b = r.min.x + 2.0 + (r.width() - 4.0) * (i + 1) as f32 / n - 1.0;
                let o = egui::Rect::from_min_max(egui::pos2(a, r.max.y - cao_vach - 1.0), egui::pos2(b, r.max.y - 1.0));
                pt.rect_filled(o, 0.0, mau_vach(x));
                if tu <= i && i < den {
                    pt.rect_stroke(o, 0.0, egui::Stroke::new(1.0_f32, CHU_TOI));
                }
            }
            if self.chu {
                let toi = m == TOI_GIU || m == VE_TOI || m == DA_BAN;
                pt.text(egui::pos2(r.center().x, r.center().y - 3.0 * k), egui::Align2::CENTER_CENTER, ma, egui::FontId::proportional(9.5), if toi { Color32::WHITE } else { CHU_TOI });
            }
        }
    }

    pub fn cho_tai(&self, x: f32, y: f32) -> Option<String> {
        self.vi_tri.iter().find(|(_, r)| r.contains(egui::pos2(x, y))).map(|(g, _)| g.clone())
    }
}

fn sap(ds: &BTreeSet<String>) -> String {
    ds.iter().cloned().collect::<Vec<_>>().join(", ")
}

#[derive(Clone)]
pub struct VeXe {
    pub ghe: String,
    pub tu: usize,
    pub den: usize,
}

pub struct UngDungDatVeXe {
    so_do: Option<SoDoXe>,
    pub chon: BTreeSet<String>, // chỗ đang chọn (chưa giữ)
    pub giu: BTreeSet<String>,  // chỗ mình đang giữ (theo kq) cho đoạn doan_giu
    doan_giu: (usize, usize),
    phien_giu: i64, // số thứ tự lần ghi (kq) của lần giữ chỗ / thanh toán gần nhất
    phien_ve: i64,
    pub ve: Vec<VeXe>, // vé của mình
    han: Option<f64>,  // thời điểm (bay_gio) hết hạn giữ chỗ
    pub so_giu: u32,
    so_ve: usize,
    pub tu: usize,
    pub den: usize,
    pub thong_bao: (String, Color32, Color32),
    pub dong_ho: (String, Color32),
    tt_bat: bool,
}

impl UngDungDatVeXe {
    fn bao(&mut self, text: &str, nen: Color32, chu: Color32) {
        self.thong_bao = (text.to_string(), nen, chu);
    }

    fn gia(nd: &NguoiDung, tu: usize, den: usize, so_cho: usize) -> i64 {
        let g: Vec<i64> = nd.tt.as_ref().and_then(|t| t.get("gia_chang")).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
        g.iter().skip(tu).take(den.saturating_sub(tu)).sum::<i64>() * so_cho as i64
    }

    fn con_han(&self) -> bool {
        self.han.is_some_and(|h| bay_gio() < h)
    }

    pub fn giu_bat(&self, nd: &NguoiDung) -> bool {
        nd.kn.is_some() && !self.chon.is_empty() && self.giu.is_empty()
    }

    pub fn tt_bat(&self, nd: &NguoiDung) -> bool {
        nd.kn.is_some() && !self.giu.is_empty() && self.con_han()
    }

    fn trang(&self, nd: &NguoiDung, g: &str) -> &'static str {
        nd.tt.as_ref().map_or("trong", |t| trang_cho(t, g, Some(nd.ten.as_str()), self.tu, self.den))
    }

    /// Chọn đoạn đường (người dùng hoặc kịch bản demo): chỉ số trạm; "Đến" phải sau "Đi từ".
    pub fn dat_doan(&mut self, nd: &NguoiDung, tu: usize, den: usize) {
        if !self.giu.is_empty() {
            return; // đang giữ chỗ: không đổi đoạn
        }
        let tu = tu.min(SO_CHANG - 1);
        self.tu = tu;
        self.den = den.clamp(tu + 1, SO_CHANG);
        let bo: BTreeSet<String> = self.chon.iter().filter(|g| self.trang(nd, g) != "trong").cloned().collect();
        if !bo.is_empty() {
            for g in &bo {
                self.chon.remove(g);
            }
            self.bao(&format!("Chỗ {} không trống cả đoạn {}", sap(&bo), ten_doan(self.tu, self.den)), mau("#ffe0b2"), Color32::BLACK);
        }
    }

    /// Bấm một chỗ (người dùng hoặc kịch bản demo).
    pub fn bam(&mut self, nd: &NguoiDung, g: &str) {
        if nd.kn.is_none() || nd.tt.is_none() {
            return;
        }
        let trang = self.trang(nd, g);
        if self.chon.contains(g) {
            self.chon.remove(g);
        } else if trang == "trong" && !self.giu.contains(g) {
            if self.chon.len() + self.giu.len() >= TOI_DA_GHE {
                self.bao(&format!("Tối đa {TOI_DA_GHE} chỗ mỗi lần"), mau("#ffe0b2"), Color32::BLACK);
                return;
            }
            self.chon.insert(g.to_string());
        } else if trang == "giu" {
            self.bao(&format!("Chỗ {g} đang có người giữ trên đoạn này"), mau("#ffe0b2"), Color32::BLACK);
        } else if trang == "ban" {
            self.bao(&format!("Chỗ {g} đã bán trên đoạn này"), mau("#e0e0e0"), Color32::BLACK);
        }
        self.tt_bat = self.tt_bat(nd);
    }

    /// Bấm "GIỮ CHỖ" (nút chỉ bấm được khi đã chọn chỗ và chưa giữ).
    pub fn giu_cho(&mut self, nd: &mut NguoiDung) {
        if !self.giu_bat(nd) {
            return;
        }
        let ghe: Vec<String> = self.chon.iter().cloned().collect();
        nd.nhat_ky.ghi(&format!("Giữ chỗ {} {}…", ghe.join(", "), ten_doan(self.tu, self.den)));
        self.bao("Đang giữ chỗ…", mau("#fff3c4"), Color32::BLACK);
        nd.gui_yc("giu_cho", obj(json!({"ghe": ghe, "tu": self.tu, "den": self.den})));
    }

    /// Bấm "THANH TOÁN" (bấm được khi đang giữ, còn hạn).
    pub fn thanh_toan(&mut self, nd: &mut NguoiDung) {
        if self.tt_bat(nd) {
            nd.nhat_ky.ghi(&format!("Thanh toán {}…", sap(&self.giu)));
            nd.gui_yc("thanh_toan", obj(json!({"ghe": self.giu.iter().collect::<Vec<_>>(), "tu": self.doan_giu.0, "den": self.doan_giu.1})));
        }
    }

    pub fn nha(&mut self, nd: &mut NguoiDung) {
        if !self.giu.is_empty() {
            nd.gui_yc("nha", obj(json!({"ghe": self.giu.iter().collect::<Vec<_>>(), "tu": self.doan_giu.0, "den": self.doan_giu.1})));
        }
    }

    /// Chỗ mình giữ/mua bị người khác lấy (tranh chấp) hoặc hết hạn; chỗ đang chọn bị người khác giữ.
    /// Chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq giữ chỗ / thanh toán).
    fn kiem_cho(&mut self, nd: &mut NguoiDung) {
        let Some(tt) = nd.tt.clone() else { return };
        let phien = lay_i64(&tt, "phien").unwrap_or(0);
        let ten = nd.ten.clone();
        if phien >= self.phien_giu && !self.giu.is_empty() {
            let (tu, den) = self.doan_giu;
            for g in self.giu.clone() {
                let chu: Vec<Option<&str>> = (tu..den).map(|k| chu_o(&tt, &format!("{g}#{k}"))).collect();
                if chu.iter().all(|a| *a == Some(ten.as_str())) {
                    continue;
                }
                self.giu.remove(&g);
                match chu.iter().flatten().find(|a| **a != ten) {
                    Some(ai) => {
                        self.bao(&format!("Chỗ {g} đã bị {ai} lấy mất!"), mau("#ff6060"), Color32::WHITE);
                        nd.nhat_ky.ghi(&format!("Chỗ {g} đã bị {ai} lấy mất!"));
                    }
                    None => {
                        self.bao("Hết thời gian giữ chỗ – chỗ đã được nhả", mau("#ffd0d0"), DO_DAM);
                        nd.nhat_ky.ghi(&format!("Hết thời gian giữ chỗ: {g}"));
                    }
                }
            }
            if self.giu.is_empty() {
                self.han = None;
            }
        }
        if phien >= self.phien_ve {
            let mut con = Vec::new();
            for v in self.ve.clone() {
                let ai = (v.tu..v.den).map(|k| chu_o(&tt, &format!("{}#{k}", v.ghe))).find(|a| *a != Some(ten.as_str()));
                match ai {
                    None => con.push(v),
                    Some(ai) => {
                        let ai = ai.unwrap_or("không ai");
                        self.bao(&format!("Vé {} bị ghi cho {ai}!", v.ghe), mau("#ff6060"), Color32::WHITE);
                        nd.nhat_ky.ghi(&format!("Vé {} bị ghi cho {ai}!", v.ghe));
                    }
                }
            }
            self.ve = con;
        }
        let bi_lay: BTreeSet<String> = self.chon.iter().filter(|g| self.trang(nd, g) != "trong").cloned().collect();
        if !bi_lay.is_empty() {
            for g in &bi_lay {
                self.chon.remove(g);
            }
            self.bao(&format!("Chỗ {} vừa có người giữ", sap(&bi_lay)), mau("#ffe0b2"), Color32::BLACK);
            nd.nhat_ky.ghi(&format!("Chỗ {} vừa có người giữ", sap(&bi_lay)));
        }
    }

    fn ve_bang_ron(&self, pt: &egui::Painter, g: Pos2, tt: Option<&Obj>) {
        pt.rect_filled(egui::Rect::from_min_size(g, egui::vec2(256.0, 62.0)), 0.0, MAU_XE);
        pt.rect_filled(egui::Rect::from_min_size(p(g, 0.0, 44.0), egui::vec2(256.0, 18.0)), 0.0, mau("#0f7a4f"));
        let Some(tt) = tt else { return };
        pt.text(p(g, 10.0, 13.0), egui::Align2::LEFT_CENTER, so_us(tt.get("nha_xe")).to_uppercase(), egui::FontId::proportional(14.0), Color32::WHITE);
        pt.text(p(g, 10.0, 32.0), egui::Align2::LEFT_CENTER, format!("Chuyến {} · {}", so_us(tt.get("chuyen")), so_us(tt.get("ten_xe"))), egui::FontId::proportional(10.5), mau("#d7f0e2"));
        pt.text(p(g, 10.0, 53.0), egui::Align2::LEFT_CENTER, format!("Giữ chỗ {} s · giá theo chặng", fmt_g(lay_f64(tt, "ttl_s").unwrap_or(0.0))), egui::FontId::proportional(10.5), mau("#ffe082"));
    }

    fn ve_tuyen(&self, pt: &egui::Painter, g: Pos2, tt: Option<&Obj>) {
        pt.rect_filled(egui::Rect::from_min_size(g, egui::vec2(256.0, 40.0)), 0.0, Color32::WHITE);
        let n = TRAM.len();
        let x: Vec<f32> = (0..n).map(|i| 16.0 + i as f32 * 224.0 / (n - 1) as f32).collect();
        pt.line_segment([p(g, x[0], 12.0), p(g, x[n - 1], 12.0)], egui::Stroke::new(6.0_f32, mau("#cfd8dc")));
        pt.line_segment([p(g, x[self.tu], 12.0), p(g, x[self.den], 12.0)], egui::Stroke::new(6.0_f32, MAU_XE));
        let gia: Vec<i64> = tt.and_then(|t| t.get("gia_chang")).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
        for i in 0..n {
            let nen = if self.tu <= i && i <= self.den { MAU_XE } else { Color32::WHITE };
            pt.circle(p(g, x[i], 12.0), 6.0, nen, egui::Stroke::new(2.0_f32, MAU_XE));
            pt.text(p(g, x[i], 30.0), egui::Align2::CENTER_CENTER, MA_TRAM[i], egui::FontId::proportional(10.0), CHU_TOI);
            if i + 1 < n && i < gia.len() {
                pt.text(p(g, (x[i] + x[i + 1]) / 2.0, 30.0), egui::Align2::CENTER_CENTER, format!("{}k", gia[i] / 1000), egui::FontId::proportional(9.0), VIEN);
            }
        }
    }
}

impl UngDung for UngDungDatVeXe {
    const TIEU_DE: &'static str = "Đặt vé xe khách";

    fn moi(_ten: &str) -> Self {
        UngDungDatVeXe {
            so_do: None,
            chon: BTreeSet::new(),
            giu: BTreeSet::new(),
            doan_giu: (0, SO_CHANG),
            phien_giu: 0,
            phien_ve: 0,
            ve: Vec::new(),
            han: None,
            so_giu: 0,
            so_ve: 0,
            tu: 0,
            den: SO_CHANG,
            thong_bao: ("Chưa kết nối".into(), mau("#e8e8e8"), Color32::BLACK),
            dong_ho: ("--:--".into(), mau("#909090")),
            tt_bat: false,
        }
    }

    fn ve(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung) {
        if let Some(loai) = nd.tt.as_ref().and_then(|t| lay_str(t, "loai_xe")) {
            if self.so_do.as_ref().is_none_or(|s| s.loai_xe != loai) {
                self.so_do = Some(SoDoXe::new(loai, 1.0, true));
            }
        }
        let rong = self.so_do.as_ref().map_or(376.0, |s| s.rong.max(376.0));
        ui.horizontal_top(|ui| {
            let (r, pt) = ui.allocate_painter(egui::vec2(rong, 480.0), egui::Sense::click());
            pt.rect_filled(r.rect, 0.0, NEN_XE);
            if let Some(sd) = &self.so_do {
                sd.ve(&pt, r.rect.min, nd.tt.as_ref(), Some(nd.ten.as_str()), self.tu, self.den, &self.chon, &HashMap::new());
                let (mut x, mut y) = (8.0, sd.cao + 10.0);
                for (i, (c, chu)) in CHU_GIAI.iter().enumerate() {
                    // chú giải màu, 2 hàng
                    if i == 3 {
                        (x, y) = (8.0, y + 16.0);
                    }
                    let o = egui::Rect::from_min_size(p(r.rect.min, x, y), egui::vec2(14.0, 11.0));
                    pt.rect_filled(o, 0.0, *c);
                    pt.rect_stroke(o, 0.0, egui::Stroke::new(1.0_f32, VIEN));
                    pt.text(p(r.rect.min, x + 18.0, y + 6.0), egui::Align2::LEFT_CENTER, *chu, egui::FontId::proportional(10.0), CHU_TOI);
                    x += 30.0 + chu.chars().count() as f32 * 6.0;
                }
                let chang: Vec<String> = (0..SO_CHANG).map(|k| format!("{}-{}", MA_TRAM[k], MA_TRAM[k + 1])).collect();
                pt.text(p(r.rect.min, 8.0, y + 26.0), egui::Align2::LEFT_CENTER, format!("Vạch ở chân chỗ: từng chặng {}", chang.join(" | ")), egui::FontId::proportional(9.0), mau("#455a64"));
                if r.clicked() {
                    if let Some(pos) = r.interact_pointer_pos() {
                        if let Some(g) = sd.cho_tai(pos.x - r.rect.min.x, pos.y - r.rect.min.y) {
                            self.bam(nd, &g);
                        }
                    }
                }
            }
            egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, Color32::BLACK)).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(258.0);
                    let (r, pt) = ui.allocate_painter(egui::vec2(256.0, 62.0), egui::Sense::hover());
                    self.ve_bang_ron(&pt, r.rect.min, nd.tt.as_ref());
                    let (mut tu, mut den) = (self.tu, self.den);
                    ui.add_enabled_ui(self.giu.is_empty(), |ui| {
                        ui.horizontal(|ui| {
                            ui.add_sized([44.0, 20.0], egui::Label::new("Đi từ"));
                            egui::ComboBox::from_id_salt("di_tu").width(140.0).selected_text(TRAM[tu]).show_ui(ui, |ui| {
                                for (i, t) in TRAM[..SO_CHANG].iter().enumerate() {
                                    ui.selectable_value(&mut tu, i, *t);
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.add_sized([44.0, 20.0], egui::Label::new("Đến"));
                            egui::ComboBox::from_id_salt("den").width(140.0).selected_text(TRAM[den]).show_ui(ui, |ui| {
                                for (i, t) in TRAM.iter().enumerate().skip(1) {
                                    ui.selectable_value(&mut den, i, *t);
                                }
                            });
                        });
                    });
                    if (tu, den) != (self.tu, self.den) {
                        self.dat_doan(nd, tu, den);
                    }
                    let (r, pt) = ui.allocate_painter(egui::vec2(256.0, 40.0), egui::Sense::hover());
                    self.ve_tuyen(&pt, r.rect.min, nd.tt.as_ref());
                    let (ghe, tam, chu_tt) = if self.giu.is_empty() {
                        (
                            format!("Chỗ đã chọn: {}", if self.chon.is_empty() { "–".into() } else { sap(&self.chon) }),
                            format!("{} · tạm tính {}", ten_doan(self.tu, self.den), tien(Self::gia(nd, self.tu, self.den, self.chon.len()))),
                            "THANH TOÁN".to_string(),
                        )
                    } else {
                        let g = Self::gia(nd, self.doan_giu.0, self.doan_giu.1, self.giu.len());
                        (format!("Đang giữ: {}", sap(&self.giu)), format!("{} · tổng {}", ten_doan(self.doan_giu.0, self.doan_giu.1), tien(g)), format!("THANH TOÁN {}", tien(g)))
                    };
                    ui.label(RichText::new(ghe).strong().size(13.0));
                    ui.label(tam);
                    let gb = self.giu_bat(nd);
                    if nut_chinh(ui, "GIỮ CHỖ", if gb { mau("#ff8f00") } else { mau("#ffcc80") }, gb, 256.0, 30.0) {
                        self.giu_cho(nd);
                    }
                    ui.horizontal(|ui| {
                        ui.label("Giữ chỗ còn");
                        ui.label(RichText::new(&self.dong_ho.0).monospace().strong().size(22.0).color(self.dong_ho.1));
                    });
                    let tb = self.tt_bat(nd);
                    if nut_chinh(ui, &chu_tt, if tb { MAU_XE } else { mau("#8fb8a3") }, tb, 256.0, 30.0) {
                        self.thanh_toan(nd);
                    }
                    if nut(ui, "Nhả chỗ", mau("#f0f0f0"), nd.kn.is_some() && !self.giu.is_empty(), 70.0) {
                        self.nha(nd);
                    }
                    nhan_nen(ui, &self.thong_bao.0.clone(), self.thong_bao.1, self.thong_bao.2, 12.0, 244.0);
                    let ds: Vec<String> = self.ve.iter().map(|v| format!("{} {}→{}", v.ghe, MA_TRAM[v.tu], MA_TRAM[v.den])).collect();
                    let ve = format!("Vé của bạn: {}\nBạn: giữ được {} lần, mua {} vé", if ds.is_empty() { "–".into() } else { ds.join(", ") }, self.so_giu, self.so_ve);
                    ui.add(egui::Label::new(RichText::new(ve).size(12.0).color(VE_TOI)).wrap());
                });
            });
        });
    }

    fn ket_qua(&mut self, nd: &mut NguoiDung, loai: &str, truong: &Obj, tin: &Obj, rtt: f64) {
        let ghe: Vec<String> = truong.get("ghe").and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default();
        let (tu, den) = (lay_i64(truong, "tu").unwrap_or(0) as usize, lay_i64(truong, "den").unwrap_or(0) as usize);
        let ds = |k: &str| -> Vec<String> { tin.get(k).and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default() };
        let ly_do = so_us(tin.get("ly_do"));
        match loai {
            "giu_cho" => {
                if dung(tin, "ok") {
                    self.so_giu += 1;
                    self.giu = ds("ghe").into_iter().collect();
                    self.doan_giu = (tu, den);
                    self.phien_giu = lay_i64(tin, "phien").unwrap_or(0);
                    for g in &self.giu {
                        self.chon.remove(g);
                    }
                    let ms = lay_f64(tin, "het_han_ms").unwrap_or(0.0);
                    self.han = Some(bay_gio() + ms / 1000.0);
                    self.bao(&format!("Đã giữ {} – thanh toán trong {:.0} s", ghe.join(", "), ms / 1000.0), mau("#c8f0d2"), XANH_DAM);
                    nd.nhat_ky.ghi(&format!("  ĐÃ GIỮ {} ({rtt:.1} ms)", ghe.join(", ")));
                    self.kiem_cho(nd);
                } else {
                    let bo = if tin.contains_key("ghe_ban") { ds("ghe_ban") } else { ghe.clone() };
                    for g in bo {
                        self.chon.remove(&g);
                    }
                    self.bao(&format!("Không giữ được: {ly_do}"), mau("#ffd0d0"), DO_DAM);
                    nd.nhat_ky.ghi(&format!("  không giữ được: {ly_do}"));
                }
            }
            "thanh_toan" => {
                if dung(tin, "ok") {
                    self.so_ve += ghe.len();
                    self.ve.extend(ghe.iter().map(|g| VeXe { ghe: g.clone(), tu, den }));
                    self.phien_ve = lay_i64(tin, "phien").unwrap_or(0);
                    self.giu.clear();
                    self.han = None;
                    self.bao(&format!("Đặt vé thành công! {}", tien(lay_i64(tin, "tien").unwrap_or(0))), mau("#c8f0d2"), XANH_DAM);
                    nd.nhat_ky.ghi(&format!("  ĐẶT VÉ THÀNH CÔNG: {} ({rtt:.1} ms)", ds("ma_ve").join(", ")));
                } else {
                    self.bao(&format!("Thanh toán bị từ chối: {ly_do}"), mau("#ff6060"), Color32::WHITE);
                    nd.nhat_ky.ghi(&format!("  thanh toán bị từ chối: {ly_do}"));
                }
            }
            _ => {
                if dung(tin, "ok") {
                    let nha = ds("ghe");
                    for g in &nha {
                        self.giu.remove(g);
                    }
                    if self.giu.is_empty() {
                        self.han = None;
                    }
                    self.bao(&format!("Đã nhả {}", nha.join(", ")), mau("#e8e8e8"), Color32::BLACK);
                    nd.nhat_ky.ghi(&format!("  đã nhả {}", nha.join(", ")));
                } else {
                    nd.nhat_ky.ghi(&format!("  nhả bị từ chối: {ly_do}"));
                }
            }
        }
        self.tt_bat = self.tt_bat(nd);
    }

    fn cap_nhat(&mut self, nd: &mut NguoiDung, tt: &Obj) {
        if self.thong_bao.0 == "Chưa kết nối" {
            self.bao("Chọn đoạn đường, chọn chỗ rồi bấm GIỮ CHỖ", mau("#e8e8e8"), Color32::BLACK);
        }
        let dem = |k: &str| tt.get("dem").and_then(|d| d.get(k)).and_then(Value::as_i64).unwrap_or(0);
        let co_chu = tt.get("chu").and_then(Value::as_object).is_some_and(|c| c.values().any(|v| !v.is_null()));
        if !co_chu && (!self.ve.is_empty() || !self.giu.is_empty()) && dem("ban") == 0 && dem("giu") == 0 {
            self.ve.clear(); // máy chủ đặt lại: xe trống
        }
        let co: Vec<String> = tt.get("ghe").and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default();
        self.chon.retain(|g| co.contains(g)); // đổi loại xe: bỏ chỗ không còn
        if !self.giu.is_empty() {
            let (tu, den) = self.doan_giu;
            let cua_toi: Vec<f64> = self
                .giu
                .iter()
                .flat_map(|g| (tu..den).map(move |k| format!("{g}#{k}")))
                .filter(|o| chu_o(tt, o) == Some(nd.ten.as_str()))
                .filter_map(|o| tt.get("con_lai_ms").and_then(|c| c.get(&o)).and_then(Value::as_f64))
                .collect();
            if let Some(ms) = cua_toi.into_iter().reduce(f64::min) {
                self.han = Some(bay_gio() + ms / 1000.0);
            }
        }
        self.kiem_cho(nd);
        self.tt_bat = self.tt_bat(nd);
    }

    fn moi_nhip(&mut self, nd: &mut NguoiDung) {
        let Some(h) = self.han else {
            self.dong_ho = ("--:--".into(), mau("#909090"));
            return;
        };
        let con = h - bay_gio();
        self.dong_ho = (dong_ho(con), if con < 10.0 { DO_DAM } else { XANH_DAM });
        if con <= 0.0 && self.tt_bat {
            self.bao("Hết thời gian giữ chỗ", mau("#ffd0d0"), DO_DAM);
        }
        self.tt_bat = self.tt_bat(nd);
    }

    fn dong_dinh_ky(&self, nd: &NguoiDung) -> String {
        let tt = nd.tt.clone().unwrap_or_default();
        let d = |k: &str| so_us(tt.get("dem").and_then(|x| x.get(k)));
        let con = self.han.map_or("–".into(), |h| format!("{:.0} s", (h - bay_gio()).max(0.0)));
        let giu = if self.giu.is_empty() { "–".to_string() } else { sap(&self.giu) };
        format!("chỗ-chặng trống {}, giữ {}, bán {}; bạn giữ {giu} (còn {con}), vé {}", d("trong"), d("giu"), d("ban"), self.ve.len())
    }
}

/// dat_ve_xe_gui [tên] [host] [cổng]
pub fn run(args: &[String]) {
    let ten = args.first().cloned().unwrap_or_else(|| "An".into());
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    let cs = CuaSoNguoiDung::<UngDungDatVeXe>::moi(&ten, &host, cong);
    chay_cua_so(&cs.tieu_de(), [700.0, 720.0], cs);
}
