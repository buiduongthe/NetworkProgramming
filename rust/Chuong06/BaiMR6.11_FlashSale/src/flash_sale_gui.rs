//! Bài mở rộng MR6.11 – Trang flash sale (ứng dụng người dùng, egui).
//!
//! Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở gui_chung::CuaSoNguoiDung; tệp này
//! vẽ trang "giờ vàng" như sàn thương mại điện tử: dải FLASH SALE, đồng hồ đếm ngược tới giờ mở bán (chạy
//! cục bộ theo mo_con_ms của cap_nhat), thẻ sản phẩm (ảnh vẽ bằng bút vẽ egui, nhãn giảm giá, giá sale cỡ
//! lớn, giá gốc gạch ngang), thanh tồn kho "Đã bán x%" (cam, "SẮP CHÁY HÀNG" khi > 80 %, "HẾT HÀNG"), chọn
//! số lượng 1–2, nút MUA NGAY bị khóa tới giờ mở bán, đơn hàng của bạn và "vừa mua" trực tiếp.
//!
//! Chạy:  flash_sale_gui <tên> [host] [cổng]       (mặc định 127.0.0.1 44752)

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::CONG_GUI;
use eframe::egui::{self, Color32, Pos2, RichText, Stroke};
use serde_json::{json, Value};

pub const DO_SALE: &str = "#e4002b";
pub const CAM_SALE: &str = "#ff6a00";

/// Ảnh sản phẩm: hộp sạc và đôi tai nghe không dây, tâm (cx, cy) trong khung gốc g (rộng w, cao h), tỉ lệ k.
#[allow(clippy::too_many_arguments)]
pub fn ve_tai_nghe(pt: &egui::Painter, g: Pos2, w: f32, h: f32, cx: f32, cy: f32, k: f32, nen: bool) {
    let q = |x: f32, y: f32| p(g, x, y);
    if nen {
        pt.rect_filled(egui::Rect::from_min_size(g, egui::vec2(w, h)), 0.0, mau("#fff1e6"));
        for (i, m) in ["#ffe0cc", "#ffd1b3", "#ffc299"].iter().enumerate() {
            let r = (95.0 - i as f32 * 22.0) * k;
            let c = q(cx, cy + 10.0 * k);
            // hình tròn bị cắt theo khung (như Canvas của Tk)
            pt.with_clip_rect(egui::Rect::from_min_size(g, egui::vec2(w, h))).circle_filled(c, r, mau(m));
        }
    }
    let (x0, x1, y0, y1) = (cx - 52.0 * k, cx + 52.0 * k, cy + 2.0 * k, cy + 62.0 * k); // hộp sạc bo góc
    let r = 22.0 * k;
    let xy = [x0 + r, y0, x1 - r, y0, x1, y0, x1, y0 + r, x1, y1 - r, x1, y1, x1 - r, y1, x0 + r, y1, x0, y1, x0, y1 - r, x0, y0 + r, x0, y0];
    let diem = |d: f32| -> Vec<Pos2> { xy.chunks(2).map(|c| q(c[0] + d, c[1] + d)).collect() };
    to_da_giac(pt, &duong_tron(&diem(3.0 * k), true), mau("#c9b8a8")); // bóng
    let hop = duong_tron(&diem(0.0), true);
    to_da_giac(pt, &hop, mau("#fafafa"));
    pt.add(egui::Shape::closed_line(hop, Stroke::new(1.0_f32, mau("#9a9a9a"))));
    pt.line_segment([q(x0 + 4.0 * k, y0 + 22.0 * k), q(x1 - 4.0 * k, y0 + 22.0 * k)], Stroke::new(1.0_f32, mau("#b8b8b8")));
    pt.add(egui::Shape::ellipse_filled(q(cx, y0 + 37.0 * k), egui::vec2(3.0 * k, 3.0 * k), mau("#2ecc71")));
    for d in [-1.0f32, 1.0] {
        // hai tai nghe
        let (hx, hy) = (cx + d * 28.0 * k, cy - 42.0 * k);
        let than = duong_tron(&[q(hx - d * 4.0 * k, hy + 10.0 * k), q(hx + d * 6.0 * k, hy + 10.0 * k), q(hx + d * 12.0 * k, hy + 52.0 * k), q(hx + d * 2.0 * k, hy + 54.0 * k)], true);
        to_da_giac(pt, &than, mau("#f4f4f4"));
        pt.add(egui::Shape::closed_line(than, Stroke::new(1.0_f32, mau("#9a9a9a"))));
        pt.add(egui::Shape::ellipse_filled(q(hx, hy + 1.0 * k), egui::vec2(17.0 * k, 16.0 * k), Color32::WHITE));
        pt.add(egui::Shape::ellipse_stroke(q(hx, hy + 1.0 * k), egui::vec2(17.0 * k, 16.0 * k), Stroke::new(1.0_f32, mau("#9a9a9a"))));
        pt.add(egui::Shape::ellipse_filled(q(hx, hy + 1.0 * k), egui::vec2(8.0 * k, 7.0 * k), mau("#3a3a3a")));
        pt.add(egui::Shape::ellipse_filled(q(hx - 1.5 * k, hy - 1.0 * k), egui::vec2(2.5 * k, 2.0 * k), mau("#7a7a7a")));
    }
}

/// Đếm ngược HH:MM:SS (làm tròn lên giây).
pub fn dong_ho(giay: f64) -> String {
    let g = ((giay + 0.999) as i64).max(0);
    format!("{:02}:{:02}:{:02}", g / 3600, g / 60 % 60, g % 60)
}

pub struct TrangFlashSale {
    mo_luc: Option<f64>,                       // thời điểm mở bán theo đồng hồ máy này
    pub don: Vec<(String, i64, i64, String)>,  // đơn của tôi: (mã, số lượng, thành tiền, giờ)
    pub dang_cho: bool,
    pub sl: i64,
    pub thong_bao: (String, Color32, Color32),
}

impl TrangFlashSale {
    fn bao(&mut self, text: &str, nen: Color32, chu: Color32) {
        self.thong_bao = (text.to_string(), nen, chu);
    }

    fn da_mo(&self) -> bool {
        self.mo_luc.is_some_and(|m| bay_gio() >= m)
    }

    /// Nút MUA NGAY: (chữ, bấm được, màu nền) – như _nut() của bản Tk.
    pub fn nut_mua(&self, nd: &NguoiDung) -> (&'static str, bool, Color32) {
        if self.dang_cho {
            return ("ĐANG XỬ LÝ…", false, mau(CAM_SALE));
        }
        match (&nd.kn, &nd.tt) {
            (Some(_), Some(tt)) if lay_i64(tt, "ton").unwrap_or(0) <= 0 => ("HẾT HÀNG", false, mau("#909090")),
            (Some(_), Some(_)) if !self.da_mo() => ("CHỜ MỞ BÁN", false, mau("#f5b48a")),
            (Some(_), Some(_)) => ("MUA NGAY", true, mau(CAM_SALE)),
            _ => ("CHỜ MỞ BÁN", false, mau("#c8c8c8")),
        }
    }

    /// Chữ trạng thái và đồng hồ trên dải FLASH SALE: (nhãn, đồng hồ, nền đồng hồ).
    pub fn dong_ho(&self, nd: &NguoiDung) -> (String, String, Color32) {
        let (Some(tt), Some(m)) = (&nd.tt, self.mo_luc) else { return ("Bắt đầu sau".into(), "--:--:--".into(), mau("#1a1a1a")) };
        let con = m - bay_gio();
        if con > 0.0 {
            ("Bắt đầu sau".into(), dong_ho(con), mau("#1a1a1a"))
        } else if lay_i64(tt, "ton").unwrap_or(0) <= 0 {
            ("Đã kết thúc".into(), "CHÁY HÀNG".into(), mau("#404040"))
        } else {
            (String::new(), "ĐANG DIỄN RA".into(), DO_DAM)
        }
    }

    /// Bấm MUA NGAY với số lượng sl (None: số đang chọn).
    pub fn mua(&mut self, nd: &mut NguoiDung, sl: Option<i64>) {
        if nd.kn.is_none() || self.dang_cho {
            return;
        }
        if let Some(s) = sl {
            self.sl = s;
        }
        self.dang_cho = true;
        nd.nhat_ky.ghi(&format!("MUA NGAY {} sản phẩm", self.sl));
        nd.gui_yc("mua", obj(json!({"so_luong": self.sl})));
    }
}

impl UngDung for TrangFlashSale {
    const TIEU_DE: &'static str = "Flash sale";

    fn moi(_ten: &str) -> Self {
        TrangFlashSale { mo_luc: None, don: Vec::new(), dang_cho: false, sl: 1, thong_bao: ("Chưa kết nối".into(), Color32::from_gray(0xe0), Color32::BLACK) }
    }

    fn ve(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung) {
        let tt = nd.tt.clone();
        let rong = 640.0;
        // ---- dải FLASH SALE
        let (nhan, dh, nen_dh) = self.dong_ho(nd);
        egui::Frame::none().fill(mau(DO_SALE)).inner_margin(egui::Margin::symmetric(10.0, 4.0)).show(ui, |ui| {
            ui.set_width(rong - 20.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("FLASH SALE").strong().italics().size(25.0).color(Color32::WHITE));
                ui.label(RichText::new("GIỜ VÀNG\nGIÁ SỐC").strong().size(11.0).color(mau("#ffe066")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::Frame::none().fill(nen_dh).inner_margin(egui::Margin::symmetric(6.0, 2.0)).show(ui, |ui| {
                        ui.label(RichText::new(dh).monospace().strong().size(19.0).color(Color32::WHITE));
                    });
                    ui.label(RichText::new(nhan).strong().size(12.5).color(Color32::WHITE));
                });
            });
        });
        // ---- thẻ sản phẩm
        let mut bam = false;
        egui::Frame::none().fill(Color32::WHITE).stroke(Stroke::new(1.0_f32, mau("#d0d0d0"))).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
            ui.set_width(rong - 12.0);
            ui.horizontal_top(|ui| {
                let (r, pt) = ui.allocate_painter(egui::vec2(230.0, 210.0), egui::Sense::hover());
                let g = r.rect.min;
                ve_tai_nghe(&pt, g, 230.0, 210.0, 115.0, 92.0, 1.0, true);
                if let Some(tt) = &tt {
                    let sp = tt.get("sp").and_then(Value::as_object).cloned().unwrap_or_default();
                    let (gs, gg) = (lay_f64(&sp, "gia_sale").unwrap_or(1.0), lay_f64(&sp, "gia_goc").unwrap_or(1.0));
                    let giam = round_py(100.0 * (1.0 - gs / gg));
                    pt.circle(p(g, 195.0, 35.0), 27.0, mau(DO_SALE), Stroke::new(2.0_f32, Color32::WHITE));
                    pt.text(p(g, 195.0, 35.0), egui::Align2::CENTER_CENTER, format!("-{giam}%"), egui::FontId::proportional(15.0), Color32::WHITE);
                    if lay_i64(tt, "ton").unwrap_or(0) <= 0 {
                        pt.rect_filled(egui::Rect::from_min_max(p(g, 0.0, 170.0), p(g, 230.0, 205.0)), 0.0, mau("#404040"));
                        pt.text(p(g, 115.0, 187.0), egui::Align2::CENTER_CENTER, "HẾT HÀNG", egui::FontId::proportional(18.0), Color32::WHITE);
                    }
                }
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    let sp = tt.as_ref().and_then(|t| t.get("sp")).and_then(Value::as_object).cloned().unwrap_or_default();
                    ui.add(egui::Label::new(RichText::new(lay_str(&sp, "ten").unwrap_or("–")).strong().size(15.0)).wrap());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(lay_i64(&sp, "gia_sale").map_or("–".into(), tien)).strong().size(27.0).color(mau(DO_SALE)));
                        if let Some(gg) = lay_i64(&sp, "gia_goc") {
                            ui.label(RichText::new(tien(gg)).size(13.5).strikethrough().color(mau("#909090")));
                        }
                    });
                    if let Some(tt) = &tt {
                        ui.label(RichText::new(format!("Mỗi đơn tối đa {} sản phẩm · Số lượng có hạn: {}", so_us(sp.get("toi_da")), so_us(tt.get("ton_dau")))).size(11.0).color(mau("#606060")));
                        // thanh tồn kho
                        let (ton_dau, ton) = (lay_i64(tt, "ton_dau").unwrap_or(1).max(1), lay_i64(tt, "ton").unwrap_or(0));
                        let ti_le = ((ton_dau - ton) as f32 / ton_dau as f32).min(1.0);
                        let (r, pt) = ui.allocate_painter(egui::vec2(300.0, 24.0), egui::Sense::hover());
                        let b = egui::Rect::from_min_size(r.rect.min + egui::vec2(0.0, 2.0), egui::vec2(300.0, 20.0));
                        pt.rect_filled(b, 0.0, mau("#ffd6c2"));
                        pt.rect_filled(egui::Rect::from_min_size(b.min, egui::vec2(300.0 * ti_le, 20.0)), 0.0, mau(if ti_le >= 0.8 { DO_SALE } else { CAM_SALE }));
                        let chu = if ton <= 0 { "HẾT HÀNG".to_string() } else if ti_le >= 0.8 { format!("SẮP CHÁY HÀNG – còn {ton}") } else { format!("ĐÃ BÁN {}%", round_py(ti_le as f64 * 100.0)) };
                        pt.text(b.center(), egui::Align2::CENTER_CENTER, chu, egui::FontId::proportional(13.0), if ti_le > 0.45 { Color32::WHITE } else { mau("#a03000") });
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Số lượng:");
                        for n in [1, 2] {
                            if ui.radio(self.sl == n, n.to_string()).clicked() {
                                self.sl = n;
                            }
                        }
                    });
                    ui.add_space(4.0);
                    let (chu, bat, m) = self.nut_mua(nd);
                    if nut_chinh(ui, chu, m, bat, 220.0, 38.0) {
                        bam = true;
                    }
                });
            });
        });
        if bam {
            self.mua(nd, None);
        }
        let (text, nen, mc) = self.thong_bao.clone();
        egui::Frame::none().fill(nen).inner_margin(egui::Margin::symmetric(6.0, 5.0)).show(ui, |ui| {
            ui.set_width(rong - 12.0);
            ui.vertical_centered(|ui| ui.label(RichText::new(text).strong().size(14.0).color(mc)));
        });
        ui.horizontal_top(|ui| {
            nhom(ui, "Đơn hàng của bạn", |ui| {
                let dong: Vec<(String, Color32)> = self.don.iter().rev().map(|(ma, sl, tt, g)| (format!("{g} {ma} ×{sl} {:>11}", tien(*tt)), Color32::BLACK)).collect();
                danh_sach(ui, &dong, 4, 330.0, 11.0);
            });
            nhom(ui, "Vừa mua (trực tiếp)", |ui| {
                let dong: Vec<(String, Color32)> = tt
                    .as_ref()
                    .and_then(|t| t.get("don_gan"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|x| {
                        let ng = x.get("nguoi").map(py_str).unwrap_or_default();
                        (format!("{} {:<6} ×{}", so_us(x.get("gio")), ng, so_us(x.get("so_luong"))), if ng == nd.ten { mau("#0040c0") } else { Color32::BLACK })
                    })
                    .collect();
                danh_sach(ui, &dong, 4, ui.available_width().max(200.0), 11.0);
            });
        });
    }

    fn ket_qua(&mut self, nd: &mut NguoiDung, _loai: &str, _truong: &Obj, tin: &Obj, rtt: f64) {
        self.dang_cho = false;
        if dung(tin, "ok") {
            let (ma, sl, tt) = (so_us(tin.get("ma_don")), lay_i64(tin, "so_luong").unwrap_or(0), lay_i64(tin, "thanh_tien").unwrap_or(0));
            self.don.push((ma.clone(), sl, tt, gio_hien()));
            self.bao(&format!("Đặt hàng thành công! Đơn {ma} – {sl} sp"), mau("#70d070"), Color32::BLACK);
            nd.nhat_ky.ghi(&format!("  đơn {ma}, {} ({rtt:.1} ms)", tien(tt)));
        } else {
            let m = lay_str(tin, "ma_loi").unwrap_or("");
            let text = match m {
                "chua_mo" => "Chưa tới giờ mở bán!".to_string(),
                "het_hang" => "Rất tiếc, sản phẩm đã HẾT HÀNG".to_string(),
                "khong_du" => format!("Chỉ còn {} sản phẩm – giảm số lượng", tin.get("ton").map(py_str).unwrap_or_else(|| "None".into())),
                "xung_dot" => "Hệ thống đang bận, vui lòng thử lại".to_string(),
                _ => lay_str(tin, "ly_do").unwrap_or("").to_string(),
            };
            if m == "het_hang" {
                self.bao(&text, mau("#ff6060"), Color32::WHITE);
            } else {
                self.bao(&text, mau("#ffb347"), Color32::BLACK);
            }
            nd.nhat_ky.ghi(&format!("  từ chối: {}", tin.get("ly_do").map(py_str).unwrap_or_else(|| "None".into())));
        }
    }

    fn cap_nhat(&mut self, _nd: &mut NguoiDung, tt: &Obj) {
        self.mo_luc = Some(bay_gio() + lay_f64(tt, "mo_con_ms").unwrap_or(0.0) / 1000.0);
        if self.thong_bao.0 == "Chưa kết nối" {
            self.bao("Đón giờ vàng – nút MUA NGAY mở khi hết đếm ngược", Color32::from_gray(0xe0), Color32::BLACK);
        }
    }

    fn su_kien(&mut self, nd: &mut NguoiDung, text: &str) {
        nd.nhat_ky.ghi(&format!("* {text}"));
        if text.starts_with("Đặt lại") {
            // đợt sale mới
            self.don.clear();
            self.bao("Đợt sale mới – chờ giờ mở bán", Color32::from_gray(0xe0), Color32::BLACK);
        }
    }

    fn mat_ket_noi(&mut self, _nd: &mut NguoiDung) {
        self.dang_cho = false;
    }

    fn dong_dinh_ky(&self, nd: &NguoiDung) -> String {
        let tt = nd.tt.clone().unwrap_or_default();
        let con = (self.mo_luc.unwrap_or(0.0) - bay_gio()).max(0.0);
        let mo = if con > 0.0 { format!("mở sau {con:.0} s") } else { "đang bán".into() };
        format!("tồn {}/{}, {mo}; bạn có {} đơn", so_us(tt.get("ton")), so_us(tt.get("ton_dau")), self.don.len())
    }
}

/// flash_sale_gui [tên] [host] [cổng]
pub fn run(args: &[String]) {
    let ten = args.first().cloned().unwrap_or_else(|| "An".into());
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    let cs = CuaSoNguoiDung::<TrangFlashSale>::moi(&ten, &host, cong);
    chay_cua_so(&cs.tieu_de(), [700.0, 680.0], cs);
}
