//! Bài mở rộng MR6.12 – Ứng dụng ngân hàng số (ứng dụng người dùng, egui).
//!
//! Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở gui_chung::CuaSoNguoiDung; tệp này vẽ
//! ứng dụng ngân hàng: thẻ tài khoản (nền chuyển màu, chip, số tài khoản, chủ thẻ, số dư khả dụng), chọn
//! tài khoản của tôi, lịch sử giao dịch (tiền ra đỏ "−", tiền vào xanh "+"), khung chuyển tiền (người nhận,
//! số tiền + nút chọn nhanh, nội dung, nút Chuyển tiền), dòng thông báo kết quả (thành công, không đủ số
//! dư, khóa chết – giao dịch bị hủy), dòng "+ Nhận … từ …" khi có tiền đến, và tình trạng hệ thống (tổng
//! tiền LỆCH khi có mất cập nhật). Số liệu đến từ cap_nhat của máy chủ (≤ 10 lần/giây).
//!
//! Chạy:  ngan_hang_gui <tên> [host] [cổng]       (mặc định 127.0.0.1 44762)

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::{so_du_cua, CONG_GUI};
use eframe::egui::{self, Color32, RichText, Stroke};
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};

pub const TEN_NGAN_HANG: &str = "NGÂN HÀNG SỐ LTM";
const NHANH: [(i64, &str); 4] = [(100_000, "100K"), (500_000, "500K"), (1_000_000, "1 triệu"), (2_000_000, "2 triệu")];

fn tron(c1: Color32, c2: Color32, t: f32) -> Color32 {
    let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgb(f(c1.r(), c2.r()), f(c1.g(), c2.g()), f(c1.b(), c2.b()))
}

/// Vẽ thẻ tài khoản: nền chuyển màu, góc bo, chip, số tài khoản, chủ tài khoản, số dư.
pub fn ve_the(pt: &egui::Painter, r: egui::Rect, ma: &str, chu: &str, so_du: i64) {
    let (tren, duoi) = (mau("#0b3d91"), mau("#1e88e5"));
    let (w, h) = (r.width(), r.height());
    let g = r.min;
    pt.rect_filled(r, 0.0, NEN);
    // nền chuyển màu từ trên xuống, góc bo 16: hình chữ nhật bo góc (lồi) tô bằng lưới tam giác hình quạt,
    // màu mỗi đỉnh nội suy theo độ cao
    let bo = 16.0;
    let mut vien: Vec<egui::Pos2> = Vec::new();
    for (cx, cy, a0) in [(g.x + w - bo, g.y + bo, -90.0f32), (g.x + w - bo, g.y + h - bo, 0.0), (g.x + bo, g.y + h - bo, 90.0), (g.x + bo, g.y + bo, 180.0)] {
        for k in 0..=8 {
            let a = (a0 + 90.0 * k as f32 / 8.0).to_radians();
            vien.push(egui::pos2(cx + bo * a.cos(), cy + bo * a.sin()));
        }
    }
    let mut mesh = egui::Mesh::default();
    let tam = r.center();
    mesh.colored_vertex(tam, tron(tren, duoi, 0.5));
    for d in &vien {
        mesh.colored_vertex(*d, tron(tren, duoi, ((d.y - g.y) / h).clamp(0.0, 1.0)));
    }
    let n = vien.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    pt.add(egui::Shape::mesh(mesh));
    let pt2 = pt.with_clip_rect(r.shrink(1.0));
    pt2.circle_stroke(p(g, w - 45.0, 45.0), 105.0, Stroke::new(2.0_f32, mau("#5c9ce6"))); // họa tiết
    pt2.circle_stroke(p(g, w - 10.0, 70.0), 100.0, Stroke::new(1.0_f32, mau("#4a8ad8")));
    let chu_ = |x: f32, y: f32, s: &str, can: egui::Align2, co: f32, m: Color32| {
        pt.text(p(g, x, y), can, s, egui::FontId::proportional(co), m);
    };
    chu_(18.0, 20.0, TEN_NGAN_HANG, egui::Align2::LEFT_CENTER, 14.0, Color32::WHITE);
    chu_(w - 18.0, 20.0, "THANH TOÁN", egui::Align2::RIGHT_CENTER, 10.5, mau("#cfe3ff"));
    pt.rect(egui::Rect::from_min_max(p(g, 20.0, 44.0), p(g, 62.0, 74.0)), 0.0, mau("#e8c766"), Stroke::new(1.0_f32, mau("#a8862e"))); // chip
    for y in [54.0, 64.0] {
        pt.line_segment([p(g, 20.0, y), p(g, 62.0, y)], Stroke::new(1.0_f32, mau("#a8862e")));
    }
    pt.line_segment([p(g, 41.0, 44.0), p(g, 41.0, 74.0)], Stroke::new(1.0_f32, mau("#a8862e")));
    for (i, rr) in [8.0f32, 14.0, 20.0].iter().enumerate() {
        // biểu tượng không tiếp xúc
        let cung: Vec<egui::Pos2> = (0..=10).map(|k| {
            let a = (-40.0 + 8.0 * k as f32).to_radians();
            p(g, 70.0 + rr * a.cos(), 59.0 - rr * a.sin())
        }).collect();
        pt.add(egui::Shape::line(cung, Stroke::new(if i == 0 { 2.0_f32 } else { 1.0_f32 }, Color32::WHITE)));
    }
    chu_(110.0, 50.0, "Số tài khoản", egui::Align2::LEFT_CENTER, 10.5, mau("#cfe3ff"));
    pt.text(p(g, 110.0, 68.0), egui::Align2::LEFT_CENTER, format!("{ma}  ••••  6125"), egui::FontId::monospace(15.0), Color32::WHITE);
    chu_(20.0, 98.0, "Số dư khả dụng", egui::Align2::LEFT_CENTER, 11.5, mau("#cfe3ff"));
    chu_(20.0, 126.0, &tien(so_du), egui::Align2::LEFT_CENTER, 27.0, Color32::WHITE);
    chu_(20.0, h - 22.0, &format!("CHỦ TÀI KHOẢN: {}", chu.to_uppercase()), egui::Align2::LEFT_CENTER, 11.5, Color32::WHITE);
    chu_(w - 18.0, h - 22.0, "VND", egui::Align2::RIGHT_CENTER, 11.5, mau("#cfe3ff"));
}

fn ds_tk(tt: &Obj) -> Vec<(String, String)> {
    tt.get("tai_khoan").and_then(Value::as_array).into_iter().flatten().map(|t| (so_us(t.get("ma")), so_us(t.get("chu")))).collect()
}

fn ds_gd(tt: &Obj) -> Vec<Obj> {
    tt.get("lich_su").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_object().cloned()).collect()).unwrap_or_default()
}

pub struct UngDungNganHang {
    pub tk: Option<String>,   // mã tài khoản của tôi
    da_thay: HashSet<i64>,    // ma_gd đã đưa vào lịch sử
    pub so_chuyen: u64,
    pub so_thanh_cong: u64,
    pub so_nhan: u64,
    pub so_kq: u64,
    so_gd_cu: i64,
    ls: VecDeque<(String, Color32)>,
    pub den: String,          // "TK02 – Bình"
    pub o_tien: String,
    o_noi_dung: String,
    pub thong_bao: (String, Color32, Color32),
    pub nhan_den: (String, Color32),
    nut_bat: bool,
}

impl UngDungNganHang {
    fn chu(&self, nd: &NguoiDung, ma: &str) -> String {
        nd.tt.as_ref().and_then(|tt| ds_tk(tt).into_iter().find(|(m, _)| m == ma)).map_or("?".into(), |x| x.1)
    }

    fn so_du(&self, nd: &NguoiDung) -> i64 {
        match (&nd.tt, &self.tk) {
            (Some(tt), Some(tk)) => so_du_cua(tt, tk),
            _ => 0,
        }
    }

    fn bao(&mut self, text: &str, nen: Color32, chu: Color32) {
        self.thong_bao = (text.to_string(), nen, chu);
    }

    /// Thêm một giao dịch vào đầu lịch sử: tiền ra (đỏ, dấu −) hoặc tiền vào (xanh, dấu +).
    fn them_ls(&mut self, nd: &NguoiDung, g: &Obj, ra: bool, ghi_chu: &str) {
        let doi = lay_str(g, if ra { "den" } else { "tu" }).unwrap_or("").to_string();
        let nd_gd = lay_str(g, "noi_dung").filter(|s| !s.is_empty()).map_or(String::new(), |s| format!(" · {s}"));
        let dong = format!(
            "{} {}{:>13} {} {doi} {}{nd_gd}{ghi_chu}",
            so_us(g.get("gio")),
            if ra { "−" } else { "+" },
            tien(lay_i64(g, "so_tien").unwrap_or(0)),
            if ra { "tới" } else { "từ " },
            self.chu(nd, &doi)
        );
        self.ls.push_front((dong, if ra { DO_DAM } else { XANH_DAM }));
        self.ls.truncate(100);
    }

    fn dien_den(&mut self, nd: &NguoiDung) {
        let Some(tt) = &nd.tt else { return };
        let ds: Vec<String> = ds_tk(tt).into_iter().filter(|(m, _)| Some(m) != self.tk.as_ref()).map(|(m, c)| format!("{m} – {c}")).collect();
        if !ds.contains(&self.den) {
            if let Some(d) = ds.first() {
                self.den = d.clone();
            }
        }
    }

    fn doi_tk(&mut self, nd: &mut NguoiDung, ma: &str) {
        if self.tk.as_deref() == Some(ma) {
            return;
        }
        self.tk = Some(ma.to_string());
        self.ls.clear();
        self.da_thay = nd.tt.as_ref().map(ds_gd).unwrap_or_default().iter().filter_map(|g| lay_i64(g, "ma")).collect();
        self.dien_den(nd);
        nd.nhat_ky.ghi(&format!("Đăng nhập {ma} – {}, số dư {}", self.chu(nd, ma), tien(self.so_du(nd))));
    }

    /// Chọn tài khoản đích (kịch bản demo) – như người dùng chọn trong danh sách.
    pub fn chon_nguoi_nhan(&mut self, nd: &NguoiDung, ma: &str) {
        self.den = format!("{ma} – {}", self.chu(nd, ma));
    }

    /// Bấm "Chuyển tiền".
    pub fn bam_chuyen(&mut self, nd: &mut NguoiDung) {
        let den = self.den.split(' ').next().unwrap_or("").to_string();
        let s: String = self.o_tien.chars().filter(|c| !matches!(c, '.' | ' ' | 'đ')).collect();
        let Ok(x) = s.parse::<i64>() else {
            self.bao("Số tiền không hợp lệ", mau("#ffb347"), Color32::BLACK);
            return;
        };
        let tk = self.tk.clone().unwrap_or_default();
        self.so_chuyen += 1;
        nd.nhat_ky.ghi(&format!("Chuyển {} {tk}→{den}…", tien(x)));
        self.bao(&format!("Đang xử lý: {} tới {den}…", tien(x)), mau("#fff3c4"), Color32::BLACK);
        let noi_dung: String = self.o_noi_dung.trim().chars().take(40).collect();
        nd.gui_yc("chuyen", obj(json!({"tu": tk, "den": den, "so_tien": x, "noi_dung": noi_dung})));
    }

    fn dong_dem(&self) -> String {
        format!("Đã chuyển {} lần · thành công {} · nhận {}", self.so_chuyen, self.so_thanh_cong, self.so_nhan)
    }
}

impl UngDung for UngDungNganHang {
    const TIEU_DE: &'static str = "Ngân hàng số";

    fn moi(_ten: &str) -> Self {
        UngDungNganHang {
            tk: None,
            da_thay: HashSet::new(),
            so_chuyen: 0,
            so_thanh_cong: 0,
            so_nhan: 0,
            so_kq: 0,
            so_gd_cu: 0,
            ls: VecDeque::new(),
            den: String::new(),
            o_tien: "500.000".into(),
            o_noi_dung: "Chuyển tiền".into(),
            thong_bao: ("Chưa kết nối".into(), Color32::from_gray(0xe0), Color32::BLACK),
            nhan_den: (String::new(), NEN),
            nut_bat: false,
        }
    }

    fn ve(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung) {
        let tt = nd.tt.clone();
        ui.horizontal_top(|ui| {
            // ---- trái: thẻ, chọn tài khoản, lịch sử
            ui.vertical(|ui| {
                ui.set_width(344.0);
                let (r, pt) = ui.allocate_painter(egui::vec2(340.0, 190.0), egui::Sense::hover());
                let tk = self.tk.clone().unwrap_or_else(|| "TK??".into());
                let chu = if self.tk.is_some() { self.chu(nd, &tk) } else { "–".into() };
                ve_the(&pt, r.rect, &tk, &chu, self.so_du(nd));
                ui.add_space(4.0);
                let mut doi: Option<String> = None;
                ui.horizontal(|ui| {
                    ui.label("Tài khoản của tôi:");
                    let hien = self.tk.as_ref().map_or(String::new(), |m| format!("{m} – {}", self.chu(nd, m)));
                    egui::ComboBox::from_id_salt(("tk", nd.ten.clone())).width(150.0).selected_text(hien).show_ui(ui, |ui| {
                        for (m, c) in tt.as_ref().map(ds_tk).unwrap_or_default() {
                            if ui.selectable_label(self.tk.as_deref() == Some(m.as_str()), format!("{m} – {c}")).clicked() {
                                doi = Some(m);
                            }
                        }
                    });
                });
                if let Some(m) = doi {
                    self.doi_tk(nd, &m);
                }
                nhom(ui, "Lịch sử giao dịch (mới nhất trên cùng)", |ui| {
                    let dong: Vec<(String, Color32)> = self.ls.iter().cloned().collect();
                    danh_sach(ui, &dong, 9, 330.0, 10.5);
                });
            });
            // ---- phải: chuyển tiền, thông báo
            ui.vertical(|ui| {
                ui.set_width(300.0);
                let mut bam = false;
                nhom(ui, "Chuyển tiền", |ui| {
                    egui::Grid::new(("ck", nd.ten.clone())).num_columns(2).spacing([4.0, 4.0]).show(ui, |ui| {
                        ui.label("Đến tài khoản:");
                        let ds: Vec<String> = tt.as_ref().map(ds_tk).unwrap_or_default().into_iter().filter(|(m, _)| Some(m) != self.tk.as_ref()).map(|(m, c)| format!("{m} – {c}")).collect();
                        egui::ComboBox::from_id_salt(("den", nd.ten.clone())).width(150.0).selected_text(self.den.clone()).show_ui(ui, |ui| {
                            for d in ds {
                                ui.selectable_value(&mut self.den, d.clone(), d);
                            }
                        });
                        ui.end_row();
                        ui.label("Số tiền (đ):");
                        ui.add(egui::TextEdit::singleline(&mut self.o_tien).desired_width(110.0).horizontal_align(egui::Align::RIGHT).font(egui::FontId::proportional(14.0)));
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        for (x, chu) in NHANH {
                            if nut(ui, chu, mau("#e8eef8"), true, 56.0) {
                                self.o_tien = tien(x).trim_end_matches(" đ").to_string();
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Nội dung:");
                        o_nhap(ui, &mut self.o_noi_dung, 170.0, true);
                    });
                    ui.vertical_centered(|ui| {
                        if nut(ui, "Chuyển tiền", mau("#ff8c00"), self.nut_bat && nd.kn.is_some(), 140.0) {
                            bam = true;
                        }
                    });
                });
                if bam {
                    self.bam_chuyen(nd);
                }
                let (text, nen, mc) = self.thong_bao.clone();
                egui::Frame::none().fill(nen).inner_margin(egui::Margin::symmetric(6.0, 4.0)).show(ui, |ui| {
                    ui.set_width(286.0);
                    ui.set_min_height(46.0);
                    ui.add(egui::Label::new(RichText::new(text).strong().size(12.5).color(mc)).wrap());
                });
                let (text, nen) = self.nhan_den.clone();
                egui::Frame::none().fill(nen).inner_margin(egui::Margin::symmetric(6.0, 2.0)).show(ui, |ui| {
                    ui.set_width(286.0);
                    ui.add(egui::Label::new(RichText::new(text).strong().size(11.5).color(mau("#0050a0"))).wrap());
                });
                ui.label(RichText::new(self.dong_dem()).size(10.5));
                if let Some(tt) = &tt {
                    let lech = lay_i64(tt, "tong").unwrap_or(0) - lay_i64(tt, "tong_dau").unwrap_or(0);
                    let mut chu = format!("Máy chủ: {} tài khoản, {} giao dịch", ds_tk(tt).len(), so_us(tt.get("so_gd")));
                    if dung(tt, "khoa_ngay_tho") {
                        chu += " – khóa ngây thơ";
                    }
                    if lech != 0 {
                        chu += &format!("\nTổng tiền hệ thống LỆCH {}!", tien(lech));
                    }
                    ui.add(egui::Label::new(RichText::new(chu).size(10.5).color(if lech != 0 { DO_DAM } else { mau("#404040") })).wrap());
                }
            });
        });
    }

    fn ket_qua(&mut self, nd: &mut NguoiDung, _loai: &str, truong: &Obj, tin: &Obj, rtt: f64) {
        let (x, den) = (lay_i64(truong, "so_tien").unwrap_or(0), lay_str(truong, "den").unwrap_or("").to_string());
        self.so_kq += 1;
        if dung(tin, "ok") {
            self.so_thanh_cong += 1;
            self.da_thay.insert(lay_i64(tin, "ma_gd").unwrap_or(-1));
            if lay_str(truong, "tu") == self.tk.as_deref() {
                let mut g = obj(json!({"gio": gio_hien()}));
                g.extend(truong.clone());
                self.them_ls(nd, &g, true, "");
            }
            nd.nhat_ky.ghi(&format!("  thành công ({rtt:.1} ms), mã GD {}", so_us(tin.get("ma_gd"))));
            let text = format!("Chuyển thành công {} tới {den} – {}\nSố dư mới: {}", tien(x), self.chu(nd, &den), tien(lay_i64(tin, "so_du").unwrap_or(0)));
            self.bao(&text, mau("#70d070"), Color32::BLACK);
        } else {
            let ly_do = lay_str(tin, "ly_do").unwrap_or("").to_string();
            nd.nhat_ky.ghi(&format!("  thất bại ({rtt:.0} ms): {ly_do}"));
            match lay_str(tin, "ma_loi") {
                Some("khoa_cho_lau") => {
                    let mut c = ly_do.chars();
                    let hoa = c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str());
                    self.bao(&hoa, mau("#ff6060"), Color32::WHITE);
                }
                Some("khong_du") => self.bao(&format!("Không đủ số dư để chuyển {}", tien(x)), mau("#ffb347"), Color32::BLACK),
                _ => self.bao(&format!("Giao dịch thất bại: {ly_do}"), mau("#ffb347"), Color32::BLACK),
            }
        }
    }

    fn cap_nhat(&mut self, nd: &mut NguoiDung, tt: &Obj) {
        let so_gd = lay_i64(tt, "so_gd").unwrap_or(0);
        if so_gd < self.so_gd_cu {
            // máy chủ vừa đặt lại: số dư về ban đầu
            self.ls.clear();
            self.nhan_den = (String::new(), NEN);
            self.bao("Máy chủ đặt lại: số dư về ban đầu", Color32::from_gray(0xe0), Color32::BLACK);
            nd.nhat_ky.ghi("Máy chủ đặt lại: số dư về ban đầu");
        }
        self.so_gd_cu = so_gd;
        let ds = ds_tk(tt);
        if self.tk.as_ref().is_none_or(|tk| !ds.iter().any(|(m, _)| m == tk)) {
            self.tk = ds.iter().find(|(_, c)| *c == nd.ten).or(ds.first()).map(|x| x.0.clone());
            self.da_thay = ds_gd(tt).iter().filter_map(|g| lay_i64(g, "ma")).collect();
            self.ls.clear();
            let tk = self.tk.clone().unwrap_or_default();
            nd.nhat_ky.ghi(&format!("Đăng nhập {tk} – {}, số dư {}", self.chu(nd, &tk), tien(self.so_du(nd))));
            self.bao("Xin chào! Chọn người nhận và số tiền để chuyển.", Color32::from_gray(0xe0), Color32::BLACK);
        }
        self.dien_den(nd);
        self.nut_bat = true;
        let tk = self.tk.clone().unwrap_or_default();
        for g in ds_gd(tt).into_iter().rev() {
            // giao dịch mới liên quan tới tài khoản của tôi
            let ma = lay_i64(&g, "ma").unwrap_or(-1);
            if !self.da_thay.insert(ma) {
                continue;
            }
            let (tu, den) = (lay_str(&g, "tu").unwrap_or("").to_string(), lay_str(&g, "den").unwrap_or(""));
            if den == tk {
                self.so_nhan += 1;
                self.them_ls(nd, &g, false, "");
                let x = tien(lay_i64(&g, "so_tien").unwrap_or(0));
                nd.nhat_ky.ghi(&format!("+ Nhận {x} từ {tu} ({})", self.chu(nd, &tu)));
                self.nhan_den = (format!("+ Nhận {x} từ {tu} – {} lúc {}", self.chu(nd, &tu), so_us(g.get("gio"))), mau("#bfe8ff"));
            } else if tu == tk {
                // tiền ra do phiên khác của cùng tài khoản
                self.them_ls(nd, &g, true, &format!(" ({})", so_us(g.get("nguoi"))));
            }
        }
    }

    fn mat_ket_noi(&mut self, _nd: &mut NguoiDung) {
        self.nut_bat = false;
    }

    fn dong_dinh_ky(&self, nd: &NguoiDung) -> String {
        format!(
            "{} số dư {}; chuyển {}, thành công {}, nhận {}",
            self.tk.as_deref().unwrap_or("None"),
            tien(self.so_du(nd)),
            self.so_chuyen,
            self.so_thanh_cong,
            self.so_nhan
        )
    }
}

/// ngan_hang_gui [tên] [host] [cổng]
pub fn run(args: &[String]) {
    let ten = args.first().cloned().unwrap_or_else(|| "An".into());
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    let cs = CuaSoNguoiDung::<UngDungNganHang>::moi(&ten, &host, cong);
    chay_cua_so(&cs.tieu_de(), [700.0, 620.0], cs);
}
