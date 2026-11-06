//! Bài mở rộng MR6.8 – Ứng dụng đặt vé xem phim (người dùng, egui).
//!
//! Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở gui_chung::CuaSoNguoiDung; tệp này
//! vẽ ứng dụng đặt vé: áp phích phim, sơ đồ phòng chiếu 8 × 12 ghế với MÀN HÌNH, lối đi, nhãn hàng A–H;
//! màu ghế: trống / đang chọn / người khác giữ / đã bán / bạn giữ / vé của bạn. Bấm ghế để chọn, "GIỮ
//! CHỖ" gửi giu_cho, đồng hồ giữ chỗ đếm ngược (đỏ khi < 10 s), "THANH TOÁN" và "Nhả ghế". Sơ đồ cập nhật
//! trực tiếp từ cap_nhat (≤ 10 lần/giây) khi người khác giữ/mua ghế; ghế đang chọn bị người khác giữ thì
//! tự bỏ chọn và báo.
//!
//! Chạy:  dat_ve_gui <tên> [host] [cổng]       (mặc định 127.0.0.1 44722)

use crate::chung::*;
use crate::dat_ve::chu_ghe;
use crate::gui_chung::*;
use crate::may_chu::CONG_GUI;
use eframe::egui::{self, Color32, Pos2, RichText};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

pub const TRONG: Color32 = Color32::WHITE;
pub const CHON: Color32 = Color32::from_rgb(0xb9, 0xf6, 0xca);
pub const KHAC_GIU: Color32 = Color32::from_rgb(0xff, 0xa7, 0x26);
pub const DA_BAN: Color32 = Color32::from_rgb(0x9e, 0x9e, 0x9e);
pub const TOI_GIU: Color32 = Color32::from_rgb(0x2e, 0x7d, 0x32);
pub const VE_TOI: Color32 = Color32::from_rgb(0x15, 0x65, 0xc0);
pub const DO_RAP: Color32 = Color32::from_rgb(0xe7, 0x1a, 0x0f);
const CHU_GIAI: [(Color32, &str); 6] = [(TRONG, "Trống"), (CHON, "Đang chọn"), (KHAC_GIU, "Người khác giữ"), (DA_BAN, "Đã bán"), (TOI_GIU, "Bạn giữ"), (VE_TOI, "Vé của bạn")];

pub fn dong_ho(giay: f64) -> String {
    let giay = (giay + 0.999).max(0.0) as i64;
    format!("{:02}:{:02}", giay / 60, giay % 60)
}

/// Sơ đồ ghế; dùng ở ứng dụng đặt vé (ô = 30 px) và bảng điều khiển máy chủ (ô nhỏ).
pub struct SoDoGhe {
    hang: Vec<char>,
    le: f32,
    tren: f32,
    chu: bool,
    pub rong: f32,
    pub cao: f32,
    vi_tri: Vec<(String, egui::Rect)>, // tọa độ cục bộ
}

impl SoDoGhe {
    pub fn new(hang: &str, cot: usize, o: f32, khe: f32, le: f32, tren: f32, chu: bool) -> SoDoGhe {
        let loi_di = [2usize, 10];
        let hang: Vec<char> = hang.chars().collect();
        let rong = le * 2.0 + cot as f32 * (o + khe) + loi_di.len() as f32 * o * 0.5;
        let cao = tren + hang.len() as f32 * (o * 0.86 + khe) + 4.0;
        let mut vi_tri = Vec::new();
        for (i, r) in hang.iter().enumerate() {
            let y = tren + i as f32 * (o * 0.86 + khe);
            for c in 1..=cot {
                let x = le + (c - 1) as f32 * (o + khe) + loi_di.iter().filter(|&&a| c > a).count() as f32 * o * 0.5;
                vi_tri.push((format!("{r}{c}"), egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(o, o * 0.86))));
            }
        }
        SoDoGhe { hang, le, tren, chu, rong, cao, vi_tri }
    }

    /// Vẽ nền (màn hình, nhãn hàng) và các ghế: mau_ghe {ghế: màu}; vien {ghế: màu viền} (ghế nóng).
    pub fn ve(&self, pt: &egui::Painter, g: Pos2, mau_ghe: &HashMap<String, Color32>, vien: &HashMap<String, Color32>) {
        let w = self.rong;
        let lech = self.tren * 0.55;
        let man = duong_tron(&[p(g, self.le, lech), p(g, w * 0.5, lech * 0.35), p(g, w - self.le, lech)], false);
        pt.add(egui::Shape::line(man, egui::Stroke::new(if self.chu { 4.0_f32 } else { 2.0 }, mau("#e8e8f0"))));
        if self.chu {
            pt.text(p(g, w / 2.0, lech + 10.0), egui::Align2::CENTER_CENTER, "MÀN HÌNH", egui::FontId::proportional(10.5), mau("#b0b0c0"));
            for r in &self.hang {
                let o = self.vi_tri.iter().find(|(k, _)| *k == format!("{r}1")).unwrap().1;
                for x in [self.le / 2.0, w - self.le / 2.0] {
                    pt.text(p(g, x, o.center().y), egui::Align2::CENTER_CENTER, r.to_string(), egui::FontId::proportional(12.0), mau("#c0c0d0"));
                }
            }
        }
        for (ma, o) in &self.vi_tri {
            let r = o.translate(g.to_vec2());
            let m = mau_ghe.get(ma).copied().unwrap_or(TRONG);
            pt.rect_filled(r, 0.0, m);
            let (v, day) = match vien.get(ma) {
                Some(c) => (*c, 2.0_f32),
                None => (mau("#606070"), 1.0),
            };
            pt.rect_stroke(r, 0.0, egui::Stroke::new(day, v));
            if self.chu {
                // tay vịn: vạch tối ở chân ghế
                pt.rect_filled(egui::Rect::from_min_max(egui::pos2(r.min.x + 2.0, r.max.y - 4.0), egui::pos2(r.max.x - 2.0, r.max.y - 1.0)), 0.0, mau("#707080"));
                let toi = m == TOI_GIU || m == VE_TOI || m == DA_BAN;
                pt.text(egui::pos2(r.center().x, r.center().y - 2.0), egui::Align2::CENTER_CENTER, &ma[1..], egui::FontId::proportional(9.5), if toi { Color32::WHITE } else { mau("#303040") });
                if m == DA_BAN {
                    pt.line_segment([egui::pos2(r.min.x + 4.0, r.min.y + 3.0), egui::pos2(r.max.x - 4.0, r.max.y - 6.0)], egui::Stroke::new(1.0_f32, mau("#e0e0e0")));
                }
            }
        }
    }

    pub fn ghe_tai(&self, x: f32, y: f32) -> Option<String> {
        self.vi_tri.iter().find(|(_, r)| r.contains(egui::pos2(x, y))).map(|(g, _)| g.clone())
    }
}

/// Màu ghế theo cap_nhat (của mình / người khác, giữ / bán).
pub fn mau_theo_tt(tt: &Obj, ten: Option<&str>) -> HashMap<String, Color32> {
    let mut m = HashMap::new();
    for (r, dong) in lay_str(tt, "hang").unwrap_or("").chars().zip(tt.get("so_do").and_then(Value::as_array).into_iter().flatten()) {
        for (c, x) in dong.as_str().unwrap_or("").chars().enumerate() {
            let g = format!("{r}{}", c + 1);
            let cua_toi = ten.is_some() && chu_ghe(tt, &g) == ten;
            match x {
                'b' => m.insert(g, if cua_toi { VE_TOI } else { DA_BAN }),
                'g' => m.insert(g, if cua_toi { TOI_GIU } else { KHAC_GIU }),
                _ => None,
            };
        }
    }
    m
}

fn sap(ds: &BTreeSet<String>) -> String {
    ds.iter().cloned().collect::<Vec<_>>().join(", ")
}

pub struct UngDungDatVe {
    so_do: SoDoGhe,
    pub chon: BTreeSet<String>, // ghế đang chọn (chưa giữ)
    pub giu: BTreeSet<String>,  // ghế mình đang giữ (theo kq)
    phien_giu: i64,             // số thứ tự lần ghi (kq) của lần giữ chỗ / thanh toán gần nhất
    phien_ve: i64,
    pub ve: BTreeSet<String>, // vé của mình
    han: Option<f64>,         // thời điểm (bay_gio) hết hạn giữ chỗ
    pub so_giu: u32,
    so_ve: usize,
    pub thong_bao: (String, Color32, Color32),
    pub dong_ho: (String, Color32),
    tt_bat: bool,
}

impl UngDungDatVe {
    fn bao(&mut self, text: &str, nen: Color32, chu: Color32) {
        self.thong_bao = (text.to_string(), nen, chu);
    }

    fn gia(nd: &NguoiDung) -> i64 {
        nd.tt.as_ref().and_then(|t| lay_i64(t, "gia_ve")).unwrap_or(0)
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

    /// 'trong' | 'giu' (người khác) | 'ban' | 'toi_giu' | 've' theo cap_nhat mới nhất.
    fn trang_ghe(&self, nd: &NguoiDung, g: &str) -> &'static str {
        let Some(tt) = &nd.tt else { return "trong" };
        let hang = lay_str(tt, "hang").unwrap_or("");
        let (r, c) = (g.chars().next().unwrap_or('A'), g[1..].parse::<usize>().unwrap_or(1));
        let x = hang.chars().position(|h| h == r).and_then(|i| tt.get("so_do")?.get(i)?.as_str()?.chars().nth(c - 1)).unwrap_or('.');
        let cua_toi = chu_ghe(tt, g) == Some(nd.ten.as_str());
        match (x, cua_toi) {
            ('g', true) => "toi_giu",
            ('g', false) => "giu",
            ('b', true) => "ve",
            ('b', false) => "ban",
            _ => "trong",
        }
    }

    /// Bấm một ghế (người dùng hoặc kịch bản demo).
    pub fn bam(&mut self, nd: &NguoiDung, g: &str) {
        if nd.kn.is_none() {
            return;
        }
        let trang = self.trang_ghe(nd, g);
        if self.chon.contains(g) {
            self.chon.remove(g);
        } else if trang == "trong" && !self.giu.contains(g) {
            if self.chon.len() + self.giu.len() >= 8 {
                self.bao("Tối đa 8 ghế mỗi lần", mau("#ffe0b2"), Color32::BLACK);
                return;
            }
            self.chon.insert(g.to_string());
        } else if trang == "giu" {
            self.bao(&format!("Ghế {g} đang có người giữ"), mau("#ffe0b2"), Color32::BLACK);
        } else if trang == "ban" {
            self.bao(&format!("Ghế {g} đã bán"), mau("#e0e0e0"), Color32::BLACK);
        }
        self.tt_bat = self.tt_bat(nd);
    }

    /// Bấm "GIỮ CHỖ" (nút chỉ bấm được khi đã chọn ghế và chưa giữ).
    pub fn giu_cho(&mut self, nd: &mut NguoiDung) {
        if !self.giu_bat(nd) {
            return;
        }
        let ghe: Vec<String> = self.chon.iter().cloned().collect();
        nd.nhat_ky.ghi(&format!("Giữ chỗ {}…", ghe.join(", ")));
        self.bao("Đang giữ chỗ…", mau("#fff3c4"), Color32::BLACK);
        nd.gui_yc("giu_cho", obj(json!({"ghe": ghe})));
    }

    /// Bấm "THANH TOÁN" (bấm được khi đang giữ, còn hạn).
    pub fn thanh_toan(&mut self, nd: &mut NguoiDung) {
        if self.tt_bat(nd) {
            nd.nhat_ky.ghi(&format!("Thanh toán {}…", sap(&self.giu)));
            nd.gui_yc("thanh_toan", obj(json!({"ghe": self.giu.iter().collect::<Vec<_>>()})));
        }
    }

    pub fn nha(&mut self, nd: &mut NguoiDung) {
        if !self.giu.is_empty() {
            nd.gui_yc("nha", obj(json!({"ghe": self.giu.iter().collect::<Vec<_>>()})));
        }
    }

    /// Ghế mình giữ/mua bị người khác lấy (tranh chấp) hoặc hết hạn; ghế đang chọn bị người khác giữ.
    /// Chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq giữ chỗ / thanh toán).
    fn kiem_ghe(&mut self, nd: &mut NguoiDung) {
        let Some(tt) = nd.tt.clone() else { return };
        let phien = lay_i64(&tt, "phien").unwrap_or(0);
        if phien >= self.phien_giu {
            for g in self.giu.clone() {
                match chu_ghe(&tt, &g) {
                    Some(ai) if ai == nd.ten => continue,
                    Some(ai) => {
                        self.bao(&format!("Ghế {g} đã bị {ai} lấy mất!"), mau("#ff6060"), Color32::WHITE);
                        nd.nhat_ky.ghi(&format!("Ghế {g} đã bị {ai} lấy mất!"));
                    }
                    None => {
                        self.bao("Hết thời gian giữ chỗ – ghế đã được nhả", mau("#ffd0d0"), DO_DAM);
                        nd.nhat_ky.ghi(&format!("Hết thời gian giữ chỗ: {g}"));
                    }
                }
                self.giu.remove(&g);
            }
            if self.giu.is_empty() {
                self.han = None;
            }
        }
        if phien >= self.phien_ve {
            for g in self.ve.clone() {
                let ai = chu_ghe(&tt, &g);
                if ai != Some(nd.ten.as_str()) {
                    self.ve.remove(&g);
                    let ai = ai.unwrap_or("không ai");
                    self.bao(&format!("Vé {g} bị ghi cho {ai}!"), mau("#ff6060"), Color32::WHITE);
                    nd.nhat_ky.ghi(&format!("Vé {g} bị ghi cho {ai}!"));
                }
            }
        }
        let bi_lay: BTreeSet<String> = self.chon.iter().filter(|g| self.trang_ghe(nd, g) != "trong").cloned().collect();
        if !bi_lay.is_empty() {
            for g in &bi_lay {
                self.chon.remove(g);
            }
            self.bao(&format!("Ghế {} vừa có người giữ", sap(&bi_lay)), mau("#ffe0b2"), Color32::BLACK);
            nd.nhat_ky.ghi(&format!("Ghế {} vừa có người giữ", sap(&bi_lay)));
        }
    }

    fn ve_phim(&self, pt: &egui::Painter, g: Pos2, tt: Option<&Obj>) {
        for i in 0..12 {
            // nền chuyển màu tím -> xanh
            let (r, gg, b) = (43 + i * 2, 22 + i * 6, 64 + i * 9);
            pt.rect_filled(egui::Rect::from_min_size(p(g, 0.0, i as f32 * 8.0), egui::vec2(250.0, 8.0)), 0.0, Color32::from_rgb(r, gg, b));
        }
        pt.circle(p(g, 219.0, 31.0), 23.0, mau("#4fc3f7"), egui::Stroke::new(2.0_f32, mau("#b3e5fc")));
        // lục địa: hình quạt 200°–340° (phía dưới) tô xanh lá
        let mut qua = vec![p(g, 219.0, 31.0)];
        qua.extend((0..=14).map(|k| {
            let a = (200.0 + 10.0 * k as f32).to_radians();
            p(g, 219.0 + 23.0 * a.cos(), 31.0 - 23.0 * a.sin())
        }));
        pt.add(egui::Shape::convex_polygon(qua, mau("#43a047"), egui::Stroke::NONE));
        for k in 0..14 {
            let (x, y) = ((k * 37) % 170 + 6, (k * 23) % 60 + 6);
            pt.circle_filled(p(g, x as f32 + 1.0, y as f32 + 1.0), 1.0, Color32::WHITE);
        }
        let Some(tt) = tt else { return };
        pt.text(p(g, 10.0, 40.0), egui::Align2::LEFT_CENTER, so_us(tt.get("phim")).to_uppercase(), egui::FontId::proportional(16.0), Color32::WHITE);
        pt.text(p(g, 10.0, 66.0), egui::Align2::LEFT_CENTER, format!("2D Phụ đề · Suất {} · Phòng 3", so_us(tt.get("suat"))), egui::FontId::proportional(10.5), mau("#e0d8f0"));
        pt.text(p(g, 10.0, 82.0), egui::Align2::LEFT_CENTER, format!("{}/ghế · giữ chỗ {} s", tien(lay_i64(tt, "gia_ve").unwrap_or(0)), fmt_g(lay_f64(tt, "ttl_s").unwrap_or(0.0))), egui::FontId::proportional(10.5), mau("#ffd54f"));
    }
}

impl UngDung for UngDungDatVe {
    const TIEU_DE: &'static str = "Đặt vé xem phim";

    fn moi(_ten: &str) -> Self {
        UngDungDatVe {
            so_do: SoDoGhe::new("ABCDEFGH", 12, 30.0, 4.0, 24.0, 46.0, true),
            chon: BTreeSet::new(),
            giu: BTreeSet::new(),
            phien_giu: 0,
            phien_ve: 0,
            ve: BTreeSet::new(),
            han: None,
            so_giu: 0,
            so_ve: 0,
            thong_bao: ("Chưa kết nối".into(), mau("#e8e8e8"), Color32::BLACK),
            dong_ho: ("--:--".into(), mau("#909090")),
            tt_bat: false,
        }
    }

    fn ve(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung) {
        let gia = Self::gia(nd);
        ui.horizontal_top(|ui| {
            let (r, pt) = ui.allocate_painter(egui::vec2(486.0, 322.0), egui::Sense::click());
            pt.rect_filled(r.rect, 0.0, mau("#1c1c24"));
            let mut m = nd.tt.as_ref().map(|t| mau_theo_tt(t, Some(nd.ten.as_str()))).unwrap_or_default();
            for g in &self.chon {
                m.entry(g.clone()).or_insert(CHON);
            }
            self.so_do.ve(&pt, r.rect.min, &m, &HashMap::new());
            let (mut x, y) = (10.0, self.so_do.cao + 14.0);
            for (c, chu) in CHU_GIAI {
                // chú giải màu
                let o = egui::Rect::from_min_size(p(r.rect.min, x, y), egui::vec2(14.0, 12.0));
                pt.rect_filled(o, 0.0, c);
                pt.rect_stroke(o, 0.0, egui::Stroke::new(1.0_f32, mau("#606070")));
                pt.text(p(r.rect.min, x + 18.0, y + 6.0), egui::Align2::LEFT_CENTER, chu, egui::FontId::proportional(10.0), mau("#d0d0e0"));
                x += 22.0 + chu.chars().count() as f32 * 6.0;
            }
            if r.clicked() {
                if let Some(pos) = r.interact_pointer_pos() {
                    if let Some(g) = self.so_do.ghe_tai(pos.x - r.rect.min.x, pos.y - r.rect.min.y) {
                        self.bam(nd, &g);
                    }
                }
            }
            egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, Color32::BLACK)).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(252.0);
                    let (r, pt) = ui.allocate_painter(egui::vec2(250.0, 96.0), egui::Sense::hover());
                    self.ve_phim(&pt, r.rect.min, nd.tt.as_ref());
                    let (ghe, tam) = if self.giu.is_empty() {
                        (format!("Ghế đã chọn: {}", if self.chon.is_empty() { "–".into() } else { sap(&self.chon) }), format!("Tạm tính: {}", tien(gia * self.chon.len() as i64)))
                    } else {
                        (format!("Đang giữ: {}", sap(&self.giu)), format!("Tổng: {}", tien(gia * self.giu.len() as i64)))
                    };
                    ui.label(RichText::new(ghe).strong().size(13.0));
                    ui.label(tam);
                    let gb = self.giu_bat(nd);
                    if nut_chinh(ui, "GIỮ CHỖ", if gb { mau("#ff8f00") } else { mau("#ffcc80") }, gb, 250.0, 30.0) {
                        self.giu_cho(nd);
                    }
                    ui.horizontal(|ui| {
                        ui.label("Giữ chỗ còn");
                        ui.label(RichText::new(&self.dong_ho.0).monospace().strong().size(22.0).color(self.dong_ho.1));
                    });
                    let tb = self.tt_bat(nd);
                    let chu_tt = if self.giu.is_empty() { "THANH TOÁN".to_string() } else { format!("THANH TOÁN {}", tien(gia * self.giu.len() as i64)) };
                    if nut_chinh(ui, &chu_tt, if tb { DO_RAP } else { mau("#ef9a9a") }, tb, 250.0, 30.0) {
                        self.thanh_toan(nd);
                    }
                    if nut(ui, "Nhả ghế", mau("#f0f0f0"), nd.kn.is_some() && !self.giu.is_empty(), 70.0) {
                        self.nha(nd);
                    }
                    nhan_nen(ui, &self.thong_bao.0.clone(), self.thong_bao.1, self.thong_bao.2, 12.0, 238.0);
                    let ve = format!("Vé của bạn: {}\nBạn: giữ được {} lần, mua {} vé", if self.ve.is_empty() { "–".into() } else { sap(&self.ve) }, self.so_giu, self.so_ve);
                    ui.add(egui::Label::new(RichText::new(ve).size(12.0).color(VE_TOI)).wrap());
                });
            });
        });
    }

    fn ket_qua(&mut self, nd: &mut NguoiDung, loai: &str, truong: &Obj, tin: &Obj, rtt: f64) {
        let ghe: Vec<String> = truong.get("ghe").and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default();
        let ds = |k: &str| -> Vec<String> { tin.get(k).and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default() };
        let ly_do = so_us(tin.get("ly_do"));
        match loai {
            "giu_cho" => {
                if dung(tin, "ok") {
                    self.so_giu += 1;
                    self.giu = ds("ghe").into_iter().collect();
                    self.phien_giu = lay_i64(tin, "phien").unwrap_or(0);
                    for g in &self.giu {
                        self.chon.remove(g);
                    }
                    let ms = lay_f64(tin, "het_han_ms").unwrap_or(0.0);
                    self.han = Some(bay_gio() + ms / 1000.0);
                    self.bao(&format!("Đã giữ {} – thanh toán trong {:.0} s", ghe.join(", "), ms / 1000.0), mau("#c8f0d2"), XANH_DAM);
                    nd.nhat_ky.ghi(&format!("  ĐÃ GIỮ {} ({rtt:.1} ms)", ghe.join(", ")));
                    self.kiem_ghe(nd);
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
                    for g in &ghe {
                        self.ve.insert(g.clone());
                        self.giu.remove(g);
                    }
                    self.phien_ve = lay_i64(tin, "phien").unwrap_or(0);
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
            self.bao("Chọn ghế rồi bấm GIỮ CHỖ", mau("#e8e8e8"), Color32::BLACK);
        }
        let dem = |k: &str| tt.get("dem").and_then(|d| d.get(k)).and_then(Value::as_i64).unwrap_or(0);
        let co_chu = tt.get("chu").and_then(Value::as_object).is_some_and(|c| c.values().any(|v| !v.is_null()));
        if !co_chu && (!self.ve.is_empty() || !self.giu.is_empty()) && dem("ban") == 0 && dem("giu") == 0 {
            self.ve.clear(); // máy chủ đặt lại: rạp trống
        }
        let cua_toi: Vec<f64> = self.giu.iter().filter(|g| chu_ghe(tt, g) == Some(nd.ten.as_str())).filter_map(|g| tt.get("con_lai_ms").and_then(|c| c.get(g)).and_then(Value::as_f64)).collect();
        if let Some(ms) = cua_toi.into_iter().reduce(f64::min) {
            self.han = Some(bay_gio() + ms / 1000.0);
        }
        self.kiem_ghe(nd);
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
        let hoac = |s: &BTreeSet<String>| if s.is_empty() { "–".to_string() } else { sap(s) };
        format!("trống {}, giữ {}, bán {}; bạn giữ {} (còn {con}), vé {}", d("trong"), d("giu"), d("ban"), hoac(&self.giu), hoac(&self.ve))
    }
}

/// dat_ve_gui [tên] [host] [cổng]
pub fn run(args: &[String]) {
    let ten = args.first().cloned().unwrap_or_else(|| "An".into());
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    let cs = CuaSoNguoiDung::<UngDungDatVe>::moi(&ten, &host, cong);
    chay_cua_so(&cs.tieu_de(), [800.0, 560.0], cs);
}
