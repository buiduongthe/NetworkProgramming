//! Bài mở rộng MR6.11 – Bảng điều khiển máy chủ flash sale (egui).
//!
//! Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế
//! độ, nhật ký) ở gui_chung::CuaSoServer; tệp này chỉ vẽ khung "Tài nguyên": ảnh sản phẩm, tồn kho, thanh
//! đã bán, số đã bán theo kho và theo sổ kiểm toán (đỏ khi lệch), đồng hồ mở bán, vài đơn gần nhất, tham
//! số đặt lại (tồn đầu kỳ, mở bán sau bao nhiêu giây).
//!
//! Chạy:  server_gui [host] [cổng]        (mặc định 127.0.0.1 44752)

use crate::chung::*;
use crate::flash_sale_gui::ve_tai_nghe;
use crate::gui_chung::*;
use crate::may_chu::{CONG_GUI, NV};
use crate::nguoi_ao::SO_LAN_MAC_DINH;
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};

pub struct TaiNguyenFlashSale {
    o_ton_dau: String,
    o_mo_sau: String,
}

impl TaiNguyenFlashSale {
    /// Điền ô tham số (kịch bản demo) – như người dùng gõ.
    pub fn dat_tham_so(&mut self, ton_dau: i64, mo_sau_s: f64) {
        self.o_ton_dau = ton_dau.to_string();
        self.o_mo_sau = fmt_g(mo_sau_s);
    }
}

impl TaiNguyen<NV> for TaiNguyenFlashSale {
    const TIEU_DE: &'static str = "Máy chủ flash sale MR6.11";
    const TEN_TAI_NGUYEN: &'static str = "Tài nguyên: kho hàng";

    fn moi(ts: &Obj) -> Self {
        TaiNguyenFlashSale { o_ton_dau: so_us(ts.get("ton_dau")), o_mo_sau: fmt_g(lay_f64(ts, "mo_sau_s").unwrap_or(30.0)) }
    }

    fn ve(&mut self, ui: &mut egui::Ui, tt: Option<&Obj>, _tk: Option<&Obj>) {
        ui.horizontal_top(|ui| {
            let (r, pt) = ui.allocate_painter(egui::vec2(120.0, 112.0), egui::Sense::hover());
            ve_tai_nghe(&pt, r.rect.min, 120.0, 112.0, 60.0, 48.0, 0.55, true);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                let Some(tt) = tt else {
                    for _ in 0..4 {
                        ui.label("–");
                    }
                    return;
                };
                let (ton_dau, ton) = (lay_i64(tt, "ton_dau").unwrap_or(1).max(1), lay_i64(tt, "ton").unwrap_or(0));
                let (da_ban, theo_so) = (lay_i64(tt, "da_ban").unwrap_or(0), lay_i64(tt, "ban_theo_so").unwrap_or(0));
                ui.label(RichText::new(tt.get("sp").and_then(|s| s.get("ten")).map(py_str).unwrap_or_default()).strong().size(11.5));
                ui.label(RichText::new(format!("Tồn {ton}/{ton_dau}")).strong().size(20.0).color(mau("#8b0000")));
                let ti_le = ((ton_dau - ton) as f32 / ton_dau as f32).min(1.0);
                let (r, pt) = ui.allocate_painter(egui::vec2(180.0, 14.0), egui::Sense::hover());
                pt.rect_filled(r.rect, 0.0, mau("#dddddd"));
                pt.rect_filled(egui::Rect::from_min_size(r.rect.min, egui::vec2(180.0 * ti_le.max(0.0), 14.0)), 0.0, mau(if ti_le >= 0.8 { "#e4002b" } else { "#ff6a00" }));
                let lech = theo_so > ton_dau || theo_so != da_ban;
                ui.label(RichText::new(format!("Đã bán {da_ban} (sổ: {theo_so}) · {} đơn", so_us(tt.get("so_don")))).color(if lech { DO_DAM } else { Color32::BLACK }));
                let (chu, m) = if !dung(tt, "mo_ban") {
                    let g = lay_i64(tt, "mo_con_ms").unwrap_or(0) / 1000;
                    (format!("Mở bán sau 00:{:02}:{:02}", g / 60, g % 60), mau("#a06000"))
                } else if ton <= 0 {
                    ("HẾT HÀNG".into(), mau("#606060"))
                } else {
                    ("ĐANG MỞ BÁN".into(), XANH_DAM)
                };
                ui.label(RichText::new(chu).monospace().strong().size(14.0).color(m));
            });
        });
        let dong: Vec<(String, Color32)> = tt
            .and_then(|t| t.get("don_gan"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(3)
            .map(|x| (format!("{} {:<6} ×{}", so_us(x.get("gio")), x.get("nguoi").map(py_str).unwrap_or_default(), so_us(x.get("so_luong"))), Color32::BLACK))
            .collect();
        danh_sach(ui, &dong, 3, 250.0, 10.5);
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label("Tồn đầu kỳ:");
            o_nhap(ui, &mut self.o_ton_dau, 50.0, true);
            ui.label("  Mở bán sau (s):");
            o_nhap(ui, &mut self.o_mo_sau, 40.0, true);
        });
    }

    fn tham_so_dat_lai(&self) -> Result<Obj, String> {
        let loi = || "tham số phải là số".to_string();
        Ok(obj(json!({"ton_dau": self.o_ton_dau.trim().parse::<i64>().map_err(|_| loi())?, "mo_sau_s": self.o_mo_sau.trim().parse::<f64>().map_err(|_| loi())?})))
    }
}

pub type CuaSoServerFlashSale = CuaSoServer<NV, TaiNguyenFlashSale>;

pub fn tao_cua_so(host: &str, cong: u16) -> CuaSoServerFlashSale {
    CuaSoServer::moi(host, cong, "khoa", 2.0, Obj::new(), 10000, 200, SO_LAN_MAC_DINH)
}

pub fn tieu_de() -> String {
    format!("{} – bảng điều khiển", <TaiNguyenFlashSale as TaiNguyen<NV>>::TIEU_DE)
}

/// server_gui [host] [cổng]
pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    chay_cua_so(&tieu_de(), [1060.0, 720.0], tao_cua_so(&host, cong));
}
