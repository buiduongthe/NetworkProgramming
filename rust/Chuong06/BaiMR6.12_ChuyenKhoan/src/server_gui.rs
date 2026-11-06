//! Bài mở rộng MR6.12 – Bảng điều khiển máy chủ ngân hàng (egui).
//!
//! Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế
//! độ, nhật ký) ở gui_chung::CuaSoServer; tệp này chỉ vẽ khung "Tài nguyên": TỔNG TIỀN HỆ THỐNG (xanh khi
//! khớp tổng ban đầu, đỏ khi lệch), bảng tài khoản dạng thanh số dư, số giao dịch, số vụ khóa chết, vài
//! giao dịch gần nhất và tham số đặt lại (số tài khoản, số dư ban đầu, ô chọn "khóa ngây thơ").
//!
//! Chạy:  server_gui [host] [cổng]        (mặc định 127.0.0.1 44762)

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::{tien_dau, CONG_GUI, NV};
use crate::nguoi_ao::SO_LAN_MAC_DINH;
use eframe::egui::{self, Color32, RichText, Stroke};
use serde_json::{json, Value};

pub struct TaiNguyenNganHang {
    o_so_tk: String,
    o_so_du: String,
    pub ngay_tho: bool,
}

impl TaiNguyenNganHang {
    /// Bấm ô chọn "khóa ngây thơ" (kịch bản demo) – có hiệu lực khi bấm Đặt lại.
    pub fn dat_khoa_ngay_tho(&mut self, bat: bool) {
        self.ngay_tho = bat;
    }
}

/// Mỗi tài khoản một thanh số dư (≤ 10 dòng mỗi cột, 2 cột khi > 10 tài khoản).
fn ve_tai_khoan(ui: &mut egui::Ui, tt: Option<&Obj>) {
    let (r, pt) = ui.allocate_painter(egui::vec2(360.0, 150.0), egui::Sense::hover());
    pt.rect(r.rect, 0.0, Color32::WHITE, Stroke::new(1.0_f32, mau("#808080")));
    let Some(tt) = tt else { return };
    let ds: Vec<&Value> = tt.get("tai_khoan").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
    let dau = lay_i64(tt, "tong_dau").unwrap_or(0) / (ds.len().max(1) as i64);
    let lon = ds.iter().map(|t| t.get("so_du").and_then(Value::as_i64).unwrap_or(0)).chain(std::iter::once(2 * dau)).max().unwrap_or(1).max(1) as f32;
    let cot = if ds.len() <= 10 { 1 } else { 2 };
    let (rong, cao) = (360.0 / cot as f32, 15.0);
    let g = r.rect.min;
    for (i, t) in ds.iter().enumerate() {
        let so_du = t.get("so_du").and_then(Value::as_i64).unwrap_or(0);
        let (x, y) = ((i / 10) as f32 * rong + 4.0, (i % 10) as f32 * cao + 3.0);
        pt.text(p(g, x, y + 6.0), egui::Align2::LEFT_CENTER, format!("{} {}", so_us(t.get("ma")), so_us(t.get("chu"))), egui::FontId::proportional(9.5), Color32::BLACK);
        let (x0, x1) = (x + 62.0, x + rong - 76.0);
        pt.rect_filled(egui::Rect::from_min_max(p(g, x0, y + 2.0), p(g, x1, y + 11.0)), 0.0, mau("#eeeeee"));
        let w = ((x1 - x0) * so_du as f32 / lon).max(0.0);
        pt.rect_filled(egui::Rect::from_min_max(p(g, x0, y + 2.0), p(g, x0 + w, y + 11.0)), 0.0, mau(if so_du >= dau { "#4a90d9" } else { "#f0a030" }));
        let xd = x0 + (x1 - x0) * dau as f32 / lon;
        pt.line_segment([p(g, xd, y), p(g, xd, y + 13.0)], Stroke::new(1.0_f32, mau("#808080")));
        pt.text(p(g, x + rong - 6.0, y + 6.0), egui::Align2::RIGHT_CENTER, tien(so_du), egui::FontId::monospace(9.5), Color32::BLACK);
    }
}

impl TaiNguyen<NV> for TaiNguyenNganHang {
    const TIEU_DE: &'static str = "Máy chủ ngân hàng MR6.12";
    const TEN_TAI_NGUYEN: &'static str = "Tài nguyên: các tài khoản";

    fn moi(ts: &Obj) -> Self {
        TaiNguyenNganHang { o_so_tk: so_us(ts.get("so_tk")), o_so_du: so_us(ts.get("so_du_dau")), ngay_tho: lay_i64(ts, "khoa_ngay_tho").unwrap_or(0) != 0 }
    }

    fn ve(&mut self, ui: &mut egui::Ui, tt: Option<&Obj>, _tk: Option<&Obj>) {
        ui.label(RichText::new("TỔNG TIỀN HỆ THỐNG").strong().size(10.5).color(mau("#404040")));
        let lech = tt.map(|t| lay_i64(t, "tong").unwrap_or(0) - lay_i64(t, "tong_dau").unwrap_or(0));
        let (chu, nen, mc) = match (tt, lech) {
            (Some(t), Some(l)) => (tien(lay_i64(t, "tong").unwrap_or(0)), mau(if l != 0 { "#ff6060" } else { "#70d070" }), if l != 0 { Color32::WHITE } else { Color32::BLACK }),
            _ => ("–".into(), Color32::from_gray(0xe0), Color32::BLACK),
        };
        nhan_nen(ui, &chu, nen, mc, 20.0, 260.0);
        if let (Some(t), Some(l)) = (tt, lech) {
            let ct = format!("ban đầu {} – {}", tien(lay_i64(t, "tong_dau").unwrap_or(0)), if l != 0 { format!("LỆCH {}", tien_dau(l)) } else { "khớp".into() });
            ui.label(RichText::new(ct).size(10.5).color(if l != 0 { DO_DAM } else { XANH_DAM }));
        }
        ve_tai_khoan(ui, tt);
        if let Some(t) = tt {
            let kc = lay_i64(t, "khoa_chet").unwrap_or(0);
            let chu = format!("Giao dịch: {}   Khóa chết phát hiện: {kc}{}", so_us(t.get("so_gd")), if dung(t, "khoa_ngay_tho") { "   [khóa ngây thơ]" } else { "   [khóa theo thứ tự]" });
            ui.label(RichText::new(chu).size(11.5).color(if kc > 0 { DO_DAM } else { Color32::BLACK }));
        }
        let dong: Vec<(String, Color32)> = tt
            .and_then(|t| t.get("lich_su"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(3)
            .map(|g| (format!("{} {}→{} {:>13} {}", so_us(g.get("gio")), so_us(g.get("tu")), so_us(g.get("den")), tien(g.get("so_tien").and_then(Value::as_i64).unwrap_or(0)), so_us(g.get("nguoi"))), Color32::BLACK))
            .collect();
        danh_sach(ui, &dong, 3, 360.0, 10.0);
        ui.horizontal(|ui| {
            ui.label("Số tài khoản:");
            o_nhap(ui, &mut self.o_so_tk, 30.0, true);
            ui.label("  Số dư ban đầu:");
            o_nhap(ui, &mut self.o_so_du, 76.0, true);
        });
        ui.checkbox(&mut self.ngay_tho, "Khóa ngây thơ: nguồn rồi đích (có thể khóa chết)");
    }

    fn tham_so_dat_lai(&self) -> Result<Obj, String> {
        let loi = || "tham số phải là số".to_string();
        Ok(obj(json!({"so_tk": self.o_so_tk.trim().parse::<i64>().map_err(|_| loi())?, "so_du_dau": self.o_so_du.trim().parse::<i64>().map_err(|_| loi())?, "khoa_ngay_tho": self.ngay_tho as i64})))
    }
}

pub type CuaSoServerNganHang = CuaSoServer<NV, TaiNguyenNganHang>;

pub fn tao_cua_so(host: &str, cong: u16) -> CuaSoServerNganHang {
    CuaSoServer::moi(host, cong, "khoa", 2.0, Obj::new(), 500, 200, SO_LAN_MAC_DINH)
}

pub fn tieu_de() -> String {
    format!("{} – bảng điều khiển", <TaiNguyenNganHang as TaiNguyen<NV>>::TIEU_DE)
}

/// server_gui [host] [cổng]
pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    chay_cua_so(&tieu_de(), [1060.0, 760.0], tao_cua_so(&host, cong));
}
