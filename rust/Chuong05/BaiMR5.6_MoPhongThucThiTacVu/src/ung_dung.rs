//! Bài mở rộng MR5.6 – Mô phỏng thực thi tác vụ: ứng dụng giao diện (eframe/egui).
//!
//! Chọn chế độ (đơn luồng/đa luồng × đồng bộ/bất đồng bộ) hoặc "So sánh 4 chế độ"; biểu đồ Gantt chạy theo
//! thời gian: mỗi hàng là một luồng (luồng hệ điều hành, vòng lặp sự kiện hay luồng thợ), màu là tác vụ
//! đang dùng CPU; ô nhạt có gạch chéo: luồng bị chặn chờ I/O; ô viền đứt: sẵn sàng nhưng chờ lõi CPU; ô chấm:
//! chờ join. Hàng "I/O chờ" (bất đồng bộ) vẽ các lần chờ I/O không giữ luồng nào. Nút "Chen ngang U ngay" đưa
//! việc khẩn U vào đúng thời điểm đang xem và mô phỏng lại (lịch sử trước đó không đổi vì bộ lập lịch tất định).
//!
//! egui là giao diện "immediate mode": mỗi khung hình, `update` dựng lại toàn bộ giao diện từ trạng thái.
//! Toàn bộ chạy trên luồng giao diện: mô phỏng rất nhanh (vài mili giây), hoạt hình chỉ là xin vẽ lại sau
//! 30 ms (`request_repaint_after`) – không có luồng nền nào, không bao giờ chặn vòng lặp sự kiện.
//!
//! Chạy:  cargo run --release --bin ung_dung
//!        cargo run --release --bin ung_dung -- --demo   kịch bản tự thao tác, chụp cửa sổ thành
//!                                                       ket_qua_gui.png (dưới Linux không màn hình: xvfb-run)

use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Shape, Stroke, Vec2};
use lap_lich::{self as l, KetQua, ThamSo, TrangThai, CHE_DO};

const fn mau(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Màu đậm (đang dùng CPU) và màu nhạt (chờ I/O) của từng tác vụ.
fn mau_viec(t: &str) -> (Color32, Color32) {
    match t {
        "T1" => (mau(0x2457c5), mau(0xc9d8f6)),
        "T2" => (mau(0xe07b00), mau(0xfbdcb8)),
        "T3" => (mau(0x1b9e5a), mau(0xc3ead3)),
        _ => (mau(0xd1344b), mau(0xf6c5cc)),
    }
}
const NEN: Color32 = mau(0xffffff);
const LUOI: Color32 = mau(0xe4e7ec);
const LUOI_DAM: Color32 = mau(0xc9ced8);
const CHU: Color32 = mau(0x1d2330);
const CHU_MO: Color32 = mau(0x667085);
const XAM: Color32 = mau(0xe9ecef);
const VIEN_XAM: Color32 = mau(0xadb5bd);
const IO_XANH: Color32 = mau(0x7fb069);
const TOC_DO: [&str; 5] = ["× 0.25", "× 0.5", "× 1", "× 2", "× 4"];

struct UngDung {
    ket_qua: Vec<(&'static str, KetQua)>, // các chế độ đang hiển thị
    t_xem: f64,                           // thời điểm đang xem (nhịp, số thực để hoạt hình mượt)
    dang_chay: bool,
    last: Option<Instant>,
    // ô nhập (chuỗi như Entry của Tk, đọc lại khi bấm nút)
    t1: String,
    t2: String,
    t3: String,
    loi: i32,
    lat: String,
    toc_do: usize,
    phu_thuoc: bool,
    chia: bool,
    khan: bool,
    t_khan: String,
    demo: Option<Demo>,
}

impl UngDung {
    fn new() -> UngDung {
        UngDung {
            ket_qua: vec![],
            t_xem: 0.0,
            dang_chay: false,
            last: None,
            t1: "4".into(),
            t2: "3".into(),
            t3: "2".into(),
            loi: 2,
            lat: "0.5".into(),
            toc_do: 2,
            phu_thuoc: true,
            chia: true,
            khan: true,
            t_khan: "0.3".into(),
            demo: None,
        }
    }

    // ------------------------------------------------------------------ thao tác
    fn doc_tham_so(&self) -> ThamSo {
        let so = |s: &str, mac: f64| s.trim().replace(',', ".").parse::<f64>().unwrap_or(mac);
        ThamSo::new(
            so(&self.t1, 4.0).max(0.4),
            so(&self.t2, 3.0).max(0.4),
            so(&self.t3, 2.0).max(0.4),
            self.loi.clamp(1, 4) as i64,
            so(&self.lat, 0.5).max(0.1),
            self.phu_thuoc,
            self.chia,
            self.khan,
            so(&self.t_khan, 0.3).max(0.0),
        )
    }

    fn chon(&mut self, ds: &[&'static str]) {
        let ts = self.doc_tham_so();
        self.ket_qua = ds.iter().map(|&m| (m, l::chay(m, &ts))).collect();
        self.t_xem = 0.0;
        self.dang_chay = true;
        self.last = None;
    }

    fn xoa(&mut self) {
        self.ket_qua.clear();
        self.t_xem = 0.0;
        self.dang_chay = false;
    }

    fn chay_dung(&mut self) {
        self.dang_chay = !self.dang_chay;
        self.last = None;
    }

    /// Đặt việc khẩn U đến ngay thời điểm đang xem rồi mô phỏng lại; giữ nguyên thời điểm xem.
    fn chen_ngang(&mut self) {
        if self.ket_qua.is_empty() {
            return;
        }
        self.khan = true;
        self.t_khan = l::giay(self.t_xem as i64);
        let ts = self.doc_tham_so();
        for (m, kq) in self.ket_qua.iter_mut() {
            *kq = l::chay(m, &ts);
        }
    }

    fn do_dai(&self) -> i64 {
        self.ket_qua.iter().map(|(_, kq)| kq.tong).max().unwrap_or(0)
    }

    /// Hoạt hình: tiến thời điểm xem theo thời gian thật × tốc độ (×1: 1 giây mô phỏng mỗi giây thật).
    fn nhip(&mut self) {
        let now = Instant::now();
        if self.dang_chay && !self.ket_qua.is_empty() {
            if let Some(last) = self.last {
                let toc: f64 = TOC_DO[self.toc_do][3..].trim().parse().unwrap_or(1.0);
                self.t_xem += (now - last).as_secs_f64() * 10.0 * toc;
                if self.t_xem >= self.do_dai() as f64 {
                    self.t_xem = self.do_dai() as f64;
                    self.dang_chay = false;
                }
            }
            self.last = Some(now);
        }
    }

    // ------------------------------------------------------------------ giao diện
    fn thanh_tren(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            nhom(ui, "Điều khiển", |ui| {
                egui::Grid::new("dieu_khien").spacing([6.0, 6.0]).show(ui, |ui| {
                    let nut = [
                        ("Đơn luồng – đồng bộ", "don_dong_bo"),
                        ("Đa luồng – đồng bộ", "da_dong_bo"),
                        ("Đơn luồng – bất đồng bộ", "don_bat_dong_bo"),
                        ("Đa luồng – bất đồng bộ", "da_bat_dong_bo"),
                    ];
                    let rong = egui::vec2(158.0, 26.0);
                    for hang in 0..2 {
                        for (ten, m) in &nut[hang * 2..hang * 2 + 2] {
                            if ui.add_sized(rong, egui::Button::new(*ten)).clicked() {
                                self.chon(&[m]);
                            }
                        }
                        if hang == 0 {
                            if ui.add_sized(egui::vec2(122.0, 26.0), egui::Button::new("So sánh 4 chế độ")).clicked() {
                                self.chon(&CHE_DO);
                            }
                            if ui.add_sized(egui::vec2(148.0, 26.0), egui::Button::new("⚡ Chen ngang U ngay")).clicked() {
                                self.chen_ngang();
                            }
                        } else {
                            if ui.add_sized(egui::vec2(122.0, 26.0), egui::Button::new("Xóa")).clicked() {
                                self.xoa();
                            }
                            let chu = if self.dang_chay { "⏸ Dừng" } else { "▶ Chạy" };
                            if ui.add_sized(egui::vec2(148.0, 26.0), egui::Button::new(chu)).clicked() {
                                self.chay_dung();
                            }
                        }
                        ui.end_row();
                    }
                });
            });
            nhom(ui, "Thông số", |ui| {
                egui::Grid::new("thong_so").spacing([6.0, 3.0]).show(ui, |ui| {
                    let o_nhap = |ui: &mut egui::Ui, s: &mut String| {
                        ui.add(egui::TextEdit::singleline(s).desired_width(36.0));
                    };
                    // hàng 1
                    ui.label("T1 (s)");
                    o_nhap(ui, &mut self.t1);
                    ui.label("Số lõi CPU");
                    ui.add(egui::DragValue::new(&mut self.loi).range(1..=4).speed(0.05));
                    ui.checkbox(&mut self.phu_thuoc, "T3 chờ T1, T2 (join/await)");
                    ui.label("");
                    ui.end_row();
                    // hàng 2
                    ui.label("T2 (s)");
                    o_nhap(ui, &mut self.t2);
                    ui.label("Lát thời gian (s)");
                    o_nhap(ui, &mut self.lat);
                    ui.checkbox(&mut self.chia, "Chia CPU cuối T1 trên 2 luồng");
                    ui.label("");
                    ui.end_row();
                    // hàng 3
                    ui.label("T3 (s)");
                    o_nhap(ui, &mut self.t3);
                    ui.label("Tốc độ");
                    egui::ComboBox::from_id_salt("toc_do").width(64.0).selected_text(TOC_DO[self.toc_do]).show_ui(ui, |ui| {
                        for (i, t) in TOC_DO.iter().enumerate() {
                            ui.selectable_value(&mut self.toc_do, i, *t);
                        }
                    });
                    ui.checkbox(&mut self.khan, "Việc khẩn U lúc (s)");
                    o_nhap(ui, &mut self.t_khan);
                    ui.end_row();
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let d = self.do_dai();
                let chu = if d > 0 {
                    format!("t = {} / {} s", l::giay((self.t_xem as i64).min(d)), l::giay(d))
                } else {
                    "Chọn một chế độ".into()
                };
                ui.add_space(10.0);
                ui.label(RichText::new(chu).size(16.0).strong().color(CHU));
            });
        });
    }

    fn bang_chi_so(&self, ui: &mut egui::Ui) {
        egui::Frame::none().fill(NEN).stroke(net(1.0, LUOI_DAM)).inner_margin(4.0).show(ui, |ui| {
            ui.set_min_height(4.0 * 20.0 + 22.0);
            let cot = ((ui.available_width() - 6.0 * 8.0) / 7.0).max(60.0);
            egui::Grid::new("bang").striped(true).min_col_width(cot).max_col_width(cot).spacing([8.0, 3.0]).show(ui, |ui| {
                for c in ["Chế độ", "Hoàn tất", "Luồng", "Lõi bận", "U chờ", "Chen", "Hết lát"] {
                    o_giua(ui, RichText::new(c).strong());
                }
                ui.end_row();
                for (m, kq) in &self.ket_qua {
                    let s = l::chi_so(kq);
                    let u = if s.u_cho >= 0 { format!("{} s", l::giay(s.u_cho)) } else { "–".into() };
                    for chu in [l::ten_ngan(m).to_string(), format!("{} s", l::giay(s.tong)), s.luong.to_string(), format!("{} %", s.ban), u, s.chen.to_string(), s.lat.to_string()] {
                        o_giua(ui, RichText::new(chu));
                    }
                    ui.end_row();
                }
            });
        });
    }

    fn nhat_ky(&self, ui: &mut egui::Ui) {
        let d = self.do_dai();
        let t = (self.t_xem as i64).min(d);
        let mut chu = String::new();
        for (m, kq) in &self.ket_qua {
            if self.ket_qua.len() > 1 {
                chu += &format!("— {} —\n", l::ten_ngan(m));
            }
            for (tn, text) in &kq.nhat_ky {
                if *tn <= t {
                    chu += &format!("{:>4} s  {text}\n", l::giay(*tn));
                }
            }
        }
        egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(chu).font(FontId::proportional(12.5)).color(CHU)).wrap());
        });
    }

    // ------------------------------------------------------------------ vẽ biểu đồ Gantt
    fn ve(&self, painter: &egui::Painter, khung: Rect) {
        painter.rect_filled(khung, 0.0, NEN);
        painter.rect_stroke(khung, 0.0, net(1.0, LUOI_DAM));
        let goc = khung.min;
        let p = |x: f64, y: f64| Pos2::new(goc.x + x as f32, goc.y + y as f32);
        let (w, h_cv) = (khung.width().max(200.0) as f64, khung.height().max(200.0) as f64);
        let d = self.do_dai();
        let t = (self.t_xem as i64).min(d);
        if self.ket_qua.is_empty() {
            painter.text(p(w / 2.0, h_cv / 2.0), Align2::CENTER_CENTER, "Chọn một chế độ để bắt đầu mô phỏng", FontId::proportional(18.0), CHU_MO);
            return;
        }
        let (trai, phai) = (110.0, w - 20.0);
        let x_cua = |n: f64| trai + (phai - trai) * n / (d.max(1) as f64);
        let so_hang: f64 = self.ket_qua.iter().map(|(_, kq)| kq.lan.len() as f64 + if kq.io.is_empty() { 0.0 } else { 1.0 } + 1.2).sum();
        let h = 48f64.min((h_cv - 70.0) / so_hang); // chừa chỗ cho trục và hai hàng chú giải
        // tính trước đáy biểu đồ để vẽ lưới bên dưới các làn
        let mut y_cuoi = 8.0;
        for (_, kq) in &self.ket_qua {
            y_cuoi += h * 0.9 + h * kq.lan.len() as f64 + if kq.io.is_empty() { 0.0 } else { h } + h * 0.3;
        }
        // trục thời gian
        for g in (0..=d).step_by(5) {
            let x = x_cua(g as f64);
            painter.line_segment([p(x, 4.0), p(x, y_cuoi)], net(1.0, if g % 10 != 0 { LUOI } else { LUOI_DAM }));
            if g % 10 == 0 {
                painter.text(p(x, y_cuoi + 10.0), Align2::CENTER_CENTER, format!("{} s", g / 10), FontId::proportional(12.0), CHU_MO);
            }
        }
        let mut y = 8.0;
        for (m, kq) in &self.ket_qua {
            painter.text(p(8.0, y + h * 0.4), Align2::LEFT_CENTER, l::ten_che_do(m), FontId::proportional(13.5), CHU);
            // chữ đậm giả: vẽ lệch 0,6 px (phông nạp thêm chỉ có một độ đậm)
            painter.text(p(8.6, y + h * 0.4), Align2::LEFT_CENTER, l::ten_che_do(m), FontId::proportional(13.5), CHU);
            y += h * 0.9;
            for (ten, o) in &kq.lan {
                self.ve_lan(painter, &p, &x_cua, ten, o, y, h, t);
                y += h;
            }
            if !kq.io.is_empty() {
                self.ve_io(painter, &p, &x_cua, kq, y, h, t);
                y += h;
            }
            y += h * 0.3;
        }
        // đường "bây giờ"
        let xt = x_cua(self.t_xem);
        painter.line_segment([p(xt, 2.0), p(xt, y)], net(2.0, mau(0xf2b705)));
        self.ve_chu_giai(painter, &p, w, y + 26.0);
    }

    #[allow(clippy::too_many_arguments)]
    fn ve_lan(&self, painter: &egui::Painter, p: &dyn Fn(f64, f64) -> Pos2, x_cua: &dyn Fn(f64) -> f64, ten: &str, o: &[l::O], y: f64, h: f64, t: i64) {
        painter.text(p(104.0, y + h / 2.0), Align2::RIGHT_CENTER, ten, FontId::proportional(13.0), CHU);
        painter.rect_filled(Rect::from_min_max(p(x_cua(0.0), y + 2.0), p(x_cua(o.len() as f64), y + h - 2.0)), 0.0, mau(0xf6f7f9));
        let n = o.len() as i64;
        let mut j = 0i64;
        while j < n.min(t + 1) {
            // gộp các nhịp liên tiếp cùng trạng thái thành một khối
            let mut k = j;
            while k + 1 < n && k < t && o[(k + 1) as usize] == o[j as usize] {
                k += 1;
            }
            let c = &o[j as usize];
            let x0 = x_cua(j as f64);
            let x1 = if k >= t { x_cua(((k + 1) as f64).min(self.t_xem)) } else { x_cua((k + 1) as f64) };
            let (dam, nhat) = mau_viec(&c.nhan);
            let hop = |tren: f64, duoi: f64| Rect::from_min_max(p(x0, y + tren), p(x1, y + h - duoi));
            match c.tt {
                TrangThai::Cpu => {
                    painter.rect_filled(hop(3.0, 3.0), 0.0, dam);
                    if x1 - x0 > 18.0 {
                        painter.text(p((x0 + x1) / 2.0, y + h / 2.0), Align2::CENTER_CENTER, &c.nhan, FontId::proportional(12.0), Color32::WHITE);
                    }
                }
                TrangThai::Io => {
                    let r = hop(6.0, 6.0);
                    painter.rect_filled(r, 0.0, nhat);
                    // gạch chéo (thay cho stipple của Tk)
                    let cat = painter.with_clip_rect(r.intersect(painter.clip_rect()));
                    let mut x = r.left() - r.height();
                    while x < r.right() {
                        cat.line_segment([Pos2::new(x, r.bottom()), Pos2::new(x + r.height(), r.top())], net(1.0, dam.gamma_multiply(0.45)));
                        x += 5.0;
                    }
                    painter.rect_stroke(r, 0.0, net(1.0, dam));
                    if x1 - x0 > 40.0 {
                        painter.text(r.center(), Align2::CENTER_CENTER, format!("{} chờ I/O", c.nhan), FontId::proportional(11.0), CHU);
                    }
                }
                TrangThai::SanSang => {
                    vien_dut(painter, hop(5.0, 5.0), net(1.0, dam), 3.0, 2.0);
                }
                TrangThai::Cho => {
                    let r = hop(8.0, 8.0);
                    painter.rect_filled(r, 0.0, XAM);
                    vien_dut(painter, r, net(1.0, VIEN_XAM), 1.0, 2.0);
                    if x1 - x0 > 50.0 {
                        painter.text(r.center(), Align2::CENTER_CENTER, "chờ join", FontId::proportional(11.0), CHU_MO);
                    }
                }
                TrangThai::Ranh => {}
            }
            j = k + 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ve_io(&self, painter: &egui::Painter, p: &dyn Fn(f64, f64) -> Pos2, x_cua: &dyn Fn(f64) -> f64, kq: &KetQua, y: f64, h: f64, t: i64) {
        painter.text(p(104.0, y + h / 2.0), Align2::RIGHT_CENTER, "I/O chờ", FontId::proportional(13.0), CHU);
        painter.rect_filled(Rect::from_min_max(p(x_cua(0.0), y + 2.0), p(x_cua(kq.tong as f64), y + h - 2.0)), 0.0, mau(0xf6fbf3));
        let n_max = kq.io.iter().copied().max().unwrap_or(1).max(1) as f64;
        for (j, &n) in kq.io.iter().enumerate().take((t + 1).max(0) as usize) {
            if n > 0 {
                let hh = (h - 6.0) * n as f64 / n_max;
                painter.rect_filled(Rect::from_min_max(p(x_cua(j as f64), y + h - 3.0 - hh), p(x_cua(j as f64 + 1.0), y + h - 3.0)), 0.0, IO_XANH);
            }
        }
    }

    /// Chú giải: mỗi mục một ô mẫu + chữ; vị trí tính theo độ rộng chữ thật để không chồng nhau.
    fn ve_chu_giai(&self, painter: &egui::Painter, p: &dyn Fn(f64, f64) -> Pos2, w: f64, mut y: f64) {
        let phong = FontId::proportional(13.0);
        let (t1, t1_nhat) = mau_viec("T1");
        let mut muc: Vec<(u8, Color32, Color32, String)> = ["T1", "T2", "T3", "U"].iter().map(|t| (0u8, mau_viec(t).0, mau_viec(t).0, format!("{t} dùng CPU"))).collect();
        muc.push((1, t1_nhat, t1, "bị chặn chờ I/O".into()));
        muc.push((2, NEN, t1, "sẵn sàng, chờ lõi".into()));
        muc.push((3, XAM, VIEN_XAM, "chờ join".into()));
        muc.push((0, IO_XANH, IO_XANH, "I/O bất đồng bộ (không giữ luồng)".into()));
        let mut x = 20.0;
        for (kieu, nen, vien, chu) in muc {
            let rong = painter.layout_no_wrap(chu.clone(), phong.clone(), CHU).size().x as f64;
            if x + 23.0 + rong > w - 10.0 {
                // hết chỗ: xuống hàng
                x = 20.0;
                y += 20.0;
            }
            let r = Rect::from_min_max(p(x, y - 6.0), p(x + 18.0, y + 6.0));
            match kieu {
                0 => {
                    painter.rect_filled(r, 0.0, nen);
                }
                1 => {
                    painter.rect_filled(r, 0.0, nen);
                    for i in 0..4 {
                        let x0 = r.left() + 5.0 * i as f32 - 6.0;
                        painter.with_clip_rect(r).line_segment([Pos2::new(x0, r.bottom()), Pos2::new(x0 + 12.0, r.top())], net(1.0, vien.gamma_multiply(0.45)));
                    }
                    painter.rect_stroke(r, 0.0, net(1.0, vien));
                }
                2 => vien_dut(painter, r, net(1.0, vien), 3.0, 2.0),
                _ => {
                    painter.rect_filled(r, 0.0, nen);
                    vien_dut(painter, r, net(1.0, vien), 1.0, 2.0);
                }
            }
            painter.text(p(x + 23.0, y), Align2::LEFT_CENTER, &chu, phong.clone(), CHU);
            x += 23.0 + rong + 16.0;
        }
    }
}

/// Một ô của bảng chỉ số, chữ căn giữa cột (như cột anchor="center" của Treeview).
fn o_giua(ui: &mut egui::Ui, chu: RichText) {
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| ui.label(chu));
}

/// Nét vẽ độ dày `rong` px.
fn net(rong: f32, mau: Color32) -> Stroke {
    Stroke::new(rong, mau)
}

/// Viền nét đứt quanh hình chữ nhật.
fn vien_dut(painter: &egui::Painter, r: Rect, stroke: Stroke, net: f32, cach: f32) {
    let duong = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    painter.extend(Shape::dashed_line(&duong, stroke, net, cach));
}

/// Khung nhóm có tiêu đề (giống LabelFrame của Tk).
fn nhom(ui: &mut egui::Ui, tieu_de: &str, noi_dung: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(tieu_de).strong());
            noi_dung(ui);
        });
    });
}

impl eframe::App for UngDung {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.nhip();
        if self.demo.is_some() {
            demo_buoc(self, ctx);
        }
        egui::TopBottomPanel::top("tren").show(ctx, |ui| {
            ui.add_space(4.0);
            self.thanh_tren(ui);
            ui.add_space(4.0);
        });
        egui::SidePanel::right("nhat_ky").exact_width(360.0).resizable(false).show(ctx, |ui| {
            nhom(ui, "Nhật ký sự kiện", |ui| self.nhat_ky(ui));
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            let cao_duoi = 4.0 * 20.0 + 22.0 + 10.0 + 30.0;
            let kich_thuoc = Vec2::new(ui.available_width(), (ui.available_height() - cao_duoi).max(200.0));
            let (khung, painter) = ui.allocate_painter(kich_thuoc, egui::Sense::hover());
            self.ve(&painter, khung.rect);
            // thanh kéo thời gian: chỉ có tác dụng khi hoạt hình đang dừng
            let d = self.do_dai();
            let mut tam = self.t_xem;
            ui.spacing_mut().slider_width = ui.available_width();
            let thanh = ui.add_enabled(d > 0, egui::Slider::new(&mut tam, 0.0..=(d.max(1) as f64)).show_value(false));
            if thanh.changed() && !self.dang_chay && d > 0 {
                self.t_xem = tam;
            }
            self.bang_chi_so(ui);
        });
        if self.dang_chay || self.demo.is_some() {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
}

// ---------------------------------------------------------------------- kịch bản minh họa (--demo)
struct Demo {
    buoc: u32,
    moc: Instant, // thời điểm bắt đầu bước hiện tại
    t0: Instant,  // lúc bắt đầu hoạt hình (đo thời gian thật)
    so_lan_ve: u32,
    anh1: Option<Arc<egui::ColorImage>>,
}

fn anh_chup(ctx: &egui::Context) -> Option<Arc<egui::ColorImage>> {
    ctx.input(|i| {
        i.raw.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    })
}

/// Mỗi khung hình làm một bước của kịch bản (thay cho vòng `root.update()` của bản Tkinter).
fn demo_buoc(app: &mut UngDung, ctx: &egui::Context) {
    let mut dm = app.demo.take().unwrap();
    let qua = |ms: u64, moc: Instant| moc.elapsed() >= Duration::from_millis(ms);
    match dm.buoc {
        0 if qua(500, dm.moc) => {
            println!("[gui] Bấm 'Đa luồng – đồng bộ', dừng ở 3.8 s: U đã chen ngang Luồng T1, hai nửa T1 chạy trên Luồng T1 và T1b");
            app.chon(&["da_dong_bo"]);
            app.dang_chay = false;
            app.t_xem = 38.0;
            dm.buoc = 1;
            dm.moc = Instant::now();
        }
        1 if qua(300, dm.moc) => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            dm.buoc = 2;
        }
        2 => {
            if let Some(anh) = anh_chup(ctx) {
                dm.anh1 = Some(anh);
                println!("[gui] Bấm 'Đa luồng – bất đồng bộ', chạy hết: T2 chạy tiếp trên thợ khác sau I/O");
                app.chon(&["da_bat_dong_bo"]);
                app.toc_do = 4; // × 4
                dm.t0 = Instant::now();
                dm.so_lan_ve = 0;
                dm.buoc = 3;
            }
        }
        3 => {
            dm.so_lan_ve += 1;
            if !app.dang_chay || dm.t0.elapsed() > Duration::from_secs(10) {
                println!(
                    "[gui] Hoạt hình chạy xong sau {:.1} s thật, vẽ lại {} lần (giao diện không đứng)",
                    dm.t0.elapsed().as_secs_f64(),
                    dm.so_lan_ve
                );
                println!("[gui] Bấm 'So sánh 4 chế độ', tua tới cuối");
                app.chon(&CHE_DO);
                app.dang_chay = false;
                app.t_xem = app.do_dai() as f64;
                dm.buoc = 4;
                dm.moc = Instant::now();
            }
        }
        4 if qua(300, dm.moc) => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            dm.buoc = 5;
        }
        5 => {
            if let Some(anh2) = anh_chup(ctx) {
                for (m, kq) in &app.ket_qua {
                    let s = l::chi_so(kq);
                    let u = if s.u_cho >= 0 { format!("{} s", l::giay(s.u_cho)) } else { "–".into() };
                    println!("[gui]   {:<14} hoàn tất {} s, luồng {}, U chờ {u}, chen ngang {}", l::ten_ngan(m), l::giay(s.tong), s.luong, s.chen);
                }
                println!("[gui] Bỏ việc khẩn, bấm 'Đa luồng – đồng bộ', dừng ở 3.3 s rồi bấm 'Chen ngang U ngay'");
                app.khan = false;
                app.chon(&["da_dong_bo"]);
                app.dang_chay = false;
                app.t_xem = 33.0;
                app.chen_ngang();
                let s = l::chi_so(&app.ket_qua[0].1);
                println!("[gui]   U đến 3.3 s, chờ {} s, chen ngang {} lần, hoàn tất {} s", l::giay(s.u_cho), s.chen, l::giay(s.tong));
                let anh1 = dm.anh1.take().expect("đã chụp ảnh 1");
                match ghi_png_ghep("ket_qua_gui.png", &[&anh1, &anh2]) {
                    Ok(()) => println!("[gui] Đã chụp cửa sổ: ket_qua_gui.png"),
                    Err(e) => println!("[gui] Không ghi được ket_qua_gui.png: {e}"),
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                dm.buoc = 6;
            }
        }
        _ => {}
    }
    app.demo = Some(dm);
    ctx.request_repaint();
}

// ---------------------------------------------------------------------- ghi PNG (thay cho ImageMagick)
fn crc32(du_lieu: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in du_lieu {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// Ghép các ảnh chụp theo chiều dọc, mỗi ảnh có viền 4 px màu #d0d4dc (như `convert -border 4 -append`),
/// rồi ghi thành PNG RGB (nén zlib bằng miniz_oxide).
fn ghi_png_ghep(duong_dan: &str, anh: &[&egui::ColorImage]) -> std::io::Result<()> {
    const VIEN: usize = 4;
    let mau_vien = [0xd0u8, 0xd4, 0xdc];
    let w = anh.iter().map(|a| a.size[0] + 2 * VIEN).max().unwrap_or(0);
    let h: usize = anh.iter().map(|a| a.size[1] + 2 * VIEN).sum();
    let mut rgb = vec![0u8; w * h * 3];
    for px in rgb.chunks_mut(3) {
        px.copy_from_slice(&mau_vien);
    }
    let mut y0 = 0;
    for a in anh {
        for y in 0..a.size[1] {
            for x in 0..a.size[0] {
                let c = a.pixels[y * a.size[0] + x];
                let i = ((y0 + VIEN + y) * w + VIEN + x) * 3;
                rgb[i..i + 3].copy_from_slice(&[c.r(), c.g(), c.b()]);
            }
        }
        y0 += a.size[1] + 2 * VIEN;
    }
    let mut hang = Vec::with_capacity((w * 3 + 1) * h);
    for y in 0..h {
        hang.push(0); // bộ lọc 0: không lọc
        hang.extend_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]);
    }
    let khoi = |out: &mut Vec<u8>, kieu: &[u8], du_lieu: &[u8]| {
        out.extend_from_slice(&(du_lieu.len() as u32).to_be_bytes());
        let mut kd = kieu.to_vec();
        kd.extend_from_slice(du_lieu);
        out.extend_from_slice(&kd);
        out.extend_from_slice(&crc32(&kd).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bit/kênh, kiểu màu 2 = RGB
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    khoi(&mut out, b"IHDR", &ihdr);
    khoi(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&hang, 9));
    khoi(&mut out, b"IEND", &[]);
    std::fs::write(duong_dan, out)
}

// ---------------------------------------------------------------------- phông chữ, khởi động
/// Phông mặc định của egui thiếu nhiều chữ tiếng Việt có dấu (ế, ộ, ữ…), nên nạp thêm một phông hệ thống
/// có đủ dấu. Không tìm thấy thì giữ phông mặc định (chữ có dấu thành ô vuông).
fn cai_phong_chu(ctx: &egui::Context) {
    let mut ung_vien: Vec<String> = Vec::new();
    if let Ok(win) = std::env::var("WINDIR") {
        for t in ["segoeui.ttf", "arial.ttf", "tahoma.ttf"] {
            ung_vien.push(format!("{win}\\Fonts\\{t}"));
        }
    }
    for p in [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/Library/Fonts/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ] {
        ung_vien.push(p.to_string());
    }
    let Some(du_lieu) = ung_vien.iter().find_map(|p| std::fs::read(p).ok()) else { return };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("he_thong".into(), egui::FontData::from_owned(du_lieu));
    // đặt lên đầu danh sách: chữ nào phông hệ thống có thì dùng phông này
    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "he_thong".into());
    fonts.families.entry(egui::FontFamily::Monospace).or_default().push("he_thong".into());
    ctx.set_fonts(fonts);
}

fn main() {
    let demo = std::env::args().any(|a| a == "--demo");
    let mut app = UngDung::new();
    if demo {
        let now = Instant::now();
        app.demo = Some(Demo { buoc: 0, moc: now, t0: now, so_lan_ve: 0, anh1: None });
    }
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 860.0]).with_title("Mô phỏng thực thi tác vụ – MR5.6"),
        ..Default::default()
    };
    let kq = eframe::run_native(
        "Mô phỏng thực thi tác vụ – MR5.6",
        opt,
        Box::new(|cc| {
            cai_phong_chu(&cc.egui_ctx);
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            cc.egui_ctx.style_mut(|s| s.spacing.item_spacing = egui::vec2(8.0, 6.0));
            Ok(Box::new(app))
        }),
    );
    if let Err(e) = kq {
        eprintln!("Không mở được cửa sổ: {e}");
        std::process::exit(1);
    }
    if demo {
        std::process::exit(0); // thoát ngay, không chờ dọn dẹp ngữ cảnh OpenGL
    }
}
