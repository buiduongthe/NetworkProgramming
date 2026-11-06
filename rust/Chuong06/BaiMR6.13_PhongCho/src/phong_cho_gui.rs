//! Bài mở rộng MR6.13 – Trang bán vé có phòng chờ ảo (ứng dụng người dùng, egui).
//!
//! Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở gui_chung::CuaSoNguoiDung; tệp này vẽ
//! ba "trang" như một trang bán vé thật:
//!   1. Sự kiện: áp phích concert (vẽ bằng bút vẽ egui), giá vé, thanh "vé còn lại", nút "VÀO HÀNG CHỜ MUA
//!      VÉ" (phòng chờ tắt: "MUA VÉ NGAY");
//!   2. Phòng chờ: số thứ tự, THANH TIẾN TRÌNH vị trí, "Còn khoảng x người phía trước – chờ ~y s", số đang
//!      phục vụ, tốc độ cấp lượt; cập nhật trực tiếp theo cap_nhat (≤ 10 lần/giây);
//!   3. Chọn vé: "ĐẾN LƯỢT BẠN!", ĐỒNG HỒ đếm ngược thời hạn lượt (20 s, đỏ khi < 5 s), chọn số vé, thành
//!      tiền, nút "THANH TOÁN", kết quả ("Mua thành công…", "Hết vé", "Lượt đã hết hạn", "Hệ thống quá tải").
//! Tới lượt được báo bằng trả lời của yêu cầu "chờ dài" cho_luot (máy chủ giữ yêu cầu tới khi cấp lượt).
//!
//! Chạy:  phong_cho_gui <tên> [host] [cổng]       (mặc định 127.0.0.1 44772)

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::{so_con, CONG_GUI};
use eframe::egui::{self, Color32, RichText, Stroke};
use serde_json::{json, Value};

pub const TIM: &str = "#3b0a5e";
pub const HONG: &str = "#e0407a";

/// Bộ sinh số giả ngẫu nhiên cố định (như random.Random(7)): áp phích lần nào vẽ cũng giống nhau.
struct NgauNhien(u64);

impl NgauNhien {
    fn so(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32) / (1u64 << 24) as f32
    }
    fn nguyen(&mut self, a: i32, b: i32) -> f32 {
        (a + ((self.so() * (b - a + 1) as f32) as i32).min(b - a)) as f32
    }
}

/// Áp phích concert: nền chuyển màu, đèn sân khấu, đám đông, tên sự kiện.
pub fn ve_ap_phich(pt: &egui::Painter, r: egui::Rect, ten: &str) {
    let (w, h, g) = (r.width(), r.height(), r.min);
    let mut mesh = egui::Mesh::default();
    let (tren, duoi) = (mau("#12002a"), mau("#6a1b6e"));
    mesh.colored_vertex(r.left_top(), tren);
    mesh.colored_vertex(r.right_top(), tren);
    mesh.colored_vertex(r.right_bottom(), duoi);
    mesh.colored_vertex(r.left_bottom(), duoi);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    pt.add(egui::Shape::mesh(mesh));
    let mut nn = NgauNhien(7);
    for _ in 0..40 {
        // sao
        let (x, y) = (nn.nguyen(0, w as i32), nn.nguyen(0, (h / 2.0) as i32));
        pt.circle_filled(p(g, x + 1.0, y + 1.0), 1.0, mau("#fff4c0"));
    }
    for (x0, m) in [(w * 0.18, "#ffd36e"), (w * 0.5, "#7fd6ff"), (w * 0.82, "#ff8fc0")] {
        // đèn chiếu (nửa trong suốt như stipple của Tk)
        let c = mau(m);
        pt.add(egui::Shape::convex_polygon(vec![p(g, x0, h * 0.62), p(g, x0 - 70.0, -10.0), p(g, x0 + 70.0, -10.0)], Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 64), Stroke::NONE));
        pt.circle_filled(p(g, x0, h * 0.62), 7.0, c);
    }
    pt.rect_filled(egui::Rect::from_min_max(p(g, 0.0, h * 0.66), p(g, w, h * 0.72)), 0.0, mau("#2a0f3a")); // sân khấu
    let den = mau("#0a0012");
    for i in 0..=(w / 14.0) as i32 {
        // đám đông (bóng đầu, tay giơ)
        let x = i as f32 * 14.0 + nn.nguyen(-3, 3);
        let y = h - 18.0 + nn.nguyen(-4, 4);
        pt.circle_filled(p(g, x, y), 7.0, den);
        pt.rect_filled(egui::Rect::from_min_max(p(g, x - 9.0, y + 4.0), p(g, x + 9.0, h)), 0.0, den);
        if nn.so() < 0.3 {
            pt.line_segment([p(g, x + 5.0, y), p(g, x + 10.0, y - 20.0)], Stroke::new(4.0_f32, den));
        }
    }
    pt.text(p(g, 20.0, 30.0), egui::Align2::LEFT_CENTER, ten.to_uppercase(), egui::FontId::proportional(23.0), Color32::WHITE);
    pt.text(p(g, 22.0, 58.0), egui::Align2::LEFT_CENTER, "20:00 · 20.12.2026 · Sân khấu ngoài trời", egui::FontId::proportional(12.5), mau("#ffd9f0"));
    pt.rect_filled(egui::Rect::from_min_max(p(g, w - 120.0, 14.0), p(g, w - 14.0, 40.0)), 0.0, mau(HONG));
    pt.text(p(g, w - 67.0, 27.0), egui::Align2::CENTER_CENTER, "MỞ BÁN", egui::FontId::proportional(12.5), Color32::WHITE);
}

/// Đếm ngược thời hạn lượt: 00:ss (dưới 10 s thêm phần mười).
pub fn dong_ho(giay: f64) -> String {
    let giay = giay.max(0.0);
    if giay >= 10.0 {
        format!("00:{:02}", giay as i64)
    } else {
        format!("00:{giay:04.1}")
    }
}

/// Thanh tiến trình (như ttk.Progressbar).
pub fn thanh(ui: &mut egui::Ui, rong: f32, ti_le: f32) {
    let (r, pt) = ui.allocate_painter(egui::vec2(rong, 18.0), egui::Sense::hover());
    pt.rect(r.rect, 0.0, mau("#e6e6e6"), Stroke::new(1.0_f32, mau("#b0b0b0")));
    let w = (rong - 2.0) * ti_le.clamp(0.0, 1.0);
    pt.rect_filled(egui::Rect::from_min_size(r.rect.min + egui::vec2(1.0, 1.0), egui::vec2(w, 16.0)), 0.0, mau("#06b025"));
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trang {
    SuKien,
    Cho,
    Mua,
}

pub struct TrangBanVe {
    pub trang: Trang,
    pub so: Option<i64>,       // số thứ tự trong hàng
    truoc_dau: Option<i64>,    // số người phía trước lúc vào hàng
    han: Option<f64>,          // hạn lượt (đồng hồ đơn điệu); None: không giới hạn
    co_luot: bool,
    pub so_kq: u64,
    pub so_don: u64,
    pub so_ve: i64,
    nut_vao_bat: bool,
    pub kq_sk: String,
    pub nut_mua_bat: bool,
    pub kq: (String, Color32),
    pub dong_ho: (String, Color32),
    pub bang_luot: (String, Color32),
    nut_ve_hien: bool,
}

impl TrangBanVe {
    /// Bấm "VÀO HÀNG CHỜ MUA VÉ" (phòng chờ tắt: "MUA VÉ NGAY").
    pub fn vao_hang(&mut self, nd: &mut NguoiDung) {
        if !self.nut_vao_bat || nd.kn.is_none() {
            return;
        }
        self.kq_sk.clear();
        if !nd.tt.as_ref().is_some_and(|t| dung(t, "phong_cho")) {
            // phòng chờ tắt: vào thẳng trang mua
            self.mo_trang_mua(None);
            return;
        }
        self.nut_vao_bat = false;
        nd.nhat_ky.ghi("Vào phòng chờ…");
        nd.gui_yc("xep_hang", Obj::new());
    }

    fn mo_trang_mua(&mut self, han_ms: Option<f64>) {
        self.co_luot = true;
        self.han = han_ms.map(|ms| bay_gio() + ms / 1000.0);
        self.bang_luot = if han_ms.is_some() { ("ĐẾN LƯỢT BẠN! Hãy hoàn tất trước khi hết giờ".into(), mau("#2e9e4f")) } else { ("MUA TRỰC TIẾP (phòng chờ tắt)".into(), mau(TIM)) };
        self.kq = (String::new(), Color32::BLACK);
        self.nut_mua_bat = true;
        self.nut_ve_hien = false;
        self.trang = Trang::Mua;
    }

    /// Chọn n vé rồi bấm "THANH TOÁN" (kịch bản demo) – nút đang khóa thì không có gì xảy ra.
    pub fn mua(&mut self, nd: &mut NguoiDung, n: i64) {
        self.so_ve = n;
        if self.trang == Trang::Mua && self.nut_mua_bat {
            self.thanh_toan(nd);
        }
    }

    fn thanh_toan(&mut self, nd: &mut NguoiDung) {
        nd.nhat_ky.ghi(&format!("Thanh toán {} vé…", self.so_ve));
        self.kq = ("Đang xử lý thanh toán…".into(), mau("#404040"));
        nd.gui_yc("mua", obj(json!({"so_ve": self.so_ve})));
    }

    /// Bấm "Về trang sự kiện".
    pub fn ve_su_kien(&mut self, nd: &NguoiDung) {
        self.so = None;
        self.truoc_dau = None;
        self.han = None;
        self.co_luot = false;
        self.nut_vao_bat = nd.tt.as_ref().is_some_and(|t| !dung(t, "het_ve"));
        self.trang = Trang::SuKien;
    }

    pub fn truoc(&self, nd: &NguoiDung) -> i64 {
        match (self.so, &nd.tt) {
            (Some(so), Some(tt)) => (so - lay_i64(tt, "dang_phuc_vu").unwrap_or(0) - 1).max(0),
            _ => 0,
        }
    }

    pub fn chu_so(&self) -> String {
        self.so.map_or("Số thứ tự của bạn: –".into(), |s| format!("Số thứ tự của bạn: #{s}"))
    }

    pub fn chu_truoc(&self, nd: &NguoiDung) -> String {
        let (Some(_), Some(tt)) = (self.so, &nd.tt) else { return "–".into() };
        let truoc = self.truoc(nd);
        if truoc > 0 {
            format!("Còn khoảng {truoc} người phía trước – chờ ~{:.0} s", (truoc + 1) as f64 / lay_f64(tt, "toc_do").unwrap_or(1.0))
        } else {
            "Sắp tới lượt bạn…".into()
        }
    }

    fn ve_su_kien_trang(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung, tt: Option<&Obj>) {
        let Some(tt) = tt else {
            ui.label(RichText::new("–").strong().size(14.0).color(mau(TIM)));
            return;
        };
        let pc = dung(tt, "phong_cho");
        let (con, tong) = (lay_i64(tt, "con_lai").unwrap_or(0), so_con(tt, "su_kien", "tong_ve").max(1));
        ui.label(RichText::new(format!("Giá vé {} · tối đa {} vé/người", tien(so_con(tt, "su_kien", "gia_ve")), so_con(tt, "su_kien", "ve_toi_da"))).strong().size(14.0).color(mau(TIM)));
        ui.label(RichText::new(format!("Vé còn lại: {con} / {tong}{}", if dung(tt, "het_ve") { "  – ĐÃ BÁN HẾT" } else { "" })).size(12.5));
        thanh(ui, 510.0, con.max(0) as f32 / tong as f32);
        ui.add_space(6.0);
        let luot_s = tt.get("su_kien").and_then(|s| s.get("luot_s")).and_then(Value::as_f64).unwrap_or(20.0);
        let mo_ta = if pc {
            format!("Phòng chờ ảo BẬT: mọi người được xếp hàng theo thứ tự đến, mỗi giây cấp lượt cho {} người; mỗi lượt có {} s để mua.", fmt_g(lay_f64(tt, "toc_do").unwrap_or(0.0)), fmt_g(luot_s))
        } else {
            "Phòng chờ TẮT: ai cũng vào mua trực tiếp cùng lúc.".into()
        };
        ui.add(egui::Label::new(RichText::new(mo_ta).size(11.5).color(mau("#404040"))).wrap());
        ui.add_space(8.0);
        let mut bam = false;
        ui.vertical_centered(|ui| {
            if nut_chinh(ui, if pc { "VÀO HÀNG CHỜ MUA VÉ" } else { "MUA VÉ NGAY" }, mau(HONG), self.nut_vao_bat && nd.kn.is_some(), 300.0, 36.0) {
                bam = true;
            }
            ui.label(RichText::new(&self.kq_sk).strong().size(12.5).color(DO_DAM));
        });
        if bam {
            self.vao_hang(nd);
        }
    }

    fn ve_cho_trang(&self, ui: &mut egui::Ui, nd: &NguoiDung, tt: Option<&Obj>) {
        ui.label(RichText::new("Bạn đang ở trong phòng chờ").strong().size(16.0).color(mau(TIM)));
        ui.label(RichText::new(self.chu_so()).size(14.0));
        let truoc = self.truoc(nd);
        thanh(ui, 510.0, 1.0 - truoc as f32 / self.truoc_dau.unwrap_or(1).max(1) as f32);
        ui.label(RichText::new(self.chu_truoc(nd)).strong().size(15.0));
        if let Some(tt) = tt {
            ui.label(RichText::new(format!("Đang phục vụ tới số #{} · cấp lượt {} người/s · {} người đang chờ", so_us(tt.get("dang_phuc_vu")), fmt_g(lay_f64(tt, "toc_do").unwrap_or(0.0)), so_us(tt.get("dang_cho")))).size(11.5).color(mau("#404040")));
            ui.label(RichText::new(format!("Vé còn lại: {} / {}", so_us(tt.get("con_lai")), so_con(tt, "su_kien", "tong_ve"))).size(11.5).color(mau("#404040")));
        }
        ui.add_space(8.0);
        egui::Frame::none().fill(mau("#fff3c4")).inner_margin(egui::Margin::symmetric(4.0, 2.0)).show(ui, |ui| {
            ui.set_width(510.0);
            ui.label(RichText::new("Vui lòng không tải lại trang – bạn vẫn giữ nguyên chỗ trong hàng.").size(11.5));
        });
    }

    fn ve_mua_trang(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung, tt: Option<&Obj>) {
        let (chu, nen) = self.bang_luot.clone();
        egui::Frame::none().fill(nen).inner_margin(egui::Margin::symmetric(4.0, 4.0)).show(ui, |ui| {
            ui.set_width(530.0);
            ui.vertical_centered(|ui| ui.label(RichText::new(chu).strong().size(15.0).color(Color32::WHITE)));
        });
        ui.horizontal(|ui| {
            ui.label("Thời gian còn lại để hoàn tất:");
            ui.label(RichText::new(&self.dong_ho.0).monospace().strong().size(29.0).color(self.dong_ho.1));
        });
        let toi_da = tt.map_or(4, |t| so_con(t, "su_kien", "ve_toi_da").max(1));
        let gia = tt.map_or(0, |t| so_con(t, "su_kien", "gia_ve"));
        ui.horizontal(|ui| {
            ui.label(RichText::new("Số vé:").size(14.0));
            ui.add(egui::DragValue::new(&mut self.so_ve).range(1..=toi_da).speed(0.1));
            ui.add_space(8.0);
            ui.label(RichText::new(format!("× {} = {}", tien(gia), tien(self.so_ve * gia))).size(12.5));
        });
        let mut bam = false;
        let mut ve = false;
        ui.vertical_centered(|ui| {
            if nut_chinh(ui, "THANH TOÁN", mau("#ff8c00"), self.nut_mua_bat && nd.kn.is_some(), 230.0, 32.0) {
                bam = true;
            }
            ui.add(egui::Label::new(RichText::new(&self.kq.0).strong().size(14.0).color(self.kq.1)).wrap());
            if self.nut_ve_hien && nut(ui, "Về trang sự kiện", mau("#e8eef8"), true, 140.0) {
                ve = true;
            }
        });
        if bam {
            self.thanh_toan(nd);
        }
        if ve {
            self.ve_su_kien(nd);
        }
    }
}

impl UngDung for TrangBanVe {
    const TIEU_DE: &'static str = "Bán vé concert";

    fn moi(_ten: &str) -> Self {
        TrangBanVe {
            trang: Trang::SuKien,
            so: None,
            truoc_dau: None,
            han: None,
            co_luot: false,
            so_kq: 0,
            so_don: 0,
            so_ve: 2,
            nut_vao_bat: false,
            kq_sk: String::new(),
            nut_mua_bat: false,
            kq: (String::new(), Color32::BLACK),
            dong_ho: ("–".into(), XANH_DAM),
            bang_luot: ("ĐẾN LƯỢT BẠN!".into(), mau("#2e9e4f")),
            nut_ve_hien: false,
        }
    }

    fn ve(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung) {
        let tt = nd.tt.clone();
        let (r, pt) = ui.allocate_painter(egui::vec2(540.0, 150.0), egui::Sense::hover());
        let ten = tt.as_ref().and_then(|t| t.get("su_kien")).and_then(|s| s.get("ten")).map(py_str).unwrap_or_else(|| "Đêm nhạc".into());
        ve_ap_phich(&pt, r.rect, &ten);
        egui::Frame::none().fill(Color32::WHITE).stroke(Stroke::new(1.0_f32, mau("#9090b0"))).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
            ui.set_width(520.0);
            ui.set_height(230.0);
            ui.vertical(|ui| match self.trang {
                Trang::SuKien => self.ve_su_kien_trang(ui, nd, tt.as_ref()),
                Trang::Cho => self.ve_cho_trang(ui, nd, tt.as_ref()),
                Trang::Mua => self.ve_mua_trang(ui, nd, tt.as_ref()),
            });
        });
    }

    fn ket_qua(&mut self, nd: &mut NguoiDung, loai: &str, _truong: &Obj, tin: &Obj, rtt: f64) {
        self.so_kq += 1;
        let ly_do = tin.get("ly_do").map(py_str).unwrap_or_else(|| "None".into());
        if loai == "xep_hang" {
            if !dung(tin, "ok") {
                nd.nhat_ky.ghi(&format!("  không vào được hàng: {ly_do}"));
                self.kq_sk = format!("Rất tiếc: {ly_do}");
                self.nut_vao_bat = true;
                return;
            }
            let truoc = lay_i64(tin, "truoc").unwrap_or(0);
            self.so = lay_i64(tin, "so");
            self.truoc_dau = Some(truoc.max(1));
            nd.nhat_ky.ghi(&format!("  số thứ tự #{}, {truoc} người phía trước, ~{:.1} s", so_us(tin.get("so")), lay_f64(tin, "cho_s").unwrap_or(0.0)));
            self.trang = Trang::Cho;
            nd.gui_yc("cho_luot", Obj::new()); // chờ dài: máy chủ trả lời khi tới lượt
        } else if loai == "cho_luot" {
            if dung(tin, "ok") {
                let han_ms = lay_f64(tin, "han_ms").unwrap_or(0.0);
                nd.nhat_ky.ghi(&format!("  ĐẾN LƯỢT sau {:.1} s chờ, còn {:.0} s để mua", rtt / 1000.0, han_ms / 1000.0));
                self.mo_trang_mua(Some(han_ms));
            } else {
                nd.nhat_ky.ghi(&format!("  {ly_do}"));
                self.ve_su_kien(nd);
                self.kq_sk = format!("Rất tiếc: {ly_do}");
            }
        } else if dung(tin, "ok") {
            self.so_don += 1;
            nd.nhat_ky.ghi(&format!("  MUA THÀNH CÔNG ({rtt:.1} ms): đơn #{}, {} vé", so_us(tin.get("ma_don")), so_us(tin.get("so_ve"))));
            self.kq = (format!("Mua thành công {} vé – đơn #{}, {}", so_us(tin.get("so_ve")), so_us(tin.get("ma_don")), tien(lay_i64(tin, "thanh_tien").unwrap_or(0))), XANH_DAM);
            self.nut_mua_bat = false;
            self.han = None;
            self.co_luot = false;
            self.dong_ho = ("✓".into(), XANH_DAM);
            self.nut_ve_hien = true;
        } else {
            let ma = lay_str(tin, "ma_loi").unwrap_or("");
            nd.nhat_ky.ghi(&format!("  từ chối ({rtt:.1} ms): {ly_do}"));
            let text = match ma {
                "qua_tai" => "Hệ thống đang quá tải, vui lòng thử lại".to_string(),
                "het_ve" => "Rất tiếc, đã bán hết vé".to_string(),
                "da_mua" => "Lượt này đã được dùng để mua".to_string(),
                _ => lay_str(tin, "ly_do").unwrap_or("").to_string(),
            };
            self.kq = (text, DO_DAM);
            if matches!(ma, "het_ve" | "het_luot" | "da_mua") {
                self.nut_mua_bat = false;
                self.nut_ve_hien = true;
            }
        }
    }

    fn cap_nhat(&mut self, _nd: &mut NguoiDung, tt: &Obj) {
        if self.trang == Trang::SuKien {
            self.nut_vao_bat = !dung(tt, "het_ve");
        }
        self.so_ve = self.so_ve.min(so_con(tt, "su_kien", "ve_toi_da").max(1));
    }

    fn moi_nhip(&mut self, nd: &mut NguoiDung) {
        if self.trang != Trang::Mua || !self.co_luot {
            return;
        }
        let Some(han) = self.han else {
            self.dong_ho = ("∞".into(), XANH_DAM);
            return;
        };
        let con = han - bay_gio();
        self.dong_ho = (dong_ho(con), if con < 5.0 { DO_DAM } else { XANH_DAM });
        if con <= 0.0 {
            self.co_luot = false;
            self.nut_mua_bat = false;
            self.kq = ("Hết thời gian giữ lượt – bạn cần xếp hàng lại".into(), DO_DAM);
            self.nut_ve_hien = true;
            nd.nhat_ky.ghi("Lượt mua đã hết hạn");
        }
    }

    fn mat_ket_noi(&mut self, _nd: &mut NguoiDung) {
        self.nut_vao_bat = false;
        self.nut_mua_bat = false;
    }

    fn dong_dinh_ky(&self, nd: &NguoiDung) -> String {
        let tt = nd.tt.clone().unwrap_or_default();
        let trang = match self.trang {
            Trang::SuKien => "sự kiện",
            Trang::Cho => "phòng chờ",
            Trang::Mua => "mua",
        };
        let vi_tri = self.so.map_or(String::new(), |s| format!(", số #{s}, phục vụ #{}", so_us(tt.get("dang_phuc_vu"))));
        format!("trang {trang}{vi_tri}; vé còn {}; đơn của bạn {}", so_us(tt.get("con_lai")), self.so_don)
    }
}

/// phong_cho_gui [tên] [host] [cổng]
pub fn run(args: &[String]) {
    let ten = args.first().cloned().unwrap_or_else(|| "An".into());
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    let cs = CuaSoNguoiDung::<TrangBanVe>::moi(&ten, &host, cong);
    chay_cua_so(&cs.tieu_de(), [600.0, 620.0], cs);
}

