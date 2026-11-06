//! Bài mở rộng MR6.10 – Bảng điều khiển máy chủ đăng ký học phần (egui).
//!
//! Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế
//! độ, nhật ký) ở gui_chung::CuaSoServer; tệp này chỉ vẽ khung "Tài nguyên": mỗi lớp học phần một thanh
//! "đã đăng ký/sĩ số" (đỏ khi đủ; nếu sổ kiểm toán ghi nhiều hơn sĩ số thì hiện thêm "(sổ n)" màu đỏ),
//! tổng số chỗ đã đăng ký, tham số đặt lại (số lớp, sĩ số).
//!
//! Chạy:  server_gui [host] [cổng]        (mặc định 127.0.0.1 44742)

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::{CONG_GUI, DANH_MUC, NV};
use crate::nguoi_ao::SO_LAN_MAC_DINH;
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};

const RONG: f32 = 150.0; // bề rộng thanh sĩ số (điểm ảnh)

pub struct TaiNguyenDangKy {
    o_so_lop: String,
    o_si_so: String,
}

impl TaiNguyenDangKy {
    /// Điền ô tham số (kịch bản demo) – như người dùng gõ.
    pub fn dat_tham_so(&mut self, so_lop: i64, si_so: i64) {
        self.o_so_lop = so_lop.to_string();
        self.o_si_so = si_so.to_string();
    }
}

impl TaiNguyen<NV> for TaiNguyenDangKy {
    const TIEU_DE: &'static str = "Máy chủ đăng ký học phần MR6.10";
    const TEN_TAI_NGUYEN: &'static str = "Tài nguyên: lớp học phần";

    fn moi(ts: &Obj) -> Self {
        TaiNguyenDangKy { o_so_lop: so_us(ts.get("so_lop")), o_si_so: so_us(ts.get("si_so")) }
    }

    fn ve(&mut self, ui: &mut egui::Ui, tt: Option<&Obj>, tk: Option<&Obj>) {
        let tong = match (tt, tk) {
            (Some(_), Some(tk)) => so_us(tk.get("tom_tat")),
            _ => "–".into(),
        };
        ui.label(RichText::new(tong).strong().size(15.0).color(mau("#8b0000")));
        let lop: Vec<Obj> = tt.and_then(|t| t.get("lop")).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_object().cloned()).collect()).unwrap_or_default();
        egui::Grid::new("lop_sv").num_columns(3).spacing([4.0, 2.0]).show(ui, |ui| {
            for i in 0..DANH_MUC.len() {
                match lop.get(i) {
                    Some(x) => {
                        let (da, si, theo_so) = (lay_i64(x, "da_dk").unwrap_or(0), lay_i64(x, "si_so").unwrap_or(1).max(1), lay_i64(x, "theo_so").unwrap_or(0));
                        ui.label(RichText::new(so_us(x.get("ma"))).monospace().size(11.5));
                        let (r, pt) = ui.allocate_painter(egui::vec2(RONG, 14.0), egui::Sense::hover());
                        pt.rect_filled(r.rect, 0.0, mau("#dddddd"));
                        let w = RONG * (da as f32 / si as f32).min(1.0);
                        pt.rect_filled(egui::Rect::from_min_size(r.rect.min, egui::vec2(w, 14.0)), 0.0, if da >= si { DO_DAM } else { mau("#3cb44b") });
                        let vuot = theo_so > si;
                        let chu = format!("{da}/{si}{}", if vuot { format!(" (sổ {theo_so})") } else { String::new() });
                        ui.label(RichText::new(chu).strong().size(11.5).color(if vuot { DO_DAM } else { Color32::BLACK }));
                    }
                    None => {
                        ui.label(RichText::new("").monospace().size(11.5));
                        ui.allocate_space(egui::vec2(RONG, 14.0));
                        ui.label("");
                    }
                }
                ui.end_row();
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Số lớp:");
            o_nhap(ui, &mut self.o_so_lop, 28.0, true);
            ui.label("  Sĩ số:");
            o_nhap(ui, &mut self.o_si_so, 44.0, true);
        });
    }

    fn tham_so_dat_lai(&self) -> Result<Obj, String> {
        let loi = || "tham số phải là số nguyên".to_string();
        Ok(obj(json!({"so_lop": self.o_so_lop.trim().parse::<i64>().map_err(|_| loi())?, "si_so": self.o_si_so.trim().parse::<i64>().map_err(|_| loi())?})))
    }
}

pub type CuaSoServerDangKy = CuaSoServer<NV, TaiNguyenDangKy>;

pub fn tao_cua_so(host: &str, cong: u16) -> CuaSoServerDangKy {
    CuaSoServer::moi(host, cong, "khoa", 2.0, Obj::new(), 2000, 200, SO_LAN_MAC_DINH)
}

pub fn tieu_de() -> String {
    format!("{} – bảng điều khiển", <TaiNguyenDangKy as TaiNguyen<NV>>::TIEU_DE)
}

/// server_gui [host] [cổng]
pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    chay_cua_so(&tieu_de(), [1060.0, 720.0], tao_cua_so(&host, cong));
}
