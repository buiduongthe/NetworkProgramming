//! Bài mở rộng MR6.6 – Bảng điều khiển máy chủ đấu giá (egui).
//!
//! Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế
//! độ, nhật ký) ở gui_chung::CuaSoServer; tệp này chỉ vẽ khung "Tài nguyên": món hàng, giá hiện tại,
//! người dẫn đầu, đồng hồ phiên, vài giá gần nhất và tham số đặt lại phiên.
//!
//! Chạy:  server_gui [host] [cổng]        (mặc định 127.0.0.1 44702; phiên mặc định 120 s)

use crate::chung::*;
use crate::dau_gia_gui::{buoc_gia, dong_ho, ten_mon, ve_binh_gom};
use crate::gui_chung::*;
use crate::may_chu::{so_nguyen, CONG_GUI, NV};
use crate::nguoi_ao::SO_LAN_MAC_DINH;
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};

pub struct TaiNguyenDauGia {
    o_ts: Vec<(&'static str, &'static str, String, f32)>, // (mã, nhãn, giá trị ô, rộng)
}

impl TaiNguyenDauGia {
    /// Điền ô tham số (kịch bản demo) – như người dùng gõ.
    pub fn dat_tham_so(&mut self, ma: &str, gia_tri: &str) {
        if let Some(o) = self.o_ts.iter_mut().find(|o| o.0 == ma) {
            o.2 = gia_tri.to_string();
        }
    }
}

impl TaiNguyen<NV> for TaiNguyenDauGia {
    const TIEU_DE: &'static str = "Máy chủ đấu giá MR6.6";
    const TEN_TAI_NGUYEN: &'static str = "Tài nguyên: phiên đấu giá";

    fn moi(ts: &Obj) -> Self {
        let o = |ma: &'static str, nhan: &'static str, rong: f32| (ma, nhan, ts.get(ma).map(|v| if v.is_f64() { fmt_g(v.as_f64().unwrap_or(0.0)) } else { py_str(v) }).unwrap_or_default(), rong);
        TaiNguyenDauGia { o_ts: vec![o("gia_khoi_diem", "Giá khởi điểm", 80.0), o("buoc_gia", "Bước giá", 80.0), o("thoi_gian_s", "Phiên (s, 0 = ∞)", 50.0), o("gia_han_s", "Gia hạn (s)", 50.0)] }
    }

    fn ve(&mut self, ui: &mut egui::Ui, tt: Option<&Obj>, _tk: Option<&Obj>) {
        ui.horizontal_top(|ui| {
            let (r, pt) = ui.allocate_painter(egui::vec2(120.0, 130.0), egui::Sense::hover());
            ve_binh_gom(&pt, r.rect.min, 120.0, 130.0, 60.0, 14.0, 0.62, true);
            ui.vertical(|ui| {
                let (mut mon, mut gia, mut dan_dau, mut dh, mut ls) = ("–".to_string(), "–".to_string(), ("Dẫn đầu: –".to_string(), Color32::BLACK), ("Còn lại: –".to_string(), Color32::BLACK), Vec::new());
                if let Some(tt) = tt {
                    mon = format!("{} (bước {})", ten_mon(tt), tien(buoc_gia(tt)));
                    gia = tien(lay_i64(tt, "gia").unwrap_or(0));
                    let ai = lay_str(tt, "nguoi_dan_dau").unwrap_or("–");
                    if dung(tt, "ket_thuc") {
                        dan_dau = (format!("THẮNG: {ai}  ({} lượt)", so_us(tt.get("so_lan"))), DO_DAM);
                        dh = ("Kết thúc phiên".into(), mau("#606060"));
                    } else {
                        dan_dau = (format!("Dẫn đầu: {ai}  ({} lượt)", so_us(tt.get("so_lan"))), Color32::BLACK);
                        dh = match lay_f64(tt, "con_lai_ms") {
                            None => ("Còn lại: ∞ (không giới hạn)".into(), XANH_DAM),
                            Some(c) => (format!("Còn lại: {}", dong_ho(c / 1000.0)), if c >= 10_000.0 { XANH_DAM } else { DO_DAM }),
                        };
                    }
                    for x in tt.get("lich_su").and_then(Value::as_array).into_iter().flatten().take(3) {
                        let gh = if x.get("gia_han").and_then(Value::as_bool).unwrap_or(false) { " GH" } else { "" };
                        ls.push((format!("{:<6}{:>14}{gh}", x.get("nguoi").map(py_str).unwrap_or_default(), tien(so_nguyen(x.get("gia")))), Color32::BLACK));
                    }
                }
                ui.label(RichText::new(mon).strong().size(13.0));
                ui.label(RichText::new(gia).strong().size(23.0).color(mau("#8b0000")));
                ui.label(RichText::new(dan_dau.0).color(dan_dau.1));
                ui.label(RichText::new(dh.0).monospace().strong().size(15.0).color(dh.1));
                danh_sach(ui, &ls, 3, 250.0, 11.0);
            });
        });
        for hang in self.o_ts.chunks_mut(2) {
            ui.horizontal(|ui| {
                for (i, (_, nhan, gt, rong)) in hang.iter_mut().enumerate() {
                    if i > 0 {
                        ui.add_space(8.0);
                    }
                    ui.label(format!("{nhan}:"));
                    o_nhap(ui, gt, *rong, true);
                }
            });
        }
    }

    fn tham_so_dat_lai(&self) -> Result<Obj, String> {
        let v = |ma: &str| self.o_ts.iter().find(|o| o.0 == ma).map(|o| o.2.trim().to_string()).unwrap_or_default();
        let loi = || "tham số phải là số".to_string();
        Ok(obj(json!({
            "gia_khoi_diem": v("gia_khoi_diem").parse::<i64>().map_err(|_| loi())?,
            "buoc_gia": v("buoc_gia").parse::<i64>().map_err(|_| loi())?,
            "thoi_gian_s": v("thoi_gian_s").parse::<f64>().map_err(|_| loi())?,
            "gia_han_s": v("gia_han_s").parse::<f64>().map_err(|_| loi())?,
        })))
    }
}

pub type CuaSoServerDauGia = CuaSoServer<NV, TaiNguyenDauGia>;

pub fn tao_cua_so(host: &str, cong: u16) -> CuaSoServerDauGia {
    CuaSoServer::moi(host, cong, "khoa", 2.0, obj(json!({"thoi_gian_s": 120.0})), 200, 200, SO_LAN_MAC_DINH)
}

pub fn tieu_de() -> String {
    format!("{} – bảng điều khiển", <TaiNguyenDauGia as TaiNguyen<NV>>::TIEU_DE)
}

/// server_gui [host] [cổng]
pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    chay_cua_so(&tieu_de(), [1060.0, 720.0], tao_cua_so(&host, cong));
}
