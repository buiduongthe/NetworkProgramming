//! Bài mở rộng MR6.6 – Phòng đấu giá (ứng dụng người dùng, egui).
//!
//! Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở gui_chung::CuaSoNguoiDung; tệp này
//! vẽ phòng đấu giá: ảnh món hàng (vẽ bằng bút vẽ egui), giá hiện tại cỡ lớn, người dẫn đầu, đồng hồ
//! đếm ngược (đỏ khi còn < 10 s, báo GIA HẠN), các nút "+1/+2/+5 bước", ô nhập giá, lịch sử trả giá,
//! thông báo "Bạn bị trả giá cao hơn!" / "BẠN THẮNG!". Mọi số liệu đến từ cap_nhat của máy chủ (≤ 10
//! lần/giây); đồng hồ đếm ngược chạy cục bộ từ con_lai_ms của lần cap_nhat mới nhất.
//!
//! Chạy:  dau_gia_gui <tên> [host] [cổng]       (mặc định 127.0.0.1 44702)

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::{so_nguyen, CONG_GUI};
use eframe::egui::{self, Color32, Pos2, RichText};
use serde_json::{json, Value};
use std::f32::consts::PI;

pub const LAM: Color32 = Color32::from_rgb(0x1f, 0x4f, 0xa0); // màu men lam (gốm hoa lam)
/// nửa bề rộng của bình theo chiều cao (đơn vị trước khi nhân tỉ lệ): (y, nửa rộng)
const DANG_BINH: [(f32, f32); 12] = [(0.0, 24.0), (6.0, 22.0), (14.0, 15.0), (38.0, 16.0), (52.0, 34.0), (70.0, 50.0), (92.0, 55.0), (118.0, 47.0), (142.0, 32.0), (156.0, 22.0), (162.0, 25.0), (170.0, 25.0)];

fn nua_rong(y: f32) -> f32 {
    for w in DANG_BINH.windows(2) {
        let ((y0, r0), (y1, r1)) = (w[0], w[1]);
        if y0 <= y && y <= y1 {
            return r0 + (r1 - r0) * (y - y0) / (y1 - y0);
        }
    }
    0.0
}

/// Vẽ "Bình gốm hoa lam" vào khung (gốc g, rộng w, cao h): tâm ngang cx, đỉnh miệng ở y = tren, tỉ lệ k.
#[allow(clippy::too_many_arguments)]
pub fn ve_binh_gom(pt: &egui::Painter, g: Pos2, w: f32, h: f32, cx: f32, tren: f32, k: f32, nen: bool) {
    let q = |x: f32, y: f32| p(g, x, y);
    if nen {
        // nền tối + quầng đèn chiếu
        pt.rect_filled(egui::Rect::from_min_size(g, egui::vec2(w, h)), 0.0, mau("#23232f"));
        for (i, m) in ["#2c2c3c", "#363648", "#404056", "#4a4a62"].iter().enumerate() {
            let r = (1.0 - i as f32 * 0.2) * w * 0.5;
            pt.add(egui::Shape::ellipse_filled(q(cx, tren + 80.0 * k), egui::vec2(r, r * 0.9), mau(m)));
        }
        let day = tren + 170.0 * k;
        pt.add(egui::Shape::convex_polygon(
            vec![q(cx - 70.0 * k, day), q(cx + 70.0 * k, day), q(cx + 82.0 * k, day + 22.0 * k), q(cx - 82.0 * k, day + 22.0 * k)],
            mau("#6b4226"),
            egui::Stroke::new(1.0_f32, mau("#3e2412")),
        ));
        pt.rect_filled(egui::Rect::from_min_max(q(cx - 82.0 * k, day + 22.0 * k), q(cx + 82.0 * k, h)), 0.0, mau("#4e2f1a"));
        pt.add(egui::Shape::ellipse_filled(q(cx, day + 0.5 * k), egui::vec2(48.0 * k, 5.5 * k), mau("#1a1a1a")));
    }
    let mut vien: Vec<Pos2> = DANG_BINH.iter().map(|&(y, r)| q(cx - r * k, tren + y * k)).collect();
    vien.extend(DANG_BINH.iter().rev().map(|&(y, r)| q(cx + r * k, tren + y * k)));
    let tron = duong_tron(&vien, true);
    to_da_giac(pt, &tron, mau("#f5f2e8"));
    pt.add(egui::Shape::closed_line(tron, egui::Stroke::new(1.0_f32, mau("#8c96a8"))));
    pt.add(egui::Shape::ellipse_filled(q(cx, tren + 0.5 * k), egui::vec2(24.0 * k, 4.5 * k), mau("#dcd8cc")));
    pt.add(egui::Shape::ellipse_stroke(q(cx, tren + 0.5 * k), egui::vec2(24.0 * k, 4.5 * k), egui::Stroke::new(1.0_f32, mau("#8c96a8"))));
    for y in [10.0, 36.0, 54.0, 60.0, 138.0, 150.0] {
        // đường viền men lam quanh thân ở độ cao y
        let r = nua_rong(y) * k - 1.0;
        let day = if y == 54.0 || y == 138.0 { (2.0 * k).max(1.0) } else { 1.0 };
        pt.line_segment([q(cx - r, tren + y * k), q(cx + r, tren + y * k)], egui::Stroke::new(day, LAM));
    }
    for i in -3..=3 {
        // cánh sen quanh cổ
        let x = cx + i as f32 * 9.0 * k;
        let cung: Vec<Pos2> = (0..=12).map(|j| {
            let t = PI * j as f32 / 12.0;
            q(x + 5.0 * k * t.cos(), tren + 28.0 * k - 10.0 * k * t.sin())
        }).collect();
        pt.add(egui::Shape::line(cung, egui::Stroke::new(1.0_f32, LAM)));
    }
    let tam_y = tren + 96.0 * k; // bông hoa giữa thân
    for i in 0..8 {
        let a = i as f32 * PI / 4.0;
        let c = q(cx + 15.0 * k * a.cos(), tam_y + 15.0 * k * a.sin());
        pt.circle(c, 7.0 * k, mau("#5b82c4"), egui::Stroke::new(1.0_f32, LAM));
    }
    pt.circle_filled(q(cx, tam_y), 8.0 * k, LAM);
    for d in [-1.0f32, 1.0] {
        // dây lá cuốn hai bên
        let dk = [q(cx + d * 24.0 * k, tam_y), q(cx + d * 34.0 * k, tam_y - 16.0 * k), q(cx + d * 44.0 * k, tam_y - 4.0 * k), q(cx + d * 40.0 * k, tam_y + 14.0 * k), q(cx + d * 30.0 * k, tam_y + 22.0 * k)];
        pt.add(egui::Shape::line(duong_tron(&dk, false), egui::Stroke::new(2.0_f32, LAM)));
        pt.circle_filled(q(cx + d * 40.0 * k, tam_y - 8.0 * k), 4.0 * k, LAM);
    }
    for i in -4..=4 {
        // cánh sen chân bình
        let x = cx + i as f32 * 8.0 * k;
        pt.add(egui::Shape::line(vec![q(x - 4.0 * k, tren + 150.0 * k), q(x, tren + 140.0 * k), q(x + 4.0 * k, tren + 150.0 * k)], egui::Stroke::new(1.0_f32, LAM)));
    }
}

/// Đếm ngược dạng mm:ss (dưới 10 s thêm phần mười).
pub fn dong_ho(giay: f64) -> String {
    if giay < 10.0 {
        return format!("00:{giay:04.1}");
    }
    let giay = giay as i64;
    format!("{:02}:{:02}", giay / 60, giay % 60)
}

pub fn ten_mon(tt: &Obj) -> String {
    tt.get("mon").and_then(|m| m.get("ten")).map(py_str).unwrap_or_default()
}

pub fn buoc_gia(tt: &Obj) -> i64 {
    so_nguyen(tt.get("mon").and_then(|m| m.get("buoc_gia")))
}

pub struct PhongDauGia {
    pub het_han: Option<f64>, // thời điểm (bay_gio) hết phiên theo cap_nhat
    so_lan_cu: i64,
    dan_dau: bool,
    pub so_tra: u32,
    pub so_nhan: u32,
    han_bao_gia_han: f64,
    pub thong_bao: (String, Color32, Color32),
    pub dong_ho: (String, Color32),
    pub gia_han: String,
    o_gia: String,
    nut_bat: bool,
}

impl PhongDauGia {
    fn bao(&mut self, text: &str, nen: Color32, chu: Color32) {
        self.thong_bao = (text.to_string(), nen, chu);
    }

    /// Bấm "+n bước".
    pub fn tang(&mut self, nd: &mut NguoiDung, n: i64) {
        if let Some(tt) = &nd.tt {
            let gia = lay_i64(tt, "gia").unwrap_or(0) + n * buoc_gia(tt);
            self.tra(nd, gia);
        }
    }

    fn tra_o_nhap(&mut self, nd: &mut NguoiDung) {
        let s: String = self.o_gia.chars().filter(|c| !matches!(c, '.' | ' ' | 'đ')).collect();
        match s.parse::<i64>() {
            Ok(gia) => self.tra(nd, gia),
            Err(_) => nd.nhat_ky.ghi("Giá phải là số nguyên (đồng)"),
        }
    }

    fn tra(&mut self, nd: &mut NguoiDung, gia: i64) {
        self.so_tra += 1;
        nd.nhat_ky.ghi(&format!("Trả {}…", tien(gia)));
        nd.gui_yc("tra_gia", obj(json!({"gia": gia})));
    }
}

impl UngDung for PhongDauGia {
    const TIEU_DE: &'static str = "Phòng đấu giá";

    fn moi(_ten: &str) -> Self {
        PhongDauGia {
            het_han: None,
            so_lan_cu: 0,
            dan_dau: false,
            so_tra: 0,
            so_nhan: 0,
            han_bao_gia_han: 0.0,
            thong_bao: ("Chưa kết nối".into(), Color32::from_gray(0xe0), Color32::BLACK),
            dong_ho: ("–".into(), Color32::BLACK),
            gia_han: String::new(),
            o_gia: String::new(),
            nut_bat: false,
        }
    }

    fn ve(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung) {
        let tt = nd.tt.clone();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(232.0);
                let (r, pt) = ui.allocate_painter(egui::vec2(230.0, 240.0), egui::Sense::hover());
                ve_binh_gom(&pt, r.rect.min, 230.0, 240.0, 115.0, 26.0, 1.0, true);
                let mon = tt.as_ref().map(ten_mon).unwrap_or_else(|| "–".into());
                ui.label(RichText::new(mon).strong().size(14.0));
                if let Some(tt) = &tt {
                    let gkd = so_nguyen(tt.get("mon").and_then(|m| m.get("gia_khoi_diem")));
                    ui.label(RichText::new(format!("Khởi điểm {} · bước {}", tien(gkd), tien(buoc_gia(tt)))).size(11.5));
                }
                let cua_toi = if self.so_tra == 0 { "Bạn: chưa trả giá".to_string() } else { format!("Bạn: trả {} lần, được nhận {}", self.so_tra, self.so_nhan) };
                ui.label(RichText::new(cua_toi).size(11.5));
            });
            ui.vertical(|ui| {
                ui.set_width(440.0);
                ui.label(RichText::new("GIÁ HIỆN TẠI").strong().size(11.5).color(mau("#404040")));
                let gia = tt.as_ref().and_then(|t| lay_i64(t, "gia"));
                ui.label(RichText::new(gia.map_or("–".into(), tien)).strong().size(30.0).color(mau("#8b0000")));
                let dd = tt.as_ref().map_or("Dẫn đầu: –".into(), |t| format!("Dẫn đầu: {}  ({} lượt trả)", lay_str(t, "nguoi_dan_dau").unwrap_or("–"), so_us(t.get("so_lan"))));
                ui.label(RichText::new(dd).size(13.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Còn lại ").size(13.0));
                    ui.label(RichText::new(&self.dong_ho.0).monospace().strong().size(24.0).color(self.dong_ho.1));
                    ui.label(RichText::new(&self.gia_han).strong().size(13.0).color(DO_DAM));
                });
                nhan_nen(ui, &self.thong_bao.0.clone(), self.thong_bao.1, self.thong_bao.2, 14.0, 410.0);
                ui.horizontal(|ui| {
                    for n in [1, 2, 5] {
                        let chu = match &tt {
                            Some(t) => format!("+{n} bước\n{}", tien(lay_i64(t, "gia").unwrap_or(0) + n * buoc_gia(t))),
                            None => format!("+{n} bước\n"),
                        };
                        if nut(ui, &chu, if n == 1 { CAM } else { XANH_DUONG }, self.nut_bat, 96.0) {
                            self.tang(nd, n);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Giá của bạn:");
                    o_nhap(ui, &mut self.o_gia, 110.0, true);
                    if nut(ui, "Trả giá", mau("#ff8c00"), self.nut_bat, 70.0) {
                        self.tra_o_nhap(nd);
                    }
                });
                nhom(ui, "Lịch sử trả giá (mới nhất trên cùng)", |ui| {
                    let dong: Vec<(String, Color32)> = tt
                        .as_ref()
                        .and_then(|t| t.get("lich_su"))
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .map(|x| {
                                    let ng = x.get("nguoi").map(py_str).unwrap_or_default();
                                    let gh = if x.get("gia_han").and_then(Value::as_bool).unwrap_or(false) { " gia hạn" } else { "" };
                                    let d = format!("{} {:<6} {:>14}{gh}", so_us(x.get("gio")), ng, tien(so_nguyen(x.get("gia"))));
                                    (d, if ng == nd.ten { mau("#0040c0") } else { Color32::BLACK })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    danh_sach(ui, &dong, 8, 420.0, 12.0);
                });
            });
        });
    }

    fn ket_qua(&mut self, nd: &mut NguoiDung, _loai: &str, truong: &Obj, tin: &Obj, rtt: f64) {
        let gia = tien(lay_i64(truong, "gia").unwrap_or(0));
        if dung(tin, "ok") {
            self.so_nhan += 1;
            nd.nhat_ky.ghi(&format!("  chấp nhận {gia} ({rtt:.1} ms){}", if dung(tin, "gia_han") { " – phiên được GIA HẠN" } else { "" }));
            self.bao("Bạn đang dẫn đầu", mau("#70d070"), Color32::BLACK);
            self.dan_dau = true; // cap_nhat sau đó báo nếu bị vượt (hoặc bị ghi đè)
        } else {
            let ly_do = so_us(tin.get("ly_do"));
            nd.nhat_ky.ghi(&format!("  từ chối {gia}: {ly_do}"));
            let chu = if lay_str(tin, "ma_loi") == Some("gia_thap") { "giá đã thay đổi".to_string() } else { ly_do };
            self.bao(&format!("Bị từ chối: {chu}"), mau("#ffb347"), Color32::BLACK);
        }
    }

    fn cap_nhat(&mut self, nd: &mut NguoiDung, tt: &Obj) {
        let so_lan = lay_i64(tt, "so_lan").unwrap_or(0);
        let ket_thuc = dung(tt, "ket_thuc");
        let con = lay_f64(tt, "con_lai_ms");
        let moi = con.map(|ms| bay_gio() + ms / 1000.0);
        if let (Some(m), Some(h)) = (moi, self.het_han) {
            if m > h + 0.5 && !ket_thuc && so_lan > self.so_lan_cu {
                // hạn lùi ra sau vì có giá mới (không phải đặt lại)
                self.han_bao_gia_han = bay_gio() + 3.0;
                nd.nhat_ky.ghi(&format!("GIA HẠN: còn {:.1} s", con.unwrap_or(0.0) / 1000.0));
            }
        }
        self.het_han = moi;
        self.so_lan_cu = so_lan;
        let ai = lay_str(tt, "nguoi_dan_dau").map(str::to_string);
        let cua_toi = ai.as_deref() == Some(nd.ten.as_str());
        let gia = tien(lay_i64(tt, "gia").unwrap_or(0));
        if ket_thuc {
            self.nut_bat = false;
            if cua_toi {
                self.bao(&format!("BẠN THẮNG! {gia}"), mau("#ffd700"), Color32::BLACK);
            } else {
                self.bao(&format!("Kết thúc – {} thắng", ai.as_deref().unwrap_or("không ai")), mau("#909090"), Color32::WHITE);
            }
        } else {
            self.nut_bat = true;
            if self.dan_dau && !cua_toi {
                let ai = ai.as_deref().unwrap_or("None");
                self.bao(&format!("Bạn bị trả giá cao hơn! ({ai})"), mau("#ff6060"), Color32::WHITE);
                nd.nhat_ky.ghi(&format!("Bạn bị trả giá cao hơn: {ai} {gia}"));
            } else if cua_toi {
                self.bao("Bạn đang dẫn đầu", mau("#70d070"), Color32::BLACK);
            } else if so_lan == 0 || self.thong_bao.0 == "Chưa kết nối" {
                self.bao("Phiên đang mở – mời trả giá", Color32::from_gray(0xe0), Color32::BLACK);
            }
        }
        self.dan_dau = cua_toi && !ket_thuc;
        self.moi_nhip(nd);
    }

    fn moi_nhip(&mut self, nd: &mut NguoiDung) {
        let Some(tt) = &nd.tt else { return };
        self.dong_ho = if dung(tt, "ket_thuc") {
            ("00:00".into(), mau("#606060"))
        } else {
            match self.het_han {
                None => ("∞".into(), Color32::BLACK),
                Some(h) => {
                    let con = (h - bay_gio()).max(0.0);
                    (dong_ho(con), if con < 10.0 { DO_DAM } else { XANH_DAM })
                }
            }
        };
        self.gia_han = if bay_gio() < self.han_bao_gia_han { "GIA HẠN!".into() } else { String::new() };
    }

    fn dong_dinh_ky(&self, nd: &NguoiDung) -> String {
        let tt = nd.tt.clone().unwrap_or_default();
        let con = self.het_han.map_or("∞".into(), |h| format!("{:.1} s", (h - bay_gio()).max(0.0)));
        format!(
            "giá {}, dẫn đầu {}, còn {con}, {}; bạn trả {}, nhận {}",
            tien(lay_i64(&tt, "gia").unwrap_or(0)),
            lay_str(&tt, "nguoi_dan_dau").unwrap_or("–"),
            if dung(&tt, "ket_thuc") { "KẾT THÚC" } else { "đang đấu" },
            self.so_tra,
            self.so_nhan
        )
    }
}

/// dau_gia_gui [tên] [host] [cổng]
pub fn run(args: &[String]) {
    let ten = args.first().cloned().unwrap_or_else(|| "An".into());
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    let cs = CuaSoNguoiDung::<PhongDauGia>::moi(&ten, &host, cong);
    chay_cua_so(&cs.tieu_de(), [720.0, 640.0], cs);
}
