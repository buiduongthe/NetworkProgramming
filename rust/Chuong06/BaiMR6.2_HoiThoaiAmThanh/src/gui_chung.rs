//! Tiện ích dùng chung cho các cửa sổ giao diện (egui): phông chữ tiếng Việt, thanh đo mức,
//! nút màu, ghi ảnh chụp cửa sổ thành PNG.
//!
//! Mô hình egui là "immediate mode": mỗi lần vẽ lại, hàm `update` dựng lại toàn bộ giao diện từ
//! trạng thái hiện tại. Chỉ luồng giao diện gọi `update` và chạm vào điều khiển; luồng mạng/âm thanh
//! chỉ sửa trạng thái dùng chung (trong Mutex) và nhờ giao diện vẽ lại (`request_repaint`).

use eframe::egui::{self, Color32, RichText};

/// Phông mặc định của egui (Ubuntu-Light) thiếu nhiều chữ tiếng Việt có dấu (ế, ộ, ữ…), nên nạp
/// thêm một phông hệ thống có đủ dấu. Không tìm thấy thì giữ phông mặc định (chữ có dấu thành ô vuông).
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
    // đặt lên đầu danh sách: chữ nào phông hệ thống có thì dùng phông này
    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "he_thong".into());
    fonts.families.entry(egui::FontFamily::Monospace).or_default().push("he_thong".into());
    ctx.set_fonts(fonts);
}

/// Giao diện sáng, nền xanh nhạt giống ảnh mẫu.
pub fn dat_kieu(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.slider_width = 160.0;
    });
}

pub const XANH_LA: Color32 = Color32::from_rgb(144, 238, 144);
pub const DO_NHAT: Color32 = Color32::from_rgb(240, 128, 128);
pub const XANH_NHAT: Color32 = Color32::from_rgb(173, 216, 230);
pub const CAM: Color32 = Color32::from_rgb(255, 165, 0);
pub const NEN: Color32 = Color32::from_rgb(176, 196, 222);

/// Nút có màu nền; `bat` = false thì mờ, không bấm được.
pub fn nut(ui: &mut egui::Ui, chu: &str, mau: Color32, bat: bool) -> bool {
    ui.add_enabled(bat, egui::Button::new(RichText::new(chu).color(Color32::BLACK)).fill(mau).min_size(egui::vec2(96.0, 26.0))).clicked()
}

/// Thanh đo mức âm: -60 dBFS (trống) … 0 dBFS (đầy); xanh < -12, vàng < -3, đỏ khi gần cắt ngọn.
pub fn thanh_do_muc(ui: &mut egui::Ui, db: f64, rong: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(rong, 16.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, Color32::from_gray(40));
    let ti_le = if db.is_finite() { ((db + 60.0) / 60.0).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let mau = if db > -3.0 {
        Color32::from_rgb(230, 60, 50)
    } else if db > -12.0 {
        Color32::from_rgb(240, 200, 40)
    } else {
        Color32::from_rgb(60, 200, 80)
    };
    let mut day = rect;
    day.set_width(rect.width() * ti_le);
    p.rect_filled(day, 2.0, mau);
    let chu = if db.is_finite() && db > -90.0 { format!("{db:.1} dBFS") } else { "--- dBFS".to_string() };
    p.text(rect.center(), egui::Align2::CENTER_CENTER, chu, egui::FontId::proportional(11.0), Color32::WHITE);
}

/// Khung nhóm có tiêu đề (giống GroupBox của WinForms).
pub fn nhom<R>(ui: &mut egui::Ui, tieu_de: &str, noi_dung: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .fill(NEN)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(tieu_de).strong());
                noi_dung(ui)
            })
            .inner
        })
        .inner
}

/// Làm mượt mức âm cho dễ nhìn: trung bình trượt của công suất (không phải của dB).
pub fn lam_muot(cong_suat: &mut f64, db_khung: f64) -> f64 {
    let moi = if db_khung.is_finite() { 10f64.powf(db_khung / 10.0) } else { 0.0 };
    *cong_suat = 0.6 * *cong_suat + 0.4 * moi;
    if *cong_suat > 1e-9 {
        10.0 * cong_suat.log10()
    } else {
        f64::NEG_INFINITY
    }
}

// ---------------------------------------------------------------- Ghi PNG
fn crc32(du_lieu: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in du_lieu {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// Ghi ảnh chụp cửa sổ (RGBA của egui) thành PNG RGB: chữ ký + khối IHDR, IDAT (zlib), IEND.
pub fn ghi_png(duong_dan: &str, anh: &egui::ColorImage) -> std::io::Result<()> {
    let (w, h) = (anh.size[0], anh.size[1]);
    let mut hang = Vec::with_capacity((w * 3 + 1) * h);
    for y in 0..h {
        hang.push(0); // bộ lọc 0: không lọc
        for p in &anh.pixels[y * w..(y + 1) * w] {
            hang.extend_from_slice(&[p.r(), p.g(), p.b()]);
        }
    }
    let khoi = |out: &mut Vec<u8>, kieu: &[u8], du_lieu: &[u8]| {
        out.extend_from_slice(&(du_lieu.len() as u32).to_be_bytes());
        let mut kd = kieu.to_vec();
        kd.extend_from_slice(du_lieu);
        out.extend_from_slice(&kd);
        out.extend_from_slice(&crc32(&kd).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bit/kênh, kiểu màu 2 = RGB
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    khoi(&mut out, b"IHDR", &ihdr);
    khoi(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&hang, 9));
    khoi(&mut out, b"IEND", &[]);
    std::fs::write(duong_dan, out)
}
