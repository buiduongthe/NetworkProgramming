//! Bài mở rộng MR6.14 – Bảng điều khiển máy chủ đặt vé xe khách theo chặng (egui).
//!
//! Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế
//! độ, nhật ký) ở gui_chung::CuaSoServer; tệp này chỉ vẽ khung "Tài nguyên": sơ đồ xe thu nhỏ (mỗi chỗ có dải
//! vạch theo 3 chặng, ghế nóng viền đỏ), số chỗ-chặng mỗi loại, số chỗ đã bán theo chặng và tham số đặt lại
//! (thời hạn giữ chỗ, loại xe giuong/ghe, ghế nóng của bộ mô phỏng).
//!
//! Chạy:  server_gui [host] [cổng]        (mặc định 127.0.0.1 44782; giữ chỗ 30 s, xe giường nằm)

use crate::chung::*;
use crate::dat_ve_xe::chu_o;
use crate::dat_ve_xe_gui::{dong_cua, SoDoXe, DA_BAN, KHAC_GIU, NEN_XE};
use crate::gui_chung::*;
use crate::may_chu::{CONG_GUI, MA_TRAM, NV, SO_CHANG};
use crate::nguoi_ao::SO_LAN_MAC_DINH;
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

pub struct TaiNguyenDatVeXe {
    so_do: Option<SoDoXe>,
    o_ts: Vec<(&'static str, &'static str, String, f32)>, // (mã, nhãn, giá trị ô, rộng)
    loai_ts: String,                                     // loại xe và ghế nóng máy chủ đang dùng (theo cap_nhat)
    nong_ts: String,
}

impl TaiNguyenDatVeXe {
    /// Điền ô tham số (kịch bản demo) – như người dùng gõ.
    pub fn dat_tham_so(&mut self, ma: &str, gia_tri: &str) {
        if let Some(o) = self.o_ts.iter_mut().find(|o| o.0 == ma) {
            o.2 = gia_tri.to_string();
        }
    }
}

impl TaiNguyen<NV> for TaiNguyenDatVeXe {
    const TIEU_DE: &'static str = "Máy chủ đặt vé xe khách MR6.14";
    const TEN_TAI_NGUYEN: &'static str = "Tài nguyên: chỗ × chặng (Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang)";

    fn moi(ts: &Obj) -> Self {
        let o = |ma: &'static str, nhan: &'static str, rong: f32| (ma, nhan, ts.get(ma).map(|v| if v.is_f64() { fmt_g(v.as_f64().unwrap_or(0.0)) } else { py_str(v) }).unwrap_or_default(), rong);
        TaiNguyenDatVeXe {
            so_do: None,
            o_ts: vec![o("ttl_s", "Giữ chỗ (s)", 44.0), o("loai_xe", "Loại xe", 56.0), o("ghe_nong", "Ghế nóng", 36.0)],
            loai_ts: lay_str(ts, "loai_xe").unwrap_or("giuong").to_string(),
            nong_ts: lay_str(ts, "ghe_nong").unwrap_or("").to_string(),
        }
    }

    fn ve(&mut self, ui: &mut egui::Ui, tt: Option<&Obj>, _tk: Option<&Obj>) {
        if let Some(t) = tt {
            let loai = lay_str(t, "loai_xe").unwrap_or("giuong").to_string();
            let nong = lay_str(t, "ghe_nong").unwrap_or("").to_string();
            if self.so_do.as_ref().is_none_or(|s| s.loai_xe != loai) {
                self.so_do = Some(SoDoXe::new(&loai, 0.5, false));
                self.dat_tham_so("loai_xe", &loai); // đổi loại xe: ô ghế nóng theo máy chủ
                self.dat_tham_so("ghe_nong", &nong);
            }
            (self.loai_ts, self.nong_ts) = (loai, nong);
        }
        ui.horizontal_top(|ui| {
            let (r, pt) = ui.allocate_painter(egui::vec2(184.0, 212.0), egui::Sense::hover());
            pt.rect_filled(r.rect, 0.0, NEN_XE);
            let nong = tt.and_then(|t| lay_str(t, "ghe_nong")).unwrap_or("").to_string();
            let vien: HashMap<String, Color32> = if nong.is_empty() { HashMap::new() } else { [(nong.clone(), mau("#ff3030"))].into() };
            if let Some(sd) = &self.so_do {
                sd.ve(&pt, r.rect.min, tt, None, 0, SO_CHANG, &BTreeSet::new(), &vien);
            }
            ui.vertical(|ui| {
                if let Some(t) = tt {
                    ui.label(RichText::new(format!("{}, chuyến {}", so_us(t.get("ten_xe")), so_us(t.get("chuyen")))).strong().size(12.0));
                }
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
                    let so_do: Vec<String> = t.get("so_do").and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default();
                    let dong: Vec<String> = (0..SO_CHANG)
                        .map(|k| format!("{}-{} {:2}/{}", MA_TRAM[k], MA_TRAM[k + 1], so_do.iter().filter(|d| d.chars().nth(k) == Some('b')).count(), so_do.len()))
                        .collect();
                    ui.label(RichText::new(format!("Đã bán theo chặng:\n{}", dong.join("\n"))).monospace().size(10.5));
                    let chu = if nong.is_empty() {
                        "Ghế nóng (mô phỏng): ngẫu nhiên".to_string()
                    } else {
                        let d: Vec<char> = dong_cua(t, &nong).chars().collect();
                        let p: Vec<String> = (0..SO_CHANG)
                            .filter(|&k| d.get(k).is_some_and(|x| *x != '.'))
                            .map(|k| format!("{}-{}: {}", MA_TRAM[k], MA_TRAM[k + 1], chu_o(t, &format!("{nong}#{k}")).unwrap_or("None")))
                            .collect();
                        format!("Ghế nóng (mô phỏng): {nong}\n{}", if p.is_empty() { "trống cả tuyến".into() } else { p.join(", ") })
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
        let ttl = v("ttl_s").parse::<f64>().map_err(|_| "thời hạn giữ chỗ phải là số".to_string())?;
        let loai = v("loai_xe").to_lowercase();
        let nong = v("ghe_nong").to_uppercase();
        let mut ts = obj(json!({"ttl_s": ttl, "loai_xe": loai}));
        if !(loai != self.loai_ts && nong == self.nong_ts) {
            ts.insert("ghe_nong".into(), json!(nong)); // đổi loại xe mà không sửa ghế nóng: máy chủ tự chọn chỗ đầu
        }
        Ok(ts)
    }
}

pub type CuaSoServerDatVeXe = CuaSoServer<NV, TaiNguyenDatVeXe>;

pub fn tao_cua_so(host: &str, cong: u16) -> CuaSoServerDatVeXe {
    CuaSoServer::moi(host, cong, "khoa", 2.0, Obj::new(), 500, 200, SO_LAN_MAC_DINH)
}

pub fn tieu_de() -> String {
    format!("{} – bảng điều khiển", <TaiNguyenDatVeXe as TaiNguyen<NV>>::TIEU_DE)
}

/// server_gui [host] [cổng]
pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    chay_cua_so(&tieu_de(), [1060.0, 720.0], tao_cua_so(&host, cong));
}
