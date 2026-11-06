//! Bài mở rộng MR6.8 – Bảng điều khiển máy chủ đặt vé xem phim (egui).
//!
//! Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế
//! độ, nhật ký) ở gui_chung::CuaSoServer; tệp này chỉ vẽ khung "Tài nguyên": sơ đồ phòng chiếu thu nhỏ
//! (trống / đang giữ / đã bán, ghế nóng viền đỏ), số ghế mỗi loại và tham số đặt lại (thời hạn giữ chỗ,
//! ghế nóng của bộ mô phỏng, giá vé).
//!
//! Chạy:  server_gui [host] [cổng]        (mặc định 127.0.0.1 44722; giữ chỗ 30 s)

use crate::chung::*;
use crate::dat_ve::chu_ghe;
use crate::dat_ve_gui::{mau_theo_tt, SoDoGhe, DA_BAN, KHAC_GIU};
use crate::gui_chung::*;
use crate::may_chu::{CONG_GUI, COT, HANG, NV};
use crate::nguoi_ao::SO_LAN_MAC_DINH;
use eframe::egui::{self, Color32, RichText};
use serde_json::json;
use std::collections::HashMap;

pub struct TaiNguyenDatVe {
    so_do: SoDoGhe,
    o_ts: Vec<(&'static str, &'static str, String, f32)>, // (mã, nhãn, giá trị ô, rộng)
}

impl TaiNguyenDatVe {
    /// Điền ô tham số (kịch bản demo) – như người dùng gõ.
    pub fn dat_tham_so(&mut self, ma: &str, gia_tri: &str) {
        if let Some(o) = self.o_ts.iter_mut().find(|o| o.0 == ma) {
            o.2 = gia_tri.to_string();
        }
    }
}

impl TaiNguyen<NV> for TaiNguyenDatVe {
    const TIEU_DE: &'static str = "Máy chủ đặt vé xem phim MR6.8";
    const TEN_TAI_NGUYEN: &'static str = "Tài nguyên: phòng chiếu 8 × 12 ghế";

    fn moi(ts: &Obj) -> Self {
        let o = |ma: &'static str, nhan: &'static str, rong: f32| (ma, nhan, ts.get(ma).map(|v| if v.is_f64() { fmt_g(v.as_f64().unwrap_or(0.0)) } else { py_str(v) }).unwrap_or_default(), rong);
        TaiNguyenDatVe { so_do: SoDoGhe::new(HANG, COT, 13.0, 2.0, 6.0, 16.0, false), o_ts: vec![o("ttl_s", "Giữ chỗ (s)", 44.0), o("ghe_nong", "Ghế nóng", 36.0), o("gia_ve", "Giá vé", 60.0)] }
    }

    fn ve(&mut self, ui: &mut egui::Ui, tt: Option<&Obj>, _tk: Option<&Obj>) {
        ui.horizontal_top(|ui| {
            let (r, pt) = ui.allocate_painter(egui::vec2(206.0, 124.0), egui::Sense::hover());
            pt.rect_filled(r.rect, 0.0, mau("#1c1c24"));
            let m = tt.map(|t| mau_theo_tt(t, None)).unwrap_or_default();
            let nong = tt.and_then(|t| lay_str(t, "ghe_nong")).unwrap_or("").to_string();
            let vien: HashMap<String, Color32> = if nong.is_empty() { HashMap::new() } else { [(nong.clone(), mau("#ff3030"))].into() };
            self.so_do.ve(&pt, r.rect.min, &m, &vien);
            ui.vertical(|ui| {
                let dem = |k: &str| tt.and_then(|t| t.get("dem")).and_then(|d| d.get(k)).map_or("–".into(), |v| py_str(v));
                for (ma, chu, c) in [("trong", "Trống", Color32::WHITE), ("giu", "Đang giữ", KHAC_GIU), ("ban", "Đã bán", DA_BAN)] {
                    ui.horizontal(|ui| {
                        let (o, p2) = ui.allocate_painter(egui::vec2(16.0, 14.0), egui::Sense::hover());
                        p2.rect_filled(o.rect, 0.0, c);
                        p2.rect_stroke(o.rect, 0.0, egui::Stroke::new(1.0_f32, Color32::BLACK));
                        ui.label(RichText::new(format!(" {chu}: {}", dem(ma))).strong().size(13.0));
                    });
                }
                if let Some(t) = tt {
                    let chu = if nong.is_empty() {
                        "Ghế nóng (mô phỏng): ngẫu nhiên".to_string()
                    } else {
                        format!("Ghế nóng (mô phỏng): {nong}\n{nong}: {}", chu_ghe(t, &nong).unwrap_or("trống"))
                    };
                    ui.label(RichText::new(chu).size(10.5));
                }
            });
        });
        ui.horizontal(|ui| {
            for (i, (_, nhan, gt, rong)) in self.o_ts.iter_mut().enumerate() {
                if i > 0 {
                    ui.add_space(6.0);
                }
                ui.label(format!("{nhan}:"));
                o_nhap(ui, gt, *rong, true);
            }
        });
    }

    fn tham_so_dat_lai(&self) -> Result<Obj, String> {
        let v = |ma: &str| self.o_ts.iter().find(|o| o.0 == ma).map(|o| o.2.trim().to_string()).unwrap_or_default();
        let loi = || "thời hạn và giá vé phải là số".to_string();
        Ok(obj(json!({
            "ttl_s": v("ttl_s").parse::<f64>().map_err(|_| loi())?,
            "ghe_nong": v("ghe_nong").to_uppercase(),
            "gia_ve": v("gia_ve").parse::<i64>().map_err(|_| loi())?,
        })))
    }
}

pub type CuaSoServerDatVe = CuaSoServer<NV, TaiNguyenDatVe>;

pub fn tao_cua_so(host: &str, cong: u16) -> CuaSoServerDatVe {
    CuaSoServer::moi(host, cong, "khoa", 2.0, Obj::new(), 500, 200, SO_LAN_MAC_DINH)
}

pub fn tieu_de() -> String {
    format!("{} – bảng điều khiển", <TaiNguyenDatVe as TaiNguyen<NV>>::TIEU_DE)
}

/// server_gui [host] [cổng]
pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    chay_cua_so(&tieu_de(), [1060.0, 720.0], tao_cua_so(&host, cong));
}
