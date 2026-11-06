//! Bài mở rộng MR6.9 – Vòng quay may mắn (ứng dụng người dùng, egui).
//!
//! Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở gui_chung::CuaSoNguoiDung; tệp này
//! vẽ vòng quay: bánh xe chia ô (mỗi món quà một ô màu, xen giữa là ô "Chúc may mắn lần sau"), kim chỉ ở
//! đỉnh, đèn viền nhấp nháy khi quay, nút QUAY. Bấm QUAY: bánh xe quay tự do ngay, yêu cầu
//! {"loai":"quay"} đi tới máy chủ; khi có kết quả, bánh xe giảm tốc (ease-out) và dừng đúng ô kết quả
//! (món trúng, hoặc một ô "may mắn lần sau"). Bên phải: số quà còn lại, ảnh từng món (vẽ bằng bút vẽ
//! egui) với trạng thái còn / đã trao cho ai, danh sách người trúng trực tiếp (từ cap_nhat ≤ 10 lần/giây).
//!
//! Chạy:  vong_quay_gui <tên> [host] [cổng]       (mặc định 127.0.0.1 44732)

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::{chu_p, CONG_GUI, TEN_QUA};
use eframe::egui::{self, Color32, Pos2, RichText, Stroke};
use serde_json::Value;

pub const MAU_QUA: [&str; 10] = ["#e6194b", "#f58231", "#ffe119", "#3cb44b", "#42d4f4", "#4363d8", "#911eb4", "#f032e6", "#9a6324", "#469990"];
pub const TEN_NGAN: [&str; 10] = ["Xe đạp", "Điện thoại", "Laptop", "Tai nghe", "Đồng hồ", "Máy ảnh", "Ba lô", "Nồi cơm", "Loa", "500k"];

// ===================================================================== bút vẽ kiểu Canvas của Tk
/// Hình bầu dục trong hộp (x0, y0)–(x1, y1): tô `to` (nếu có), viền `vien` (nếu có).
pub fn oval(pt: &egui::Painter, x0: f32, y0: f32, x1: f32, y1: f32, to: Option<Color32>, vien: Option<(f32, Color32)>) {
    let (c, r) = (egui::pos2((x0 + x1) / 2.0, (y0 + y1) / 2.0), egui::vec2((x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0));
    if let Some(m) = to {
        pt.add(egui::Shape::ellipse_filled(c, r, m));
    }
    if let Some((w, m)) = vien {
        pt.add(egui::Shape::ellipse_stroke(c, r, Stroke::new(w, m)));
    }
}

/// Hình chữ nhật (x0, y0)–(x1, y1).
pub fn hcn(pt: &egui::Painter, x0: f32, y0: f32, x1: f32, y1: f32, to: Option<Color32>, vien: Option<(f32, Color32)>) {
    let r = egui::Rect::from_min_max(egui::pos2(x0.min(x1), y0.min(y1)), egui::pos2(x0.max(x1), y0.max(y1)));
    if let Some(m) = to {
        pt.rect_filled(r, 0.0, m);
    }
    if let Some((w, m)) = vien {
        pt.rect_stroke(r, 0.0, Stroke::new(w, m));
    }
}

/// Đường gấp khúc qua các điểm [x0, y0, x1, y1, …].
pub fn duong(pt: &egui::Painter, xy: &[f32], mau_net: Color32, rong: f32) {
    let d: Vec<Pos2> = xy.chunks(2).map(|c| egui::pos2(c[0], c[1])).collect();
    pt.add(egui::Shape::line(d, Stroke::new(rong, mau_net)));
}

/// Cung của hình bầu dục trong hộp, góc bắt đầu `bd` và độ mở `mo` (độ, ngược chiều kim đồng hồ như Tk).
pub fn diem_cung(x0: f32, y0: f32, x1: f32, y1: f32, bd: f32, mo: f32) -> Vec<Pos2> {
    let (cx, cy, rx, ry) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let n = ((mo.abs() / 4.0).ceil() as usize).max(2);
    (0..=n)
        .map(|k| {
            let a = (bd + mo * k as f32 / n as f32).to_radians();
            egui::pos2(cx + rx * a.cos(), cy - ry * a.sin())
        })
        .collect()
}

/// Đa giác lồi [x0, y0, …].
pub fn da_giac(pt: &egui::Painter, xy: &[f32], to: Color32, vien: Option<(f32, Color32)>) {
    let d: Vec<Pos2> = xy.chunks(2).map(|c| egui::pos2(c[0], c[1])).collect();
    pt.add(egui::Shape::convex_polygon(d, to, vien.map_or(Stroke::NONE, |(w, m)| Stroke::new(w, m))));
}

/// Chữ nhiều dòng, canh giữa quanh `tam`, xoay `goc_tk` độ (ngược chiều kim đồng hồ như Tk).
pub fn chu_giua_xoay(pt: &egui::Painter, tam: Pos2, chu: &str, co: f32, mau_chu: Color32, goc_tk: f32) {
    let th = -goc_tk.to_radians(); // egui: góc dương = theo chiều kim đồng hồ
    let dong: Vec<&str> = chu.split('\n').collect();
    let cao = co * 1.2;
    for (k, d) in dong.iter().enumerate() {
        let lech = (k as f32 - (dong.len() as f32 - 1.0) / 2.0) * cao;
        let c = egui::pos2(tam.x - th.sin() * lech, tam.y + th.cos() * lech);
        let g = pt.layout_no_wrap(d.to_string(), egui::FontId::proportional(co), mau_chu);
        let (w, h) = (g.size().x, g.size().y);
        let (cs, sn) = (th.cos(), th.sin());
        let dx = cs * w / 2.0 - sn * h / 2.0;
        let dy = sn * w / 2.0 + cs * h / 2.0;
        pt.add(egui::epaint::TextShape::new(egui::pos2(c.x - dx, c.y - dy), g, mau_chu).with_angle(th));
    }
}

// ===================================================================== ảnh quà (vẽ đơn giản)
/// Vẽ món quà i quanh tâm (cx, cy), nửa cạnh s. xam = true: đã trao (vẽ màu xám).
pub fn ve_qua(pt: &egui::Painter, i: usize, cx: f32, cy: f32, s: f32, xam: bool) {
    let m = if xam { mau("#9a9a9a") } else { mau(MAU_QUA[i]) };
    let den = if xam { mau("#7a7a7a") } else { mau("#303030") };
    match i {
        0 => {
            // xe đạp
            for dx in [-0.55, 0.55] {
                oval(pt, cx + (dx - 0.4) * s, cy - 0.1 * s, cx + (dx + 0.4) * s, cy + 0.7 * s, None, Some((2.0, den)));
            }
            duong(pt, &[cx - 0.55 * s, cy + 0.3 * s, cx - 0.1 * s, cy - 0.35 * s, cx + 0.4 * s, cy - 0.35 * s, cx + 0.55 * s, cy + 0.3 * s], m, 3.0);
            duong(pt, &[cx - 0.1 * s, cy - 0.35 * s, cx, cy + 0.3 * s, cx + 0.4 * s, cy - 0.35 * s], m, 3.0);
            duong(pt, &[cx - 0.25 * s, cy - 0.5 * s, cx + 0.05 * s, cy - 0.5 * s], den, 3.0);
            duong(pt, &[cx + 0.35 * s, cy - 0.6 * s, cx + 0.55 * s, cy - 0.6 * s], den, 3.0);
        }
        1 => {
            // điện thoại
            hcn(pt, cx - 0.42 * s, cy - 0.85 * s, cx + 0.42 * s, cy + 0.85 * s, Some(den), None);
            hcn(pt, cx - 0.34 * s, cy - 0.68 * s, cx + 0.34 * s, cy + 0.6 * s, Some(m), None);
            oval(pt, cx - 0.07 * s, cy + 0.66 * s, cx + 0.07 * s, cy + 0.8 * s, Some(mau("#d0d0d0")), None);
        }
        2 => {
            // laptop
            hcn(pt, cx - 0.7 * s, cy - 0.65 * s, cx + 0.7 * s, cy + 0.3 * s, Some(den), None);
            hcn(pt, cx - 0.6 * s, cy - 0.55 * s, cx + 0.6 * s, cy + 0.2 * s, Some(m), None);
            da_giac(pt, &[cx - 0.7 * s, cy + 0.35 * s, cx + 0.7 * s, cy + 0.35 * s, cx + 0.9 * s, cy + 0.6 * s, cx - 0.9 * s, cy + 0.6 * s], mau("#b0b0b0"), Some((1.0, den)));
        }
        3 => {
            // tai nghe
            let cung = diem_cung(cx - 0.65 * s, cy - 0.75 * s, cx + 0.65 * s, cy + 0.55 * s, 0.0, 180.0);
            pt.add(egui::Shape::line(cung, Stroke::new(3.0_f32, den)));
            for dx in [-0.65, 0.65] {
                hcn(pt, cx + (dx - 0.18) * s, cy - 0.1 * s, cx + (dx + 0.18) * s, cy + 0.6 * s, Some(m), Some((1.0, den)));
            }
        }
        4 => {
            // đồng hồ
            hcn(pt, cx - 0.25 * s, cy - 0.9 * s, cx + 0.25 * s, cy + 0.9 * s, Some(mau("#6b4226")), None);
            oval(pt, cx - 0.55 * s, cy - 0.55 * s, cx + 0.55 * s, cy + 0.55 * s, Some(Color32::WHITE), Some((3.0, m)));
            duong(pt, &[cx, cy, cx, cy - 0.38 * s], den, 2.0);
            duong(pt, &[cx, cy, cx + 0.28 * s, cy + 0.1 * s], den, 2.0);
        }
        5 => {
            // máy ảnh
            hcn(pt, cx - 0.3 * s, cy - 0.65 * s, cx + 0.15 * s, cy - 0.4 * s, Some(den), None);
            hcn(pt, cx - 0.85 * s, cy - 0.45 * s, cx + 0.85 * s, cy + 0.6 * s, Some(m), Some((1.0, den)));
            oval(pt, cx - 0.42 * s, cy - 0.33 * s, cx + 0.42 * s, cy + 0.5 * s, Some(den), Some((1.0, Color32::WHITE)));
            oval(pt, cx - 0.2 * s, cy - 0.12 * s, cx + 0.2 * s, cy + 0.28 * s, Some(mau("#5080c0")), None);
        }
        6 => {
            // ba lô
            let cung = diem_cung(cx - 0.4 * s, cy - 0.95 * s, cx + 0.4 * s, cy - 0.45 * s, 0.0, 180.0);
            pt.add(egui::Shape::line(cung, Stroke::new(2.0_f32, den)));
            hcn(pt, cx - 0.6 * s, cy - 0.7 * s, cx + 0.6 * s, cy + 0.85 * s, Some(m), Some((1.0, den)));
            hcn(pt, cx - 0.4 * s, cy + 0.15 * s, cx + 0.4 * s, cy + 0.65 * s, Some(den), None);
        }
        7 => {
            // nồi cơm điện
            oval(pt, cx - 0.8 * s, cy - 0.55 * s, cx + 0.8 * s, cy - 0.05 * s, Some(mau("#e0e0e0")), Some((1.0, den)));
            hcn(pt, cx - 0.8 * s, cy - 0.3 * s, cx + 0.8 * s, cy + 0.7 * s, Some(m), Some((1.0, den)));
            hcn(pt, cx - 0.15 * s, cy - 0.75 * s, cx + 0.15 * s, cy - 0.5 * s, Some(den), None);
            hcn(pt, cx - 0.25 * s, cy + 0.1 * s, cx + 0.25 * s, cy + 0.4 * s, Some(Color32::WHITE), None);
        }
        8 => {
            // loa
            hcn(pt, cx - 0.55 * s, cy - 0.9 * s, cx + 0.55 * s, cy + 0.9 * s, Some(m), Some((1.0, den)));
            oval(pt, cx - 0.18 * s, cy - 0.72 * s, cx + 0.18 * s, cy - 0.36 * s, Some(den), None);
            oval(pt, cx - 0.42 * s, cy - 0.15 * s, cx + 0.42 * s, cy + 0.69 * s, Some(den), Some((1.0, Color32::WHITE)));
        }
        _ => {
            // phiếu mua hàng 500k
            hcn(pt, cx - 0.9 * s, cy - 0.5 * s, cx + 0.9 * s, cy + 0.5 * s, Some(m), Some((1.0, den)));
            let mut y = cy - 0.5 * s;
            while y < cy + 0.5 * s {
                // đường cắt nét đứt
                duong(pt, &[cx + 0.4 * s, y, cx + 0.4 * s, (y + 2.0).min(cy + 0.5 * s)], Color32::WHITE, 1.0);
                y += 4.0;
            }
            pt.text(egui::pos2(cx - 0.25 * s, cy), egui::Align2::CENTER_CENTER, "500K", egui::FontId::proportional((s * 0.5).max(7.0)), Color32::WHITE);
        }
    }
}

/// Các ô của bánh xe: mã quà hoặc None ("may mắn lần sau"); quà rải đều, tối thiểu 8 ô.
pub fn bo_tri_o(so_qua: usize) -> Vec<Option<usize>> {
    let n = (2 * so_qua).max(8);
    let mut o = vec![None; n];
    for i in 0..so_qua {
        o[round_py(i as f64 * n as f64 / so_qua as f64) as usize % n] = Some(i);
    }
    o
}

/// 12345 -> "12.345".
pub fn nghin(x: i64) -> String {
    let t = tien(x);
    t.trim_end_matches(" đ").to_string()
}

fn so_qua_cua(tt: &Obj) -> usize {
    lay_i64(tt, "so_qua").unwrap_or(10).max(0) as usize
}

fn ds_qua(tt: &Obj) -> Vec<Obj> {
    tt.get("qua").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_object().cloned()).collect()).unwrap_or_default()
}

/// Ô vẽ một món quà (như tk.Canvas nhỏ có viền): nền, viền, ảnh món i (xám nếu đã trao).
pub fn o_anh_qua(ui: &mut egui::Ui, w: f32, h: f32, i: Option<usize>, xam: bool, nen: Color32, vien: Option<(f32, Color32)>) {
    let (r, pt) = ui.allocate_painter(egui::vec2(w, h), egui::Sense::hover());
    pt.rect_filled(r.rect, 0.0, nen);
    if let Some((d, m)) = vien {
        pt.rect_stroke(r.rect.shrink(d / 2.0), 0.0, Stroke::new(d, m));
    }
    if let Some(i) = i {
        ve_qua(&pt, i, r.rect.center().x, r.rect.center().y, (w.min(h) / 2.0 - 3.0).max(4.0), xam);
    }
}

// ===================================================================== cửa sổ
pub struct VongQuay {
    goc: f32, // góc xoay bánh xe (độ, ngược chiều kim đồng hồ)
    pub pha: Option<&'static str>, // None | "tu_do" | "dung"
    tt_ve: Option<Obj>, // trạng thái dùng để vẽ bánh xe
    cho_kq: Option<Obj>, // kết quả chờ đến lúc bánh xe dừng
    pub ket_qua: Option<(String, Color32, Color32)>, // thông báo kết quả cuối
    pub thong_bao: (String, Color32, Color32),
    pub nut_chu: String,
    pub nut_bat: bool,
    t_bam: f64,
    t_khung: f64,
    goc0: f32,
    goc_dich: f32,
    t_dung: f64,
    nhip: u64,
}

impl VongQuay {
    fn bao(&mut self, text: &str, nen: Color32, chu: Color32) {
        self.thong_bao = (text.to_string(), nen, chu);
    }

    /// Bấm QUAY.
    pub fn quay(&mut self, nd: &mut NguoiDung) {
        if nd.kn.is_none() || self.pha.is_some() {
            return;
        }
        self.tt_ve = nd.tt.clone();
        self.pha = Some("tu_do");
        self.t_bam = bay_gio();
        self.t_khung = self.t_bam;
        self.cho_kq = None;
        self.nut_bat = false;
        self.nut_chu = "ĐANG QUAY".into();
        self.bao("Đang quay…", mau("#fff3c4"), Color32::BLACK);
        nd.nhat_ky.ghi("Bấm QUAY");
        nd.gui_yc("quay", Obj::new());
    }

    fn bat_dau_dung(&mut self) {
        let tin = self.cho_kq.clone().unwrap_or_default();
        let o = bo_tri_o(self.tt_ve.as_ref().map_or(10, so_qua_cua));
        let trung = lay_i64(&tin, "qua").map(|q| q as usize);
        let j = match trung.and_then(|q| o.iter().position(|x| *x == Some(q))) {
            Some(j) if dung(&tin, "ok") && dung(&tin, "trung") => j,
            _ => {
                let trong: Vec<usize> = o.iter().enumerate().filter(|(_, x)| x.is_none()).map(|(k, _)| k).collect();
                *chon_ngau_nhien(&trong)
            }
        };
        let ext = 360.0 / o.len() as f32;
        let goc_dich = 90.0 - (j as f32 + 0.5) * ext + ngau_nhien_khoang(-0.3, 0.3) as f32 * ext;
        let m = ((goc_dich - (self.goc - 540.0)) / 360.0).ceil();
        self.goc0 = self.goc;
        self.goc_dich = goc_dich - 360.0 * m;
        self.t_dung = bay_gio();
        self.pha = Some("dung");
    }

    /// Hoạt hình: gọi mỗi lần vẽ (≈ 30 ms một khung khi đang quay).
    fn khung_hinh(&mut self, nd: &mut NguoiDung) {
        let Some(pha) = self.pha else { return };
        let bay = bay_gio();
        self.nhip = ((bay - self.t_bam) / 0.03) as u64;
        if pha == "tu_do" {
            self.goc -= 20.0 * ((bay - self.t_khung) / 0.03) as f32; // quay theo chiều kim đồng hồ
            self.goc = self.goc.rem_euclid(360.0);
            if self.cho_kq.is_some() && bay - self.t_bam > 0.6 {
                self.bat_dau_dung();
            }
        } else {
            let t = ((bay - self.t_dung) / 2.6).min(1.0) as f32;
            self.goc = self.goc0 + (self.goc_dich - self.goc0) * (1.0 - (1.0 - t).powi(3)); // giảm tốc (ease-out)
            if t >= 1.0 {
                self.pha = None;
                self.ket_thuc_quay(nd);
            }
        }
        self.t_khung = bay;
    }

    fn ket_thuc_quay(&mut self, nd: &mut NguoiDung) {
        let tin = self.cho_kq.clone().unwrap_or_default();
        let ok = dung(&tin, "ok");
        let da_quay = lay_str(&tin, "ma_loi") == Some("da_quay");
        let kq = if ok && dung(&tin, "trung") {
            (format!("CHÚC MỪNG! Bạn trúng {}", lay_str(&tin, "ten_qua").unwrap_or("")), mau("#ffd700"), Color32::BLACK)
        } else if ok {
            ("Chúc bạn may mắn lần sau".to_string(), mau("#d0d0ff"), Color32::BLACK)
        } else if da_quay {
            ("Bạn đã quay rồi – mỗi người 1 lượt".to_string(), mau("#ffb347"), Color32::BLACK)
        } else {
            (format!("Lỗi: {}", lay_str(&tin, "ly_do").unwrap_or("")), mau("#ff6060"), Color32::WHITE)
        };
        self.thong_bao = kq.clone();
        nd.nhat_ky.ghi(&format!("Kết quả: {}", kq.0));
        self.ket_qua = Some(kq);
        let xong = ok || da_quay;
        self.nut_chu = if xong { "ĐÃ QUAY" } else { "QUAY" }.into();
        self.nut_bat = !xong;
        if nd.tt.is_some() {
            self.tt_ve = nd.tt.clone();
        }
    }

    fn ve_banh_xe(&self, ui: &mut egui::Ui) {
        const R: f32 = 168.0; // bán kính bánh xe
        let (r, pt) = ui.allocate_painter(egui::vec2(2.0 * R + 40.0, 2.0 * R + 44.0), egui::Sense::hover());
        pt.rect_filled(r.rect, 0.0, mau("#2b1055"));
        let (cx, cy) = (r.rect.min.x + R + 20.0, r.rect.min.y + R + 26.0);
        let tt = self.tt_ve.as_ref();
        let so_qua = tt.map_or(10, so_qua_cua);
        let con: Vec<(i64, bool)> = tt.map(ds_qua).unwrap_or_default().iter().map(|q| (lay_i64(q, "ma").unwrap_or(-1), dung(q, "con"))).collect();
        let con_cua = |ma: usize| con.iter().find(|(m, _)| *m == ma as i64).is_none_or(|x| x.1);
        let o = bo_tri_o(so_qua);
        let ext = 360.0 / o.len() as f32;
        oval(&pt, cx - R - 14.0, cy - R - 14.0, cx + R + 14.0, cy + R + 14.0, Some(mau("#f4b400")), Some((3.0, mau("#a86b00"))));
        for (j, ma) in o.iter().enumerate() {
            let bat_dau = self.goc + j as f32 * ext;
            let (nen, chu, mau_chu) = match ma {
                None => (mau(if j % 2 == 1 { "#fff7e6" } else { "#ffe8f0" }), "May mắn\nlần sau".to_string(), mau("#7a1fa2")),
                Some(m) if con_cua(*m) => (mau(MAU_QUA[*m]), TEN_NGAN[*m].to_string(), if *m != 2 && *m != 4 { Color32::WHITE } else { mau("#202020") }),
                Some(m) => (mau("#bdbdbd"), format!("{}\n(đã trao)", TEN_NGAN[*m]), mau("#606060")),
            };
            let mut lat = vec![egui::pos2(cx, cy)];
            lat.extend(diem_cung(cx - R, cy - R, cx + R, cy + R, bat_dau, ext));
            pt.add(egui::Shape::convex_polygon(lat.clone(), nen, Stroke::NONE));
            pt.add(egui::Shape::closed_line(lat, Stroke::new(2.0_f32, Color32::WHITE)));
            let giua = (bat_dau + ext / 2.0).to_radians();
            let rr = R * 0.62;
            let (x, y) = (cx + rr * giua.cos(), cy - rr * giua.sin());
            let mut goc_chu = (bat_dau + ext / 2.0).rem_euclid(360.0);
            if 90.0 < goc_chu && goc_chu < 270.0 {
                goc_chu -= 180.0; // chữ ở nửa trái: lật cho dễ đọc
            }
            let co = if ma.is_none() || o.len() > 12 { 10.0 } else { 11.0 };
            chu_giua_xoay(&pt, egui::pos2(x, y), &chu, co, mau_chu, goc_chu);
        }
        for k in 0..24 {
            // đèn viền: nhấp nháy khi quay
            let a = (k as f32 * 15.0).to_radians();
            let (x, y) = (cx + (R + 7.0) * a.cos(), cy - (R + 7.0) * a.sin());
            let sang = if self.pha.is_some() { (k + self.nhip / 3) % 2 == 0 } else { k % 2 == 0 };
            pt.circle_filled(egui::pos2(x, y), 4.0, mau(if sang { "#fffde0" } else { "#c08000" }));
        }
        oval(&pt, cx - 30.0, cy - 30.0, cx + 30.0, cy + 30.0, Some(mau("#d62828")), Some((3.0, Color32::WHITE)));
        chu_giua_xoay(&pt, egui::pos2(cx, cy), "MAY\nMẮN", 10.0, Color32::WHITE, 0.0);
        da_giac(&pt, &[cx - 15.0, cy - R - 24.0, cx + 15.0, cy - R - 24.0, cx, cy - R + 12.0], mau("#d62828"), Some((2.0, Color32::WHITE))); // kim chỉ ở đỉnh
    }
}

impl UngDung for VongQuay {
    const TIEU_DE: &'static str = "Vòng quay may mắn";

    fn moi(_ten: &str) -> Self {
        VongQuay {
            goc: 90.0,
            pha: None,
            tt_ve: None,
            cho_kq: None,
            ket_qua: None,
            thong_bao: ("Chưa kết nối".into(), Color32::from_gray(0xe0), Color32::BLACK),
            nut_chu: "QUAY".into(),
            nut_bat: false,
            t_bam: 0.0,
            t_khung: 0.0,
            goc0: 0.0,
            goc_dich: 0.0,
            t_dung: 0.0,
            nhip: 0,
        }
    }

    fn ve(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung) {
        self.khung_hinh(nd);
        if self.pha.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
        }
        let tt = nd.tt.clone();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                self.ve_banh_xe(ui);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let chu = RichText::new(&self.nut_chu).strong().size(19.0).color(Color32::WHITE);
                    if ui.add_enabled(self.nut_bat && nd.kn.is_some(), egui::Button::new(chu).fill(mau("#d62828")).min_size(egui::vec2(128.0, 46.0))).clicked() {
                        self.quay(nd);
                    }
                    let (text, nen, mc) = self.thong_bao.clone();
                    egui::Frame::none().fill(nen).inner_margin(egui::Margin::symmetric(6.0, 4.0)).show(ui, |ui| {
                        ui.set_min_size(egui::vec2(226.0, 36.0));
                        ui.add(egui::Label::new(RichText::new(text).strong().size(13.0).color(mc)).wrap());
                    });
                });
            });
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("VÒNG QUAY MAY MẮN").strong().size(18.0).color(mau("#7a1fa2")));
                let Some(tt) = tt.as_ref() else {
                    ui.label(RichText::new("Mỗi người 1 lượt").size(11.0));
                    return;
                };
                ui.label(RichText::new(format!("Mỗi người 1 lượt · xác suất trúng {} mỗi lượt", chu_p(lay_f64(tt, "p").unwrap_or(0.0)))).size(11.0));
                let con_lai = lay_i64(tt, "con_lai").unwrap_or(0);
                ui.label(RichText::new(format!("Còn {}/{} quà", con_lai, so_us(tt.get("so_qua")))).strong().size(15.0).color(if con_lai > 0 { DO_DAM } else { mau("#606060") }));
                ui.label(RichText::new(format!("Đã quay: {} lượt · {} lượt trúng", nghin(lay_i64(tt, "da_quay").unwrap_or(0)), so_us(tt.get("so_trung")))).size(11.0));
                let qua = ds_qua(tt);
                nhom(ui, "Quà tặng", |ui| {
                    egui::Grid::new(("qua", nd.ten.clone())).num_columns(2).spacing([6.0, 2.0]).show(ui, |ui| {
                        for (i, q) in qua.iter().enumerate() {
                            let con = dung(q, "con");
                            ui.horizontal(|ui| {
                                o_anh_qua(ui, 36.0, 36.0, Some(i), !con, if con { Color32::WHITE } else { mau("#e8e8e8") }, Some((1.0, mau("#808080"))));
                                let chu = format!("{}\n{}", TEN_QUA[i], if con { "còn".to_string() } else { format!("→ {}", q.get("nguoi").filter(|v| !v.is_null()).map(py_str).unwrap_or_else(|| "?".into())) });
                                ui.add_sized([98.0, 34.0], egui::Label::new(RichText::new(chu).size(10.5).color(if con { XANH_DAM } else { mau("#a00000") })));
                            });
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                });
                nhom(ui, "Người trúng thưởng (trực tiếp)", |ui| {
                    let dong: Vec<(String, Color32)> = tt
                        .get("nguoi_trung")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|x| {
                            let ng = x.get("nguoi").map(py_str).unwrap_or_default();
                            let q = x.get("qua").and_then(Value::as_u64).unwrap_or(0) as usize;
                            (format!("{} {:<6} {}", so_us(x.get("gio")), ng, TEN_QUA.get(q).copied().unwrap_or("?")), if ng == nd.ten { mau("#0040c0") } else { Color32::BLACK })
                        })
                        .collect();
                    danh_sach(ui, &dong, 7, 262.0, 11.0);
                });
            });
        });
    }

    fn ket_qua(&mut self, nd: &mut NguoiDung, _loai: &str, _truong: &Obj, tin: &Obj, rtt: f64) {
        let mo_ta = if dung(tin, "ok") {
            if dung(tin, "trung") {
                format!("trúng {}", lay_str(tin, "ten_qua").unwrap_or(""))
            } else {
                "không trúng".into()
            }
        } else {
            format!("từ chối – {}", tin.get("ly_do").map(py_str).unwrap_or_else(|| "None".into()))
        };
        nd.nhat_ky.ghi(&format!("  máy chủ trả lời sau {rtt:.1} ms: {mo_ta}"));
        self.cho_kq = Some(tin.clone());
        if self.pha.is_none() {
            // (không xảy ra khi bấm nút; phòng khi gọi trực tiếp)
            self.ket_thuc_quay(nd);
        }
    }

    fn cap_nhat(&mut self, _nd: &mut NguoiDung, tt: &Obj) {
        if self.pha.is_none() {
            let moi_dat_lai = self.tt_ve.as_ref().is_some_and(|c| lay_i64(tt, "da_quay").unwrap_or(0) < lay_i64(c, "da_quay").unwrap_or(0));
            self.tt_ve = Some(tt.clone());
            if moi_dat_lai || self.ket_qua.is_none() {
                // đặt lại chương trình: được quay lại
                self.ket_qua = None;
                self.nut_chu = "QUAY".into();
                self.nut_bat = true;
                self.bao("Mời bạn bấm QUAY", Color32::from_gray(0xe0), Color32::BLACK);
            }
        }
    }

    fn mat_ket_noi(&mut self, _nd: &mut NguoiDung) {
        self.nut_bat = false;
    }

    fn dong_dinh_ky(&self, nd: &NguoiDung) -> String {
        let tt = nd.tt.clone().unwrap_or_default();
        let kq = self.ket_qua.as_ref().map_or("chưa quay".to_string(), |k| k.0.clone());
        format!("còn {}/{} quà, đã quay {} lượt; bạn: {kq}", so_us(tt.get("con_lai")), so_us(tt.get("so_qua")), so_us(tt.get("da_quay")))
    }
}

/// vong_quay_gui [tên] [host] [cổng]
pub fn run(args: &[String]) {
    let ten = args.first().cloned().unwrap_or_else(|| "An".into());
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    let cs = CuaSoNguoiDung::<VongQuay>::moi(&ten, &host, cong);
    chay_cua_so(&cs.tieu_de(), [720.0, 720.0], cs);
}
