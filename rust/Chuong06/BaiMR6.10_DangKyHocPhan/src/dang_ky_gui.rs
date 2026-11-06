//! Bài mở rộng MR6.10 – Cổng đăng ký học phần (ứng dụng người dùng, egui).
//!
//! Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở gui_chung::CuaSoNguoiDung; tệp này
//! vẽ trang đăng ký như cổng đào tạo thật: thanh tiêu đề, thông tin sinh viên (số lớp, số tín chỉ), bảng
//! lớp học phần (mã lớp, tên, tín chỉ, giảng viên, lịch, thanh "đã đăng ký/sĩ số", nút Đăng ký / Hủy; lớp
//! đủ hiện "Đã đủ"), dòng thông báo ("Đăng ký thành công", "Lớp đã đủ", "Bạn đã đăng ký lớp này") và bảng
//! "Kết quả đăng ký". Số liệu đến từ cap_nhat của máy chủ (≤ 10 lần/giây).
//!
//! Nếu máy chủ đã trả "thành công" mà danh sách lớp sau đó không có tên mình (mất cập nhật ở chế độ
//! khong_khoa), cửa sổ báo "đăng ký đã bị mất". cap_nhat có thể đến trễ hơn kq: lớp vừa có kq được ghi vào
//! tập cho_cap_nhat và bỏ qua khi đối chiếu cho tới khi có một cap_nhat MỚI tới sau (tránh báo nhầm "bị
//! MẤT" ngay sau khi Hủy, khi bản trạng thái đang giữ chụp trước lần ghi của mình).
//!
//! Chạy:  dang_ky_gui <tên> [host] [cổng]       (mặc định 127.0.0.1 44742)

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::CONG_GUI;
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const XANH_DH: &str = "#0b3d91"; // màu thanh tiêu đề cổng đào tạo
const COT: [(&str, f32); 7] = [("Mã lớp HP", 72.0), ("Tên học phần", 104.0), ("TC", 24.0), ("Giảng viên", 118.0), ("Lịch học", 118.0), ("Đã ĐK / Sĩ số", 118.0), ("", 66.0)];

fn ds_lop(tt: &Obj) -> Vec<Obj> {
    tt.get("lop").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_object().cloned()).collect()).unwrap_or_default()
}

fn co_ten(x: &Obj, ten: &str) -> bool {
    x.get("sv").and_then(Value::as_array).is_some_and(|a| a.iter().any(|s| s.as_str() == Some(ten)))
}

pub struct CongDangKy {
    pub cua_toi: BTreeMap<String, String>, // mã lớp -> giờ đăng ký (theo kết quả máy chủ)
    thieu_tu: HashMap<String, f64>,        // mã lớp -> lúc thấy danh sách lớp thiếu tên mình
    pub dang_cho: HashSet<String>,         // mã lớp đang chờ trả lời
    cho_cap_nhat: HashSet<String>,         // mã lớp vừa có kq, chưa có cap_nhat nào tới sau
    pub thong_bao: (String, Color32, Color32),
}

impl CongDangKy {
    fn bao(&mut self, text: &str, nen: Color32, chu: Color32) {
        self.thong_bao = (text.to_string(), nen, chu);
    }

    /// Chữ trên nút của hàng `ma` (như nút Tk được cấu hình lại ở mỗi cap_nhat).
    pub fn chu_nut(&self, nd: &NguoiDung, ma: &str) -> &'static str {
        if self.dang_cho.contains(ma) {
            return "…";
        }
        if self.cua_toi.contains_key(ma) {
            return "Hủy";
        }
        let day = nd.tt.as_ref().map(ds_lop).unwrap_or_default().iter().any(|x| lay_str(x, "ma") == Some(ma) && lay_i64(x, "da_dk") >= lay_i64(x, "si_so"));
        if day {
            "Đã đủ"
        } else {
            "Đăng ký"
        }
    }

    /// Bấm nút của hàng: Đăng ký (chưa có) hoặc Hủy (đã có).
    pub fn bam(&mut self, nd: &mut NguoiDung, ma: &str) {
        if nd.kn.is_none() || self.dang_cho.contains(ma) {
            return;
        }
        let loai = if self.cua_toi.contains_key(ma) { "huy" } else { "dang_ky" };
        self.dang_cho.insert(ma.to_string());
        nd.nhat_ky.ghi(&format!("{}{ma}", if loai == "huy" { "Hủy " } else { "Đăng ký " }));
        nd.gui_yc(loai, obj(json!({"ma_lop": ma})));
    }

    fn sau_cap_nhat(&mut self) {
        if self.thong_bao.0 == "Chưa kết nối" {
            self.bao("Đợt đăng ký đang mở – chọn lớp và bấm Đăng ký", Color32::from_gray(0xe0), Color32::BLACK);
        }
    }

    /// Đối chiếu kết quả mình nhận với danh sách lớp của máy chủ. cap_nhat có thể đến trễ hơn kq, nên chỉ
    /// báo khi danh sách thiếu tên mình liên tục 0,5 s, và bỏ qua lớp vừa có kq mà chưa có cap_nhat nào
    /// tới sau (bản tt đang giữ chụp trước lần ghi của mình – ví dụ vừa hủy mà tt cũ vẫn còn tên).
    fn kiem_mat(&mut self, nd: &mut NguoiDung, tt: &Obj) {
        for x in ds_lop(tt) {
            let ma = lay_str(&x, "ma").unwrap_or("").to_string();
            let cho = self.dang_cho.contains(&ma) || self.cho_cap_nhat.contains(&ma);
            let co = co_ten(&x, &nd.ten);
            if cho || co || !self.cua_toi.contains_key(&ma) {
                self.thieu_tu.remove(&ma);
            }
            if cho {
                continue;
            }
            if self.cua_toi.contains_key(&ma) && !co {
                let bay = bay_gio();
                if bay - *self.thieu_tu.entry(ma.clone()).or_insert(bay) < 0.5 {
                    continue;
                }
                self.thieu_tu.remove(&ma);
                self.cua_toi.remove(&ma); // máy chủ báo thành công nhưng danh sách không có tên
                self.bao(&format!("Đăng ký {ma} đã bị MẤT (lỗi máy chủ)!"), mau("#c00000"), Color32::WHITE);
                nd.nhat_ky.ghi(&format!("CẢNH BÁO: danh sách lớp {ma} không có tên bạn"));
                self.sau_cap_nhat();
            } else if !self.cua_toi.contains_key(&ma) && co {
                self.cua_toi.insert(ma, "–".into()); // (đăng ký từ thiết bị khác)
            }
        }
    }
}

impl UngDung for CongDangKy {
    const TIEU_DE: &'static str = "Cổng đăng ký học phần";

    fn moi(_ten: &str) -> Self {
        CongDangKy {
            cua_toi: BTreeMap::new(),
            thieu_tu: HashMap::new(),
            dang_cho: HashSet::new(),
            cho_cap_nhat: HashSet::new(),
            thong_bao: ("Chưa kết nối".into(), Color32::from_gray(0xe0), Color32::BLACK),
        }
    }

    fn ve(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung) {
        let tt = nd.tt.clone();
        let lop = tt.as_ref().map(ds_lop).unwrap_or_default();
        let rong = COT.iter().map(|c| c.1).sum::<f32>() + COT.len() as f32;
        egui::Frame::none().fill(mau(XANH_DH)).inner_margin(egui::Margin::symmetric(8.0, 6.0)).show(ui, |ui| {
            ui.set_width(rong);
            ui.horizontal(|ui| {
                ui.label(RichText::new("CỔNG ĐĂNG KÝ HỌC PHẦN").strong().size(17.0).color(Color32::WHITE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Học kỳ 1 · Năm học 2026–2027").strong().size(12.5).color(mau("#ffd54f")));
                });
            });
        });
        let tc: HashMap<String, i64> = lop.iter().map(|x| (lay_str(x, "ma").unwrap_or("").to_string(), lay_i64(x, "tc").unwrap_or(0))).collect();
        let so_tc: i64 = self.cua_toi.keys().map(|m| tc.get(m).copied().unwrap_or(0)).sum();
        egui::Frame::none().fill(mau("#e8eef8")).inner_margin(egui::Margin::symmetric(8.0, 4.0)).show(ui, |ui| {
            ui.set_width(rong);
            ui.horizontal(|ui| {
                ui.label(RichText::new(if tt.is_some() { format!("Sinh viên: {}", nd.ten) } else { "Sinh viên: –".into() }).strong().size(12.5));
                if tt.is_some() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("Đã đăng ký {} lớp · {so_tc} tín chỉ", self.cua_toi.len())).size(12.5));
                    });
                }
            });
        });
        let (text, nen, mc) = self.thong_bao.clone();
        egui::Frame::none().fill(nen).inner_margin(egui::Margin::symmetric(6.0, 5.0)).show(ui, |ui| {
            ui.set_width(rong);
            ui.vertical_centered(|ui| ui.label(RichText::new(text).strong().size(14.0).color(mc)));
        });
        let mut bam: Option<String> = None;
        nhom(ui, "Danh sách lớp học phần mở đăng ký", |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(1.0, 1.0);
            egui::Frame::none().fill(mau("#9aa9c4")).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        for (chu, w) in COT {
                            egui::Frame::none().fill(mau("#34558b")).inner_margin(egui::Margin::symmetric(3.0, 3.0)).show(ui, |ui| {
                                ui.set_width(w - 6.0);
                                ui.label(RichText::new(chu).strong().size(11.5).color(Color32::WHITE));
                            });
                        }
                    });
                    for (i, x) in lop.iter().enumerate() {
                        let ma = lay_str(x, "ma").unwrap_or("").to_string();
                        let cua_toi = self.cua_toi.contains_key(&ma);
                        let nen = if cua_toi { mau("#d8f5d8") } else if i % 2 == 1 { Color32::WHITE } else { mau("#f3f6fb") };
                        let (da, si) = (lay_i64(x, "da_dk").unwrap_or(0), lay_i64(x, "si_so").unwrap_or(1).max(1));
                        ui.horizontal(|ui| {
                            let o = [so_us(x.get("ma")), so_us(x.get("ten")), so_us(x.get("tc")), so_us(x.get("gv")), so_us(x.get("lich"))];
                            for (gt, (_, w)) in o.iter().zip(COT.iter()) {
                                egui::Frame::none().fill(nen).inner_margin(egui::Margin::symmetric(3.0, 4.0)).show(ui, |ui| {
                                    ui.set_width(w - 6.0);
                                    let mut r = RichText::new(gt).size(11.5);
                                    if cua_toi {
                                        r = r.strong();
                                    }
                                    ui.add(egui::Label::new(r).truncate());
                                });
                            }
                            egui::Frame::none().fill(nen).inner_margin(egui::Margin::symmetric(3.0, 4.0)).show(ui, |ui| {
                                ui.set_width(COT[5].1 - 6.0);
                                ui.horizontal(|ui| {
                                    let (r, pt) = ui.allocate_painter(egui::vec2(70.0, 12.0), egui::Sense::hover());
                                    pt.rect_filled(r.rect, 0.0, mau("#dddddd"));
                                    let ti_le = (da as f32 / si as f32).min(1.0);
                                    let m = if da >= si { DO_DAM } else if ti_le >= 0.8 { mau("#f0a020") } else { mau("#3cb44b") };
                                    pt.rect_filled(egui::Rect::from_min_size(r.rect.min, egui::vec2(70.0 * ti_le, 12.0)), 0.0, m);
                                    ui.label(RichText::new(format!("{da}/{si}")).strong().size(11.5).color(if da >= si { DO_DAM } else { Color32::BLACK }));
                                });
                            });
                            let chu = self.chu_nut(nd, &ma);
                            let mau_nut = match chu {
                                "Hủy" => mau("#f08080"),
                                "Đã đủ" => mau("#c8c8c8"),
                                "…" => XAM_NUT,
                                _ => mau("#7fb3ff"),
                            };
                            if nut(ui, chu, mau_nut, chu != "…" && nd.kn.is_some(), COT[6].1) {
                                bam = Some(ma.clone());
                            }
                        });
                    }
                });
            });
        });
        if let Some(ma) = bam {
            self.bam(nd, &ma);
        }
        nhom(ui, "Kết quả đăng ký của bạn", |ui| {
            let ten: HashMap<String, String> = lop.iter().map(|x| (lay_str(x, "ma").unwrap_or("").to_string(), so_us(x.get("ten")))).collect();
            let dong: Vec<(String, Color32)> = self
                .cua_toi
                .iter()
                .map(|(m, g)| (format!("{m}  {:<16} {} TC  lúc {g}", ten.get(m).map_or("?", String::as_str), tc.get(m).copied().unwrap_or(0)), Color32::BLACK))
                .collect();
            danh_sach(ui, &dong, 4, rong - 12.0, 11.5);
        });
    }

    fn ket_qua(&mut self, nd: &mut NguoiDung, loai: &str, truong: &Obj, tin: &Obj, rtt: f64) {
        let ma = lay_str(truong, "ma_lop").unwrap_or("").to_string();
        self.dang_cho.remove(&ma);
        self.cho_cap_nhat.insert(ma.clone()); // tt đang giữ có thể chụp trước lần ghi này
        if dung(tin, "ok") {
            if loai == "dang_ky" {
                self.cua_toi.insert(ma.clone(), gio_hien());
                self.bao(&format!("Đăng ký thành công lớp {ma}"), mau("#70d070"), Color32::BLACK);
            } else {
                self.cua_toi.remove(&ma);
                self.bao(&format!("Đã hủy đăng ký lớp {ma}"), mau("#d0d0ff"), Color32::BLACK);
            }
            let si_so = nd.tt.as_ref().map_or("–".into(), |t| so_us(t.get("si_so")));
            nd.nhat_ky.ghi(&format!("  thành công ({}/{si_so}, {rtt:.1} ms)", so_us(tin.get("da_dk"))));
        } else {
            let m = lay_str(tin, "ma_loi").unwrap_or("");
            let ly_do = tin.get("ly_do").map(py_str).unwrap_or_default();
            let text = match m {
                "lop_day" => format!("Lớp {ma} đã đủ sĩ số"),
                "da_dang_ky" => format!("Bạn đã đăng ký lớp {ma} rồi"),
                "xung_dot" => "Hệ thống bận, vui lòng thử lại".into(),
                _ => ly_do.clone(),
            };
            if m == "da_dang_ky" {
                self.cua_toi.entry(ma.clone()).or_insert_with(gio_hien);
            }
            if m == "lop_day" {
                self.bao(&text, mau("#ff6060"), Color32::WHITE);
            } else {
                self.bao(&text, mau("#ffb347"), Color32::BLACK);
            }
            nd.nhat_ky.ghi(&format!("  từ chối: {}", tin.get("ly_do").map(py_str).unwrap_or_else(|| "None".into())));
        }
        if nd.tt.is_some() {
            self.sau_cap_nhat(); // như _cap_nhat(tt, kiem_mat=False) của bản Python
        }
    }

    /// Có cap_nhat MỚI (tới sau mọi kq đã nhận).
    fn cap_nhat(&mut self, nd: &mut NguoiDung, tt: &Obj) {
        self.cho_cap_nhat.clear(); // cap_nhat mới tới SAU các kq đã nhận
        self.kiem_mat(nd, tt);
        self.sau_cap_nhat();
    }

    fn su_kien(&mut self, nd: &mut NguoiDung, text: &str) {
        nd.nhat_ky.ghi(&format!("* {text}"));
        if text.starts_with("Đặt lại") {
            // đợt đăng ký mới: danh sách cũ không còn
            self.cua_toi.clear();
            self.thieu_tu.clear();
            self.bao("Đợt đăng ký mới – chọn lớp và bấm Đăng ký", Color32::from_gray(0xe0), Color32::BLACK);
        }
    }

    fn moi_nhip(&mut self, nd: &mut NguoiDung) {
        if nd.kn.is_some() {
            if let Some(tt) = nd.tt.clone() {
                self.kiem_mat(nd, &tt);
            }
        }
    }

    fn mat_ket_noi(&mut self, _nd: &mut NguoiDung) {
        self.dang_cho.clear();
    }

    fn dong_dinh_ky(&self, nd: &NguoiDung) -> String {
        let tt = nd.tt.clone().unwrap_or_default();
        let lop: Vec<String> = ds_lop(&tt).iter().map(|x| format!("{} {}/{}", so_us(x.get("ma")), so_us(x.get("da_dk")), so_us(x.get("si_so")))).collect();
        let cua_toi: Vec<&str> = self.cua_toi.keys().map(String::as_str).collect();
        format!("{}; bạn: {}", lop.join(", "), if cua_toi.is_empty() { "chưa có lớp".to_string() } else { cua_toi.join(", ") })
    }
}

/// dang_ky_gui [tên] [host] [cổng]
pub fn run(args: &[String]) {
    let ten = args.first().cloned().unwrap_or_else(|| "An".into());
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    let cs = CuaSoNguoiDung::<CongDangKy>::moi(&ten, &host, cong);
    chay_cua_so(&cs.tieu_de(), [760.0, 600.0], cs);
}
