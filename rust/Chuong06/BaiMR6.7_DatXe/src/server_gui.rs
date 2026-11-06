//! Bài mở rộng MR6.7 – Bảng điều khiển máy chủ đặt xe (egui).
//!
//! Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế
//! độ, nhật ký) ở gui_chung::CuaSoServer; tệp này chỉ vẽ khung "Tài nguyên": bản đồ thu nhỏ với các xe
//! (xe đã có khách mờ đi, ghi tên khách), số xe trống, danh sách xe và tham số đặt lại (số xe).
//!
//! Chạy:  server_gui [host] [cổng]        (mặc định 127.0.0.1 44712; 1 xe)

use crate::chung::*;
use crate::dat_xe_gui::BanDo;
use crate::gui_chung::*;
use crate::may_chu::{CONG_GUI, NV};
use crate::nguoi_ao::SO_LAN_MAC_DINH;
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};

pub struct TaiNguyenDatXe {
    o_so_xe: String,
}

impl TaiNguyenDatXe {
    /// Điền ô tham số (kịch bản demo) – như người dùng gõ.
    pub fn dat_so_xe(&mut self, so_xe: &str) {
        self.o_so_xe = so_xe.to_string();
    }
}

impl TaiNguyen<NV> for TaiNguyenDatXe {
    const TIEU_DE: &'static str = "Máy chủ đặt xe MR6.7";
    const TEN_TAI_NGUYEN: &'static str = "Tài nguyên: các xe (mỗi xe 1 chỗ)";

    fn moi(ts: &Obj) -> Self {
        TaiNguyenDatXe { o_so_xe: so_us(ts.get("so_xe")) }
    }

    fn ve(&mut self, ui: &mut egui::Ui, tt: Option<&Obj>, _tk: Option<&Obj>) {
        let (rong, cao) = (200.0, 150.0);
        ui.horizontal_top(|ui| {
            let (r, pt) = ui.allocate_painter(egui::vec2(rong, cao), egui::Sense::hover());
            let bd = BanDo { pt: &pt, g: r.rect.min, w: rong, h: cao, nho: true };
            bd.ve_nen();
            if let Some(tt) = tt {
                bd.cac_xe(tt, None, true);
            }
            pt.rect_stroke(r.rect, 0.0, egui::Stroke::new(1.0_f32, mau("#808080")));
            ui.vertical(|ui| {
                let (chu, m) = match tt {
                    Some(t) => {
                        let trong = lay_i64(t, "trong").unwrap_or(0);
                        (format!("Trống {trong}/{} xe", so_us(t.get("so_xe"))), if trong > 0 { XANH_DAM } else { DO_DAM })
                    }
                    None => ("–".into(), Color32::BLACK),
                };
                ui.label(RichText::new(chu).strong().size(20.0).color(m));
                let ds: Vec<(String, Color32)> = tt
                    .and_then(|t| t.get("xe"))
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|x| {
                                let khach = x.get("khach").and_then(Value::as_str);
                                let d = format!("{:>2} {:<6}{:<11}{}", so_us(x.get("ma")), so_us(x.get("tai_xe")), so_us(x.get("bien_so")), khach.unwrap_or("·"));
                                (d, if khach.is_some() { mau("#a00000") } else { Color32::BLACK })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                danh_sach(ui, &ds, 6, 200.0, 11.0);
            });
        });
        ui.horizontal(|ui| {
            ui.label("Số xe (1–50):");
            o_nhap(ui, &mut self.o_so_xe, 44.0, true);
        });
    }

    fn tham_so_dat_lai(&self) -> Result<Obj, String> {
        let n = self.o_so_xe.trim().parse::<i64>().map_err(|_| "số xe phải là số nguyên".to_string())?;
        Ok(obj(json!({"so_xe": n})))
    }
}

pub type CuaSoServerDatXe = CuaSoServer<NV, TaiNguyenDatXe>;

pub fn tao_cua_so(host: &str, cong: u16) -> CuaSoServerDatXe {
    CuaSoServer::moi(host, cong, "khoa", 2.0, obj(json!({"so_xe": 1})), 500, 200, SO_LAN_MAC_DINH)
}

pub fn tieu_de() -> String {
    format!("{} – bảng điều khiển", <TaiNguyenDatXe as TaiNguyen<NV>>::TIEU_DE)
}

/// server_gui [host] [cổng]
pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    chay_cua_so(&tieu_de(), [1060.0, 720.0], tao_cua_so(&host, cong));
}
