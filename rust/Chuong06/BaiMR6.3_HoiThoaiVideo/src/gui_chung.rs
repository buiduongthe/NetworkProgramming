//! Tiện ích dùng chung cho các cửa sổ giao diện (egui): phông chữ tiếng Việt, nút màu, khung nhóm,
//! chuyển khung RGB thành ảnh egui, ghi ảnh chụp cửa sổ thành PNG.
//!
//! Mô hình egui là "immediate mode": mỗi lần vẽ lại, hàm `update` dựng lại toàn bộ giao diện từ
//! trạng thái hiện tại. Chỉ luồng giao diện gọi `update` và chạm vào điều khiển; luồng mạng/âm thanh
//! chỉ sửa trạng thái dùng chung (trong Mutex) và nhờ giao diện vẽ lại (`request_repaint`).
//! Ảnh camera đi vào giao diện dưới dạng texture: mỗi khung mới gọi `TextureHandle::set`.

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
pub const NEN: Color32 = Color32::from_rgb(176, 196, 222);

/// Nút có màu nền; `bat` = false thì mờ, không bấm được.
pub fn nut(ui: &mut egui::Ui, chu: &str, mau: Color32, bat: bool) -> bool {
    ui.add_enabled(bat, egui::Button::new(RichText::new(chu).color(Color32::BLACK)).fill(mau).min_size(egui::vec2(96.0, 26.0))).clicked()
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

/// Khung RGB thô (w×h×3 byte) -> ảnh egui.
pub fn anh_egui(w: usize, h: usize, rgb: &[u8]) -> egui::ColorImage {
    egui::ColorImage::from_rgb([w, h], rgb)
}

/// Ghi ảnh chụp cửa sổ (RGBA của egui) thành PNG RGB bằng bộ ghi PNG viết tay của bài (chung.rs).
pub fn ghi_png_anh(duong_dan: &str, anh: &egui::ColorImage) -> std::io::Result<()> {
    let rgb: Vec<u8> = anh.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
    crate::chung::ghi_png(duong_dan, &rgb, anh.size[0], anh.size[1])
}
