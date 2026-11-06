//! Bài mở rộng MR6.9 – Bảng điều khiển máy chủ quay số (egui).
//!
//! Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế
//! độ, nhật ký) ở gui_chung::CuaSoServer; tệp này chỉ vẽ khung "Tài nguyên": 10 món quà (ảnh, còn / đã
//! trao cho ai), số quà còn lại, số lượt đã quay, số lượt trúng theo sổ kiểm toán, tham số đặt lại (số
//! quà, xác suất p).
//!
//! Chạy:  server_gui [host] [cổng]        (mặc định 127.0.0.1 44732)

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::{chu_p, CONG_GUI, NV, TEN_QUA};
use crate::nguoi_ao::SO_LAN_MAC_DINH;
use crate::vong_quay_gui::{nghin, o_anh_qua};
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};

pub struct TaiNguyenQuaySo {
    o_so_qua: String,
    o_p: String,
}

impl TaiNguyenQuaySo {
    /// Điền ô tham số (kịch bản demo) – như người dùng gõ.
    pub fn dat_tham_so(&mut self, so_qua: &str, p: &str) {
        self.o_so_qua = so_qua.into();
        self.o_p = p.into();
    }
}

impl TaiNguyen<NV> for TaiNguyenQuaySo {
    const TIEU_DE: &'static str = "Máy chủ quay số MR6.9";
    const TEN_TAI_NGUYEN: &'static str = "Tài nguyên: kho quà";

    fn moi(ts: &Obj) -> Self {
        TaiNguyenQuaySo { o_so_qua: so_us(ts.get("so_qua")), o_p: fmt_g(lay_f64(ts, "p").unwrap_or(0.0005)) }
    }

    fn ve(&mut self, ui: &mut egui::Ui, tt: Option<&Obj>, _tk: Option<&Obj>) {
        let (mut con, mut luot) = (("–".to_string(), mau("#8b0000")), "–".to_string());
        let mut qua: Vec<Obj> = Vec::new();
        let mut so_qua = 0usize;
        if let Some(tt) = tt {
            let (st, sq) = (lay_i64(tt, "so_trung").unwrap_or(0), lay_i64(tt, "so_qua").unwrap_or(0));
            con = (format!("Còn {}/{sq} quà · trao {st}", so_us(tt.get("con_lai"))), if st > sq { DO_DAM } else { mau("#8b0000") });
            luot = format!("Đã quay {} lượt · p = {}", nghin(lay_i64(tt, "da_quay").unwrap_or(0)), chu_p(lay_f64(tt, "p").unwrap_or(0.0)));
            qua = tt.get("qua").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_object().cloned()).collect()).unwrap_or_default();
            so_qua = sq.max(0) as usize;
        }
        ui.label(RichText::new(con.0).strong().size(17.0).color(con.1));
        ui.label(RichText::new(luot).size(11.0));
        egui::Grid::new("kho_qua").num_columns(5).spacing([2.0, 2.0]).show(ui, |ui| {
            for i in 0..TEN_QUA.len() {
                ui.vertical(|ui| {
                    ui.set_width(70.0);
                    match qua.get(i).filter(|_| i < so_qua) {
                        Some(q) => {
                            let c = dung(q, "con");
                            ui.horizontal(|ui| {
                                ui.add_space(14.0);
                                o_anh_qua(ui, 44.0, 38.0, Some(i), !c, if c { Color32::WHITE } else { mau("#e4e4e4") }, Some((2.0, if c { XANH_DAM } else { mau("#b0b0b0") })));
                            });
                            let chu = if c { TEN_QUA[i].to_string() } else { format!("→{}", q.get("nguoi").filter(|v| !v.is_null()).map(py_str).unwrap_or_else(|| "?".into())) };
                            ui.add_sized([70.0, 13.0], egui::Label::new(RichText::new(chu).size(9.5).color(if c { Color32::BLACK } else { mau("#a00000") })).truncate());
                        }
                        None => {
                            ui.horizontal(|ui| {
                                ui.add_space(14.0);
                                o_anh_qua(ui, 44.0, 38.0, None, false, mau("#d0d0d0"), Some((2.0, mau("#b0b0b0"))));
                            });
                            ui.add_sized([70.0, 13.0], egui::Label::new(""));
                        }
                    }
                });
                if i % 5 == 4 {
                    ui.end_row();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Số quà:");
            o_nhap(ui, &mut self.o_so_qua, 34.0, true);
            ui.label("  Xác suất p:");
            o_nhap(ui, &mut self.o_p, 64.0, true);
        });
    }

    fn tham_so_dat_lai(&self) -> Result<Obj, String> {
        let loi = || "tham số phải là số".to_string();
        Ok(obj(json!({"so_qua": self.o_so_qua.trim().parse::<i64>().map_err(|_| loi())?, "p": self.o_p.trim().parse::<f64>().map_err(|_| loi())?})))
    }
}

pub type CuaSoServerQuaySo = CuaSoServer<NV, TaiNguyenQuaySo>;

pub fn tao_cua_so(host: &str, cong: u16) -> CuaSoServerQuaySo {
    CuaSoServer::moi(host, cong, "khoa", 2.0, Obj::new(), 10000, 200, SO_LAN_MAC_DINH)
}

pub fn tieu_de() -> String {
    format!("{} – bảng điều khiển", <TaiNguyenQuaySo as TaiNguyen<NV>>::TIEU_DE)
}

/// server_gui [host] [cổng]
pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    chay_cua_so(&tieu_de(), [1060.0, 720.0], tao_cua_so(&host, cong));
}
