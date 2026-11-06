//! Bài mở rộng MR6.7 – Ứng dụng gọi xe (người dùng, egui).
//!
//! Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở gui_chung::CuaSoNguoiDung; tệp này
//! vẽ ứng dụng gọi xe: bản đồ phố (đường, công viên, hồ, tên phố), điểm đón (xanh) và điểm đến (đỏ) –
//! bấm chuột trái/phải lên bản đồ để chọn ngã tư –, tuyến đường, các xe (màu theo xe, xe đã có khách mờ
//! đi), giá cước dự kiến, nút ĐẶT XE, trạng thái "Đang tìm tài xế…" / "Đặt thành công – tài xế …" /
//! "Hết chỗ" / "Tài xế … đã nhận khách khác!", thẻ tài xế và nút Hủy chuyến. Đặt được thì xe chạy hoạt
//! hình tới điểm đón rồi tới điểm đến. Mọi số liệu về xe đến từ cap_nhat của máy chủ.
//!
//! Chạy:  dat_xe_gui <tên> [host] [cổng]       (mặc định 127.0.0.1 44712)

use crate::chung::*;
use crate::dat_xe::{diem_mac_dinh, xe_ma};
use crate::gui_chung::*;
use crate::may_chu::{cuoc, diem_truong, json_diem, km, nga_tu, ten_diem, Diem, CONG_GUI, DUONG, TEN_DOC, TEN_NGANG};
use eframe::egui::{self, Color32, Pos2, RichText};
use serde_json::{json, Value};
use std::f32::consts::FRAC_PI_2;

pub const XANH_GRAB: Color32 = Color32::from_rgb(0x00, 0xa6, 0x51);
pub const XANH_TUYEN: Color32 = Color32::from_rgb(0x1a, 0x73, 0xe8);
pub const DO_PIN: Color32 = Color32::from_rgb(0xd9, 0x30, 0x25);
const TOC_DO: f64 = 0.28; // đơn vị bản đồ / giây (hoạt hình)

pub fn mau_xe(ten: &str) -> Color32 {
    mau(match ten {
        "trắng" => "#f4f4f4",
        "đỏ" => "#e53935",
        "xanh" => "#1e88e5",
        "bạc" => "#b0bec5",
        "vàng" => "#fdd835",
        "đen" => "#37474f",
        "cam" => "#fb8c00",
        "tím" => "#8e24aa",
        "xám" => "#78909c",
        "hồng" => "#f06292",
        _ => "#f4f4f4",
    })
}

/// Đường đi trên lưới phố từ a tới b: đi ngang trước rồi đi dọc.
pub fn duong_di(a: Diem, b: Diem) -> Vec<Diem> {
    let mut ds = vec![a];
    if a.0 != b.0 && a.1 != b.1 {
        ds.push((b.0, a.1));
    }
    ds.push(b);
    ds
}

pub fn do_dai(ds: &[Diem]) -> f64 {
    ds.windows(2).map(|w| (w[0].0 - w[1].0).abs() + (w[0].1 - w[1].1).abs()).sum()
}

/// Điểm cách đầu đường đi một đoạn s (đơn vị bản đồ) và hướng (true = ngang).
pub fn diem_tren(ds: &[Diem], mut s: f64) -> (Diem, bool) {
    for (i, w) in ds.windows(2).enumerate() {
        let (p, q) = (w[0], w[1]);
        let d = (p.0 - q.0).abs() + (p.1 - q.1).abs();
        if s <= d || i == ds.len() - 2 {
            let t = if d == 0.0 { 0.0 } else { (s / d).min(1.0) };
            return ((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t), p.1 == q.1);
        }
        s -= d;
    }
    (*ds.last().unwrap(), true)
}

/// Phần đường đi từ khoảng cách s0 tới s1 (danh sách điểm).
pub fn doan(ds: &[Diem], s0: f64, s1: f64) -> Vec<Diem> {
    let mut kq = vec![diem_tren(ds, s0).0];
    let mut tich = 0.0;
    for w in ds.windows(2) {
        tich += (w[0].0 - w[1].0).abs() + (w[0].1 - w[1].1).abs();
        if s0 < tich && tich < s1 {
            kq.push(w[1]);
        }
    }
    kq.push(diem_tren(ds, s1).0);
    kq
}

/// Bản đồ phố (dùng cả ở ứng dụng gọi xe và bảng điều khiển máy chủ).
pub struct BanDo<'a> {
    pub pt: &'a egui::Painter,
    pub g: Pos2,
    pub w: f32,
    pub h: f32,
    pub nho: bool,
}

impl BanDo<'_> {
    pub fn p(&self, x: f64, y: f64) -> Pos2 {
        egui::pos2(self.g.x + x as f32 * self.w, self.g.y + y as f32 * self.h)
    }

    /// Hình chữ nhật của khu phố giữa các đường (thụt vào khỏi lòng đường).
    fn o(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> egui::Rect {
        let le = 0.022;
        egui::Rect::from_min_max(self.p(x0 + le, y0 + le), self.p(x1 - le, y1 - le))
    }

    pub fn ve_nen(&self) {
        let pt = self.pt;
        pt.rect_filled(egui::Rect::from_min_size(self.g, egui::vec2(self.w, self.h)), 0.0, mau("#ebe7df"));
        let mut bien = vec![0.0];
        bien.extend(DUONG);
        bien.push(1.0);
        for (i, x) in bien.windows(2).enumerate() {
            // khu phố: nhà (ô nhỏ)
            for (j, y) in bien.windows(2).enumerate() {
                let r = self.o(x[0], y[0], x[1], y[1]);
                if r.width() < 6.0 || r.height() < 6.0 {
                    continue;
                }
                pt.rect_filled(r, 0.0, mau("#e2ddd2"));
                if !self.nho && (i + j) % 2 == 0 {
                    pt.rect_filled(egui::Rect::from_min_max(r.min + egui::vec2(4.0, 4.0), egui::pos2(r.center().x - 2.0, r.center().y - 2.0)), 0.0, mau("#d6d0c2"));
                }
            }
        }
        let r = self.o(0.3, 0.7, 0.5, 0.9); // công viên
        pt.rect_filled(r, 0.0, mau("#bfe3a6"));
        for k in 0..5 {
            let c = egui::pos2(r.min.x + r.width() * (0.2 + 0.15 * k as f32), r.min.y + r.height() * if k % 2 == 1 { 0.35 } else { 0.65 });
            pt.circle_filled(c, if self.nho { 3.0 } else { 6.0 }, mau("#7cbf5a"));
        }
        if !self.nho {
            pt.text(egui::pos2(r.center().x, r.max.y - 7.0), egui::Align2::CENTER_CENTER, "Công viên", egui::FontId::proportional(9.5), mau("#3d7a2a"));
        }
        let r = self.o(0.7, 0.1, 0.9, 0.3); // hồ
        pt.add(egui::Shape::ellipse_filled(r.center(), r.size() / 2.0, mau("#a9d3f2")));
        pt.add(egui::Shape::ellipse_stroke(r.center(), r.size() / 2.0, egui::Stroke::new(1.0_f32, mau("#8cc2ea"))));
        if !self.nho {
            pt.text(r.center(), egui::Align2::CENTER_CENTER, "Hồ", egui::FontId::proportional(9.5), mau("#2a6fa8"));
        }
        let rong: f32 = if self.nho { 7.0 } else { 12.0 };
        for x in DUONG {
            // đường dọc và ngang
            let chinh = x == 0.5;
            let net = egui::Stroke::new(rong + if chinh { 2.0 } else { 0.0 }, if chinh { mau("#fde293") } else { Color32::WHITE });
            pt.line_segment([self.p(x, 0.0), self.p(x, 1.0)], net);
            pt.line_segment([self.p(0.0, x), self.p(1.0, x)], net);
        }
        if !self.nho {
            for (i, (x, ten)) in DUONG.iter().zip(TEN_DOC).enumerate() {
                // tên phố: giữa hai ngã tư
                let c = self.p(*x, if i % 2 == 1 { 0.4 } else { 0.6 }) + egui::vec2(10.0, 0.0);
                chu_xoay(self.pt, c, ten, 9.5, mau("#7a7468"), FRAC_PI_2);
            }
            for (i, (y, ten)) in DUONG.iter().zip(TEN_NGANG).enumerate() {
                let c = self.p(if i % 2 == 1 { 0.4 } else { 0.6 }, *y) - egui::vec2(0.0, 10.0);
                pt.text(c, egui::Align2::CENTER_CENTER, ten, egui::FontId::proportional(9.5), mau("#7a7468"));
            }
        }
    }

    pub fn tuyen(&self, ds: &[Diem], mau_tuyen: Color32, net: bool) {
        if ds.len() < 2 {
            return;
        }
        let diem: Vec<Pos2> = ds.iter().map(|d| self.p(d.0, d.1)).collect();
        let rong: f32 = if self.nho { 3.0 } else { 5.0 };
        if net {
            self.pt.extend(egui::Shape::dashed_line(&diem, egui::Stroke::new(rong, mau_tuyen), 4.0 * rong / 2.5, 3.0 * rong / 2.5));
        } else {
            self.pt.add(egui::Shape::line(diem.clone(), egui::Stroke::new(rong + 2.0, mau("#0b4fb3"))));
            self.pt.add(egui::Shape::line(diem, egui::Stroke::new(rong, mau_tuyen)));
        }
    }

    pub fn ghim(&self, d: Diem, mau_ghim: Color32, chu: &str) {
        let c = self.p(d.0, d.1);
        let r = if self.nho { 5.0 } else { 9.0 };
        self.pt.add(egui::Shape::convex_polygon(vec![egui::pos2(c.x - r * 0.8, c.y - r * 1.4), egui::pos2(c.x + r * 0.8, c.y - r * 1.4), c], mau_ghim, egui::Stroke::NONE));
        self.pt.circle(egui::pos2(c.x, c.y - r * 1.6), r, mau_ghim, egui::Stroke::new(2.0_f32, Color32::WHITE));
        self.pt.circle_filled(egui::pos2(c.x, c.y - r * 1.6), r * 0.35, Color32::WHITE);
        if !chu.is_empty() && !self.nho {
            self.pt.text(egui::pos2(c.x + r + 3.0, c.y - r * 1.6), egui::Align2::LEFT_CENTER, chu, egui::FontId::proportional(11.0), mau_ghim);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn xe(&self, d: Diem, mau_ten: &str, nhan_xe: &str, ngang: bool, mo: bool, noi_bat: bool) {
        let c = self.p(d.0, d.1);
        let (dai, ngan) = if self.nho { (14.0, 8.0) } else { (24.0, 13.0) };
        let (rx, ry) = if ngang { (dai / 2.0, ngan / 2.0) } else { (ngan / 2.0, dai / 2.0) };
        let pt = self.pt;
        if noi_bat {
            pt.circle_stroke(c, dai * 0.85, egui::Stroke::new(3.0_f32, XANH_GRAB));
        }
        let than = if mo { mau("#d0d0d0") } else { mau_xe(mau_ten) };
        let r = |x0: f32, y0: f32, x1: f32, y1: f32| egui::Rect::from_min_max(egui::pos2(c.x + x0, c.y + y0), egui::pos2(c.x + x1, c.y + y1));
        pt.rect_filled(r(-rx + 1.0, -ry + 2.0, rx + 1.0, ry + 2.0), 0.0, mau("#9a9488"));
        pt.rect_filled(r(-rx, -ry, rx, ry), 0.0, than);
        pt.rect_stroke(r(-rx, -ry, rx, ry), 0.0, egui::Stroke::new(1.0_f32, if mo { mau("#555555") } else { mau("#202020") }));
        let kinh = mau("#5c6f80");
        if ngang {
            // kính lái, kính sau
            pt.rect_filled(r(rx * 0.25, -ry + 2.0, rx * 0.55, ry - 2.0), 0.0, kinh);
            pt.rect_filled(r(-rx * 0.7, -ry + 2.0, -rx * 0.45, ry - 2.0), 0.0, kinh);
        } else {
            pt.rect_filled(r(-rx + 2.0, ry * 0.25, rx - 2.0, ry * 0.55), 0.0, kinh);
            pt.rect_filled(r(-rx + 2.0, -ry * 0.7, rx - 2.0, -ry * 0.45), 0.0, kinh);
        }
        if !nhan_xe.is_empty() {
            let co = if self.nho { 9.0 } else { 10.5 };
            pt.text(egui::pos2(c.x, c.y - rx.max(ry) - 6.0), egui::Align2::CENTER_CENTER, nhan_xe, egui::FontId::proportional(co), if mo { mau("#707070") } else { mau("#202020") });
        }
    }

    /// Vẽ các xe của cap_nhat (xe đã có khách mờ đi).
    pub fn cac_xe(&self, tt: &Obj, bo_qua: Option<i64>, nhan_khach: bool) {
        for x in tt.get("xe").and_then(Value::as_array).into_iter().flatten() {
            if bo_qua.is_some() && x.get("ma").and_then(Value::as_i64) == bo_qua {
                continue;
            }
            let khach = x.get("khach").and_then(Value::as_str);
            let nhan = match (nhan_khach, khach) {
                (true, Some(k)) => k.to_string(),
                _ => so_us(x.get("tai_xe")),
            };
            self.xe(diem_truong(x, "vi_tri").unwrap_or((0.5, 0.5)), x.get("mau").and_then(Value::as_str).unwrap_or(""), &nhan, true, khach.is_some(), false);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Trang {
    SanSang,
    DangTim,
    DaDat,
    DangHuy,
    HetCho,
    MatXe,
    Xong,
}

impl Trang {
    pub fn ten(&self) -> &'static str {
        match self {
            Trang::SanSang => "san_sang",
            Trang::DangTim => "dang_tim",
            Trang::DaDat => "da_dat",
            Trang::DangHuy => "dang_huy",
            Trang::HetCho => "het_cho",
            Trang::MatXe => "mat_xe",
            Trang::Xong => "xong",
        }
    }
}

/// Hoạt hình: (đường đi, thời điểm bắt đầu, độ dài, mốc tới điểm đón).
type HoatHinh = (Vec<Diem>, f64, f64, f64);

pub struct UngDungGoiXe {
    pub don: Diem,
    pub den: Diem,
    pub trang: Trang,
    pub xe_toi: Option<i64>,     // mã xe được báo đặt thành công
    pub chuyen: Option<Obj>,     // thông tin chuyến từ kq
    phien_toi: i64,              // số thứ tự lần ghi (kq) của chuyến mình đặt
    pub hoat_hinh: Option<HoatHinh>,
    so_dat: u32,
    so_duoc: u32,
    t_tim: f64,
    pub nhan_tt: (String, Color32, Color32),
    pha: Option<&'static str>,
}

impl UngDungGoiXe {
    fn bao(&mut self, text: &str, nen: Color32, chu: Color32) {
        self.nhan_tt = (text.to_string(), nen, chu);
    }

    pub fn da_toi(&self) -> bool {
        self.hoat_hinh.as_ref().is_some_and(|(_, t0, dai, _)| (bay_gio() - t0) * TOC_DO >= *dai)
    }

    fn co_the_dat(&self, nd: &NguoiDung) -> bool {
        nd.kn.is_some() && matches!(self.trang, Trang::SanSang | Trang::HetCho | Trang::MatXe | Trang::Xong)
    }

    // ------------------------------------------------------------ đặt / hủy
    pub fn dat_xe(&mut self, nd: &mut NguoiDung) {
        if nd.kn.is_none() || matches!(self.trang, Trang::DangTim | Trang::DaDat) {
            return;
        }
        self.trang = Trang::DangTim;
        self.so_dat += 1;
        self.t_tim = bay_gio();
        self.bao("Đang tìm tài xế…", mau("#fff3c4"), Color32::BLACK);
        nd.nhat_ky.ghi(&format!("Đặt xe: {} -> {}", ten_diem(self.don), ten_diem(self.den)));
        nd.gui_yc("dat_xe", obj(json!({"don": json_diem(self.don), "den": json_diem(self.den)})));
    }

    pub fn huy(&mut self, nd: &mut NguoiDung) {
        if self.trang == Trang::DaDat {
            self.trang = Trang::DangHuy;
            nd.gui_yc("huy", Obj::new());
        }
    }

    /// Chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq). Xe mình được báo đặt
    /// thành công mà lại ghi khách khác: bị "cướp" chỗ (tranh chấp); xe trống: máy chủ đặt lại.
    fn kiem_xe(&mut self, nd: &mut NguoiDung) {
        let Some(tt) = nd.tt.clone() else { return };
        let Some(ma) = self.xe_toi else { return };
        if self.trang != Trang::DaDat || lay_i64(&tt, "phien").unwrap_or(0) < self.phien_toi {
            return;
        }
        let xe = xe_ma(&tt, ma);
        let khach = xe.and_then(|x| x.get("khach")).and_then(Value::as_str);
        if xe.is_some() && khach == Some(nd.ten.as_str()) {
            return;
        }
        if let (Some(x), Some(k)) = (xe, khach) {
            let tx = so_us(x.get("tai_xe"));
            self.trang = Trang::MatXe;
            self.hoat_hinh = None;
            self.bao(&format!("Tài xế {tx} đã nhận khách khác ({k})!"), mau("#ff6060"), Color32::WHITE);
            nd.nhat_ky.ghi(&format!("Tài xế {tx} đã nhận khách khác: {k}"));
        } else {
            self.trang = Trang::SanSang;
            self.xe_toi = None;
            self.chuyen = None;
            self.hoat_hinh = None;
            self.bao("Chuyến đã kết thúc (máy chủ đặt lại)", mau("#e8e8e8"), Color32::BLACK);
        }
    }

    fn chon(&mut self, x: f32, y: f32, la_don: bool) {
        if matches!(self.trang, Trang::DangTim | Trang::DaDat) {
            return;
        }
        let p = nga_tu().into_iter().min_by(|a, b| {
            let d = |q: &Diem| (q.0 - x as f64).powi(2) + (q.1 - y as f64).powi(2);
            d(a).total_cmp(&d(b))
        });
        let Some(p) = p else { return };
        if la_don && p != self.den {
            self.don = p;
        } else if !la_don && p != self.don {
            self.den = p;
        }
    }

    /// Đổi pha hoạt hình (tới điểm đón -> đang đi -> đã tới) – mỗi lần vẽ (≈ 40 ms).
    fn nhip_hoat_hinh(&mut self, nd: &mut NguoiDung) {
        let Some((_, t0, dai, moc)) = self.hoat_hinh.clone() else {
            self.pha = None;
            return;
        };
        let s = (bay_gio() - t0) * TOC_DO;
        let pha = if s < moc { "toi_don" } else if s < dai { "di" } else { "den" };
        if Some(pha) != self.pha {
            self.pha = Some(pha);
            if pha == "di" {
                let tx = self.chuyen.as_ref().map(|c| so_us(c.get("tai_xe"))).unwrap_or_default();
                self.bao(&format!("Tài xế {tx} đã đón bạn – đang đi"), mau("#c8f0d2"), XANH_DAM);
                nd.nhat_ky.ghi("Tài xế đã tới điểm đón");
            } else if pha == "den" {
                self.bao("Đã tới nơi – bấm Hoàn tất chuyến", mau("#c8f0d2"), XANH_DAM);
                nd.nhat_ky.ghi(&format!("Đã tới {}", ten_diem(self.den)));
            }
        }
    }

    fn ve_ban_do(&self, bd: &BanDo, tt: Option<&Obj>) {
        bd.ve_nen();
        match &self.hoat_hinh {
            Some((ds, t0, dai, moc)) => {
                let s = (bay_gio() - t0) * TOC_DO;
                if s < *moc {
                    // xe đang tới điểm đón: tuyến nét đứt
                    bd.tuyen(&doan(ds, s, *moc), mau("#5f8fd8"), true);
                    bd.tuyen(&duong_di(self.don, self.den), XANH_TUYEN, false);
                } else {
                    bd.tuyen(&doan(ds, s, *dai), XANH_TUYEN, false);
                }
            }
            None => bd.tuyen(&duong_di(self.don, self.den), mau("#8fb4ec"), false),
        }
        bd.ghim(self.don, XANH_GRAB, "Đón");
        bd.ghim(self.den, DO_PIN, "Đến");
        if let Some(tt) = tt {
            bd.cac_xe(tt, self.xe_toi.filter(|_| self.hoat_hinh.is_some()), false);
        }
        if let (Some((ds, t0, dai, _)), Some(c)) = (&self.hoat_hinh, &self.chuyen) {
            let (d, ngang) = diem_tren(ds, dai.min((bay_gio() - t0) * TOC_DO));
            bd.xe(d, lay_str(c, "mau").unwrap_or(""), lay_str(c, "tai_xe").unwrap_or(""), ngang, false, true);
        }
    }
}

impl UngDung for UngDungGoiXe {
    const TIEU_DE: &'static str = "Gọi xe";

    fn moi(ten: &str) -> Self {
        let (don, den) = diem_mac_dinh(ten);
        UngDungGoiXe {
            don,
            den,
            trang: Trang::SanSang,
            xe_toi: None,
            chuyen: None,
            phien_toi: 0,
            hoat_hinh: None,
            so_dat: 0,
            so_duoc: 0,
            t_tim: 0.0,
            nhan_tt: ("Chưa kết nối".into(), mau("#e8e8e8"), Color32::BLACK),
            pha: None,
        }
    }

    fn ve(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung) {
        self.nhip_hoat_hinh(nd);
        let (rong, cao) = (440.0, 340.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                let (r, pt) = ui.allocate_painter(egui::vec2(rong, cao), egui::Sense::click());
                let bd = BanDo { pt: &pt, g: r.rect.min, w: rong, h: cao, nho: false };
                self.ve_ban_do(&bd, nd.tt.as_ref());
                pt.rect_stroke(r.rect, 0.0, egui::Stroke::new(1.0_f32, mau("#808080")));
                if let Some(pos) = r.interact_pointer_pos() {
                    let (x, y) = ((pos.x - r.rect.min.x) / rong, (pos.y - r.rect.min.y) / cao);
                    if r.clicked() {
                        self.chon(x, y, true);
                    } else if r.secondary_clicked() {
                        self.chon(x, y, false);
                    }
                }
                ui.label(RichText::new("Chuột trái: chọn điểm đón · chuột phải: chọn điểm đến").size(10.5).color(mau("#404040")));
            });
            egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, Color32::BLACK)).inner_margin(egui::Margin::same(8.0)).show(ui, |ui| ui.vertical(|ui| {
                ui.set_width(236.0);
                ui.label(RichText::new("Bạn muốn đi đâu?").strong().size(15.0));
                ui.label(RichText::new(format!("● Đón:  {}", ten_diem(self.don))).color(XANH_GRAB));
                ui.label(RichText::new(format!("● Đến:  {}", ten_diem(self.den))).color(DO_PIN));
                ui.label(RichText::new(format!("{:.1} km · cước dự kiến {}", km(self.don, self.den), tien(cuoc(self.don, self.den)))).strong().size(13.0));
                let co = self.co_the_dat(nd);
                if nut_chinh(ui, "ĐẶT XE", if co { XANH_GRAB } else { mau("#7fcfa5") }, co, 236.0, 34.0) {
                    self.dat_xe(nd);
                }
                nhan_nen(ui, &self.nhan_tt.0.clone(), self.nhan_tt.1, self.nhan_tt.2, 13.0, 224.0);
                egui::Frame::none().fill(mau("#f5f7f6")).stroke(egui::Stroke::new(1.0_f32, mau("#c0c8c4"))).inner_margin(egui::Margin::same(4.0)).show(ui, |ui| {
                    ui.set_width(228.0);
                    ui.horizontal(|ui| {
                        let (r, pt) = ui.allocate_painter(egui::vec2(44.0, 44.0), egui::Sense::hover());
                        // thẻ tài xế
                        if let Some(c) = &self.chuyen {
                            pt.circle_filled(r.rect.center(), 18.0, XANH_GRAB);
                            let dau: String = lay_str(c, "tai_xe").unwrap_or("?").chars().take(1).collect();
                            pt.text(r.rect.center(), egui::Align2::CENTER_CENTER, dau, egui::FontId::proportional(20.0), Color32::WHITE);
                        }
                        ui.vertical(|ui| match &self.chuyen {
                            Some(c) => {
                                ui.label(RichText::new(format!("{}  ★ 4.9", so_us(c.get("tai_xe")))).strong().size(13.0));
                                ui.label(RichText::new(format!("{} · xe {}\nđến sau {} phút · {}", so_us(c.get("bien_so")), so_us(c.get("mau")), so_us(c.get("phut")), tien(lay_i64(c, "cuoc").unwrap_or(0)))).size(10.5));
                            }
                            None => {
                                ui.label(RichText::new("Tài xế: –").strong().size(13.0));
                            }
                        });
                    });
                });
                let chu_huy = if self.hoat_hinh.is_some() && self.da_toi() { "Hoàn tất chuyến" } else { "Hủy chuyến" };
                if nut(ui, chu_huy, mau("#f0f0f0"), self.trang == Trang::DaDat, 120.0) {
                    self.huy(nd);
                }
                if let Some(tt) = &nd.tt {
                    ui.label(format!("Xe trống quanh bạn: {}/{}", so_us(tt.get("trong")), so_us(tt.get("so_xe"))));
                }
                if self.so_dat > 0 {
                    ui.label(RichText::new(format!("Bạn: bấm đặt {} lần, được báo thành công {}", self.so_dat, self.so_duoc)).size(10.5).color(mau("#505050")));
                }
            }));
        });
        if self.hoat_hinh.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    fn ket_qua(&mut self, nd: &mut NguoiDung, loai: &str, _truong: &Obj, tin: &Obj, rtt: f64) {
        let s = |k: &str| so_us(tin.get(k));
        if loai == "dat_xe" {
            if dung(tin, "ok") {
                self.so_duoc += 1;
                self.trang = Trang::DaDat;
                self.xe_toi = lay_i64(tin, "xe");
                self.chuyen = Some(tin.clone());
                self.phien_toi = lay_i64(tin, "phien").unwrap_or(0);
                self.bao(&format!("Đặt thành công – tài xế {}", s("tai_xe")), mau("#c8f0d2"), XANH_DAM);
                nd.nhat_ky.ghi(&format!("  ĐẶT THÀNH CÔNG: {} {}, đến sau {} phút ({rtt:.1} ms)", s("tai_xe"), s("bien_so"), s("phut")));
                let goc = nd.tt.as_ref().and_then(|t| xe_ma(t, self.xe_toi.unwrap_or(0))).and_then(|x| diem_truong(x, "vi_tri")).unwrap_or(self.don);
                let mut ds = duong_di(goc, self.don);
                let moc = do_dai(&ds);
                ds.extend(duong_di(self.don, self.den).into_iter().skip(1));
                let dai = do_dai(&ds);
                self.hoat_hinh = Some((ds, bay_gio(), dai, moc));
                self.kiem_xe(nd);
            } else if lay_str(tin, "ma_loi") == Some("het_cho") {
                self.trang = Trang::HetCho;
                self.bao("Hết chỗ – không còn xe trống", mau("#ffd0d0"), DO_DAM);
                nd.nhat_ky.ghi(&format!("  HẾT CHỖ ({rtt:.1} ms)"));
            } else {
                self.trang = Trang::HetCho;
                self.bao(&format!("Không đặt được: {}", s("ly_do")), mau("#ffd0d0"), DO_DAM);
                nd.nhat_ky.ghi(&format!("  từ chối: {}", s("ly_do")));
            }
        } else {
            if dung(tin, "ok") {
                let xong = self.hoat_hinh.is_some() && self.da_toi();
                nd.nhat_ky.ghi(&if xong { "Hoàn tất chuyến, cảm ơn bạn!".to_string() } else { format!("Đã hủy chuyến ({})", s("tai_xe")) });
                self.bao(if xong { "Hoàn tất chuyến – cảm ơn bạn!" } else { "Đã hủy chuyến" }, mau("#e8e8e8"), Color32::BLACK);
                self.trang = Trang::Xong;
            } else {
                nd.nhat_ky.ghi(&format!("  hủy bị từ chối: {}", s("ly_do")));
                self.trang = Trang::SanSang;
            }
            self.xe_toi = None;
            self.chuyen = None;
            self.hoat_hinh = None;
        }
    }

    fn cap_nhat(&mut self, nd: &mut NguoiDung, _tt: &Obj) {
        if self.trang != Trang::DangTim && self.nhan_tt.0 == "Chưa kết nối" {
            self.bao("Chọn điểm đón và bấm ĐẶT XE", mau("#e8e8e8"), Color32::BLACK);
        }
        self.kiem_xe(nd);
    }

    fn moi_nhip(&mut self, _nd: &mut NguoiDung) {
        if self.trang == Trang::DangTim {
            let cham = ".".repeat(1 + ((bay_gio() - self.t_tim) * 5.0) as usize % 3);
            self.bao(&format!("Đang tìm tài xế{cham}"), mau("#fff3c4"), Color32::BLACK);
        }
    }

    fn mat_ket_noi(&mut self, _nd: &mut NguoiDung) {
        self.trang = Trang::SanSang;
        self.xe_toi = None;
        self.chuyen = None;
        self.hoat_hinh = None;
    }

    fn dong_dinh_ky(&self, nd: &NguoiDung) -> String {
        let tt = nd.tt.clone().unwrap_or_default();
        let xe = self.xe_toi.and_then(|m| xe_ma(&tt, m)).map_or(String::new(), |x| format!(", xe {} {}", so_us(x.get("ma")), so_us(x.get("tai_xe"))));
        format!("{}/{} xe trống; trạng thái {}{xe}; bạn đặt {}, được {}", so_us(tt.get("trong")), so_us(tt.get("so_xe")), self.trang.ten(), self.so_dat, self.so_duoc)
    }
}

/// dat_xe_gui [tên] [host] [cổng]
pub fn run(args: &[String]) {
    let ten = args.first().cloned().unwrap_or_else(|| "An".into());
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    let cs = CuaSoNguoiDung::<UngDungGoiXe>::moi(&ten, &host, cong);
    chay_cua_so(&cs.tieu_de(), [740.0, 600.0], cs);
}
