//! Bài mở rộng MR6.5 – Thành phần giao diện (egui) dùng chung cho server_gui, phat_gui, xem_gui.
//!
//! Quy tắc (như MR6.2, MR6.3): egui là "immediate mode" – mỗi lần vẽ lại, hàm `ui` dựng lại toàn bộ
//! cửa sổ từ trạng thái hiện tại; CHỈ luồng giao diện gọi `ui` và chạm vào điều khiển. Luồng mạng/âm
//! thanh/hiển thị của lõi chỉ ghi số liệu vào đối tượng lõi (trong Mutex) hoặc thêm dòng vào NhatKy;
//! giao diện đọc định kỳ (request_repaint_after) rồi vẽ lại. Ảnh: khung RGB thô -> egui::ColorImage ->
//! texture (TextureHandle::set mỗi khung mới).

use eframe::egui::{self, Color32, RichText};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const NEN: Color32 = Color32::from_rgb(0xb0, 0xc4, 0xde);
pub const XANH_LA: Color32 = Color32::from_rgb(0x90, 0xee, 0x90);
pub const DO: Color32 = Color32::from_rgb(0xf0, 0x80, 0x80);
pub const XANH_DUONG: Color32 = Color32::from_rgb(0xad, 0xd8, 0xe6);
pub const CAM: Color32 = Color32::from_rgb(0xff, 0xb3, 0x47);
pub const XAM: Color32 = Color32::from_rgb(0xdd, 0xdd, 0xdd);
pub const CHU_XANH: Color32 = Color32::from_rgb(0x00, 0x60, 0x00);
pub const CHU_NAU: Color32 = Color32::from_rgb(0x8a, 0x4b, 0x00);
pub const CHU_DO: Color32 = Color32::from_rgb(0xb0, 0x00, 0x00);

/// Phông mặc định của egui thiếu nhiều chữ tiếng Việt có dấu (ế, ộ, ữ…), nên nạp thêm một phông hệ
/// thống có đủ dấu. Không tìm thấy thì giữ phông mặc định (chữ có dấu thành ô vuông).
pub fn cai_phong_chu(ctx: &egui::Context) {
    let mut ung_vien: Vec<String> = Vec::new();
    if let Ok(win) = std::env::var("WINDIR") {
        for t in ["segoeui.ttf", "arial.ttf", "tahoma.ttf"] {
            ung_vien.push(format!("{win}\\Fonts\\{t}"));
        }
    }
    for p in [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/Library/Fonts/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ] {
        ung_vien.push(p.to_string());
    }
    let Some(du_lieu) = ung_vien.iter().find_map(|p| std::fs::read(p).ok()) else { return };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("he_thong".into(), egui::FontData::from_owned(du_lieu));
    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "he_thong".into());
    fonts.families.entry(egui::FontFamily::Monospace).or_default().push("he_thong".into());
    ctx.set_fonts(fonts);
}

/// Giao diện sáng, nền xanh nhạt như bản Tkinter.
pub fn dat_kieu(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.visuals.panel_fill = NEN;
        s.visuals.window_fill = NEN;
    });
}

/// Nút có màu nền; `bat` = false thì mờ, không bấm được.
pub fn nut(ui: &mut egui::Ui, chu: &str, mau: Color32, bat: bool, rong: f32) -> bool {
    ui.add_enabled(bat, egui::Button::new(RichText::new(chu).color(Color32::BLACK)).fill(mau).min_size(egui::vec2(rong, 26.0))).clicked()
}

/// Khung nhóm có tiêu đề (như LabelFrame của Tk).
pub fn nhom<R>(ui: &mut egui::Ui, tieu_de: &str, noi_dung: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .fill(NEN)
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(tieu_de).strong().size(12.5));
                noi_dung(ui)
            })
            .inner
        })
        .inner
}

/// Thanh đo mức âm -60 … 0 dBFS (như MR6.2): xanh < -12, vàng < -3, đỏ gần cắt ngọn; giữ đỉnh,
/// rơi 2 dB mỗi lần vẽ (như ThanhMuc của bản Tkinter).
pub struct ThanhMuc {
    hien: f64,
}

impl ThanhMuc {
    pub fn new() -> ThanhMuc {
        ThanhMuc { hien: f64::NEG_INFINITY }
    }

    pub fn ve(&mut self, ui: &mut egui::Ui, db: f64, rong: f32) {
        self.hien = if db > self.hien { db } else { self.hien - 2.0 };
        let chu_rong = 66.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(rong, 18.0), egui::Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 0.0, Color32::from_gray(0x20));
        let mut o_chu = rect;
        o_chu.min.x = rect.max.x - chu_rong;
        p.rect_filled(o_chu, 0.0, Color32::from_gray(0xf4));
        let ti_le = if self.hien < -60.0 || !self.hien.is_finite() { 0.0 } else { ((self.hien + 60.0) / 60.0).min(1.0) as f32 };
        let mau = if self.hien < -12.0 {
            Color32::from_rgb(0x30, 0xc0, 0x30)
        } else if self.hien < -3.0 {
            Color32::from_rgb(0xe0, 0xc0, 0x20)
        } else {
            Color32::from_rgb(0xe0, 0x30, 0x30)
        };
        let mut day = rect;
        day.set_width((rect.width() - chu_rong) * ti_le);
        p.rect_filled(day, 0.0, mau);
        p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, Color32::from_gray(0x80)));
        let chu = if db < -90.0 || !db.is_finite() { "--- dBFS".to_string() } else { format!("{db:5.1} dBFS") };
        p.text(egui::pos2(rect.max.x - 4.0, rect.center().y), egui::Align2::RIGHT_CENTER, chu, egui::FontId::proportional(11.0), Color32::BLACK);
    }
}

/// Giờ địa phương HH:MM:SS (như time.strftime("%H:%M:%S") của Python).
pub fn gio_hien() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

/// Bật khi chạy riêng từng cửa sổ (không qua demo_gui): mỗi dòng nhật ký cũng in ra dòng lệnh.
pub static IN_RA: AtomicBool = AtomicBool::new(false);

/// Ô nhật ký cuộn (chỉ đọc), mỗi dòng có giờ. Ghi được từ mọi luồng (Mutex).
/// IN_RA = true: mỗi dòng cũng in ra dòng lệnh kèm tiền tố `[ten]`, để chuyển hướng được vào tệp nhật
/// ký khi thử với thiết bị thật.
#[derive(Clone)]
pub struct NhatKy {
    ten: String,
    dong: Arc<Mutex<VecDeque<String>>>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl NhatKy {
    pub fn new(ten: &str) -> NhatKy {
        NhatKy { ten: ten.to_string(), dong: Arc::new(Mutex::new(VecDeque::new())), ctx: Arc::new(Mutex::new(None)) }
    }

    pub fn ghi(&self, text: &str) {
        let gio = gio_hien();
        if IN_RA.load(Ordering::SeqCst) {
            println!("{gio} [{}] {text}", self.ten);
        }
        let mut v = self.dong.lock().unwrap();
        v.push_back(format!("{gio} {text}"));
        while v.len() > 500 {
            v.pop_front();
        }
        drop(v);
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint();
        }
    }

    /// Hàm báo sự kiện cho lõi (gọi từ luồng nền).
    pub fn bao(&self) -> crate::may_chu::Bao {
        let nk = self.clone();
        Arc::new(move |t: &str| nk.ghi(t))
    }

    pub fn ve(&self, ui: &mut egui::Ui, so_dong: usize, rong: f32) {
        if self.ctx.lock().unwrap().is_none() {
            *self.ctx.lock().unwrap() = Some(ui.ctx().clone());
        }
        egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, Color32::from_gray(0x90))).inner_margin(egui::Margin::same(3.0)).show(ui, |ui| {
            ui.set_width(rong);
            egui::ScrollArea::vertical().id_salt(("nk", self.ten.clone())).stick_to_bottom(true).auto_shrink([false, false]).max_height(so_dong as f32 * 15.0).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                for d in self.dong.lock().unwrap().iter() {
                    ui.label(RichText::new(d).monospace().size(11.0).color(Color32::BLACK));
                }
            });
        });
    }
}

/// Khi chạy riêng (IN_RA): cứ `chu_ky` giây in một dòng số liệu ra dòng lệnh (không ghi vào ô nhật ký)
/// – để tệp nhật ký khi thử thiết bị thật cho thấy tiếng/hình có thật sự đi qua.
pub struct ThongKeDinhKy {
    ten: String,
    chu_ky: f64,
    han: f64,
}

impl ThongKeDinhKy {
    pub fn new(ten: &str) -> ThongKeDinhKy {
        ThongKeDinhKy { ten: ten.to_string(), chu_ky: 5.0, han: crate::chung::bay_gio() + 5.0 }
    }

    pub fn in_neu_den_han(&mut self, tao_dong: impl FnOnce() -> String) {
        let bay = crate::chung::bay_gio();
        if !IN_RA.load(Ordering::SeqCst) || bay < self.han {
            return;
        }
        self.han = bay + self.chu_ky; // không "đuổi bù" các chu kỳ đã lỡ (lúc chưa phát)
        println!("{} [{}] {}", gio_hien(), self.ten, tao_dong());
    }
}

/// Ô hình đen kích thước cố định; có ảnh thì vẽ texture (phóng ×2, điểm gần nhất), không thì chữ chờ.
pub fn o_hinh(ui: &mut egui::Ui, tex: Option<&egui::TextureHandle>, rong: f32, cao: f32, chu_cho: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(rong, cao), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, Color32::BLACK);
    match tex {
        Some(t) => {
            p.image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, chu_cho, egui::FontId::proportional(13.0), Color32::from_gray(0xc0));
        }
    }
}

/// Cập nhật (hoặc tạo) texture từ khung RGB thô.
pub fn dat_texture(ctx: &egui::Context, tex: &mut Option<egui::TextureHandle>, ten: &str, w: usize, h: usize, rgb: &[u8]) {
    let a = egui::ColorImage::from_rgb([w, h], rgb);
    match tex {
        Some(t) => t.set(a, egui::TextureOptions::NEAREST),
        None => *tex = Some(ctx.load_texture(ten, a, egui::TextureOptions::NEAREST)),
    }
}

/// Thanh trạng thái dưới cùng (nền trắng, viền lõm).
pub fn thanh_trang_thai(ui: &mut egui::Ui, chu: &str) {
    egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, Color32::from_gray(0x90))).inner_margin(egui::Margin::symmetric(4.0, 2.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(chu).color(Color32::BLACK));
    });
}

/// Ô nhập một dòng, mờ khi `bat` = false.
pub fn o_nhap(ui: &mut egui::Ui, s: &mut String, rong: f32, bat: bool) {
    ui.add_enabled(bat, egui::TextEdit::singleline(s).desired_width(rong));
}

pub fn chu_ms(x: Option<f64>, dau: bool) -> String {
    match x {
        None => "–".into(),
        Some(v) if dau => format!("{v:+.0} ms"),
        Some(v) => format!("{v:.0} ms"),
    }
}

/// Ghi ảnh chụp cửa sổ (RGBA của egui), có thể cắt một vùng, thành PNG bằng bộ ghi PNG viết tay.
pub fn rgb_cua_anh(anh: &egui::ColorImage, vung: Option<egui::Rect>) -> (usize, usize, Vec<u8>) {
    let (w, h) = (anh.size[0], anh.size[1]);
    let (x0, y0, x1, y1) = match vung {
        Some(r) => ((r.min.x.max(0.0) as usize).min(w), (r.min.y.max(0.0) as usize).min(h), (r.max.x.ceil() as usize).min(w), (r.max.y.ceil() as usize).min(h)),
        None => (0, 0, w, h),
    };
    let mut rgb = Vec::with_capacity((x1 - x0) * (y1 - y0) * 3);
    for y in y0..y1 {
        for p in &anh.pixels[y * w + x0..y * w + x1] {
            rgb.extend_from_slice(&[p.r(), p.g(), p.b()]);
        }
    }
    (x1 - x0, y1 - y0, rgb)
}
