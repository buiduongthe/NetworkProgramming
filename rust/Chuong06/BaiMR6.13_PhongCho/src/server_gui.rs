//! Bài mở rộng MR6.13 – Bảng điều khiển máy chủ bán vé có phòng chờ ảo (egui).
//!
//! Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế
//! độ, nhật ký) ở gui_chung::CuaSoServer; tệp này chỉ vẽ khung "Tài nguyên": VÉ CÒN LẠI (thanh), độ dài
//! HÀNG ĐỢI, số đang phục vụ, tốc độ cấp lượt (đổi được ngay), mức token trong thùng, số yêu cầu mua đang
//! xử lý so với giới hạn, số lần từ chối vì quá tải, ô chọn "Bật phòng chờ", tham số đặt lại và nút "So sánh
//! bật/tắt phòng chờ" (bảng hai dòng: mua được, hết vé, quá tải, bỏ cuộc, thời gian mua p99).
//!
//! Nút "So sánh bật/tắt phòng chờ" chạy nguoi_ao::so_sanh_phong_cho trên luồng nền (TCP tới chính máy chủ
//! này), lấy n, k ở khung "Mô phỏng tải"; ve_server() đồng bộ hai khung đó mỗi lần vẽ.
//!
//! Chạy:  server_gui [host] [cổng]        (mặc định 127.0.0.1 44772)

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::{so_con, PhongCho, CONG_GUI, NV};
use crate::nguoi_ao::{bang_phong_cho, so_sanh_phong_cho, HangPhongCho, SO_LAN_MAC_DINH};
use eframe::egui::{self, Color32, RichText, Stroke};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct TaiNguyenPhongCho {
    lop: Option<Arc<MayChu<NV>>>,
    nk: Option<NhatKy>,
    pub o_toc_do: String,
    o_ts: Vec<(&'static str, &'static str, String, f32)>, // (mã, nhãn, giá trị ô, rộng)
    pub phong_cho: bool,
    nk_n: (String, String), // n, k của khung mô phỏng (ve_server đồng bộ)
    pub dang_pc: Arc<AtomicBool>,
    ket_qua_pc: Arc<Mutex<Option<Vec<HangPhongCho>>>>,
    pub hang_pc: Vec<HangPhongCho>,
    bat_dau_pc: bool,
}

impl TaiNguyenPhongCho {
    /// Điền ô tham số / ô chọn (kịch bản demo) – như người dùng gõ.
    pub fn dat_tham_so(&mut self, ma: &str, gia_tri: &str) {
        match ma {
            "phong_cho" => self.phong_cho = gia_tri == "1",
            "toc_do" => self.o_toc_do = gia_tri.into(),
            _ => {
                if let Some(o) = self.o_ts.iter_mut().find(|o| o.0 == ma) {
                    o.2 = gia_tri.into();
                }
            }
        }
    }

    /// Bấm "Áp dụng ngay" (tốc độ cấp lượt).
    pub fn doi_toc_do(&mut self) {
        let (Some(lop), Some(nk)) = (&self.lop, &self.nk) else { return };
        let kq = self.o_toc_do.trim().parse::<f64>().map_err(|_| format!("could not convert string to float: '{}'", self.o_toc_do)).and_then(|v| PhongCho::dat_toc_do(lop, v));
        if let Err(e) = kq {
            nk.ghi(&format!("Tốc độ không hợp lệ: {e}"));
        }
    }

    /// Bấm "So sánh bật/tắt phòng chờ".
    pub fn so_sanh_pc(&mut self) {
        let (Some(lop), Some(nk)) = (self.lop.clone(), self.nk.clone()) else { return };
        if self.dang_pc.load(Ordering::SeqCst) || !lop.dang_chay() {
            nk.ghi("Cần START SERVER và chờ lượt mô phỏng trước xong");
            return;
        }
        let (Ok(n), Ok(k)) = (self.nk_n.0.trim().parse::<usize>(), self.nk_n.1.trim().parse::<usize>()) else {
            nk.ghi("n, k, lần ≥ 1");
            return;
        };
        let (n, k) = (n.max(1), k.max(1).min(n.max(1)));
        self.dang_pc.store(true, Ordering::SeqCst);
        self.bat_dau_pc = true;
        let dc = lop.dia_chi();
        let (host, cong) = dc.rsplit_once(':').map(|(h, c)| (h.to_string(), c.parse::<u16>().unwrap_or(0))).unwrap_or_default();
        let (kq, d) = (Arc::clone(&self.ket_qua_pc), Arc::clone(&self.dang_pc));
        std::thread::spawn(move || {
            let bao = |text: &str| nk.ghi(&format!("[mô phỏng] {text}"));
            let hang = so_sanh_phong_cho(&host, cong, n, k, &bao, "khoa").unwrap_or_else(|e| {
                bao(&format!("lỗi: {e}"));
                Vec::new()
            });
            for dong in bang_phong_cho(&hang) {
                nk.ghi(dong.trim());
            }
            *kq.lock().unwrap() = Some(hang);
            d.store(false, Ordering::SeqCst);
            nk.ghi_lang();
        });
    }
}

impl TaiNguyen<NV> for TaiNguyenPhongCho {
    const TIEU_DE: &'static str = "Máy chủ bán vé MR6.13";
    const TEN_TAI_NGUYEN: &'static str = "Tài nguyên: vé và phòng chờ ảo";

    fn moi(ts: &Obj) -> Self {
        let o = |ma: &'static str, nhan: &'static str, rong: f32| (ma, nhan, ts.get(ma).map(|v| if v.is_f64() { fmt_g(v.as_f64().unwrap_or(0.0)) } else { py_str(v) }).unwrap_or_default(), rong);
        TaiNguyenPhongCho {
            lop: None,
            nk: None,
            o_toc_do: fmt_g(lay_f64(ts, "toc_do").unwrap_or(50.0)),
            o_ts: vec![o("tong_ve", "Tổng vé", 50.0), o("dung_luong", "Thùng token", 40.0), o("luot_s", "Hạn lượt (s)", 40.0), o("gioi_han_xu_ly", "Giới hạn xử lý", 40.0)],
            phong_cho: lay_i64(ts, "phong_cho").unwrap_or(1) != 0,
            nk_n: ("1000".into(), "200".into()),
            dang_pc: Arc::new(AtomicBool::new(false)),
            ket_qua_pc: Arc::new(Mutex::new(None)),
            hang_pc: Vec::new(),
            bat_dau_pc: false,
        }
    }

    fn ve(&mut self, ui: &mut egui::Ui, tt: Option<&Obj>, _tk: Option<&Obj>) {
        if let Some(h) = self.ket_qua_pc.lock().unwrap().take() {
            if !h.is_empty() {
                self.hang_pc = h;
            }
        }
        egui::Grid::new("ve_hang").num_columns(2).spacing([16.0, 0.0]).show(ui, |ui| {
            ui.label(RichText::new("VÉ CÒN LẠI").strong().size(10.5).color(mau("#404040")));
            ui.label(RichText::new("HÀNG ĐỢI").strong().size(10.5).color(mau("#404040")));
            ui.end_row();
            let (ve, hang, mh) = match tt {
                Some(t) => (
                    format!("{} / {}", so_us(t.get("con_lai")), so_con(t, "su_kien", "tong_ve")),
                    if dung(t, "phong_cho") { format!("{} người", so_us(t.get("dang_cho"))) } else { "TẮT".into() },
                    if dung(t, "phong_cho") { mau("#1f4fa0") } else { mau("#808080") },
                ),
                None => ("–".into(), "–".into(), mau("#1f4fa0")),
            };
            ui.label(RichText::new(ve).strong().size(20.0).color(mau("#8b0000")));
            ui.label(RichText::new(hang).strong().size(20.0).color(mh));
            ui.end_row();
        });
        let Some(t) = tt else {
            self.ve_tham_so(ui);
            return;
        };
        let tong = so_con(t, "su_kien", "tong_ve").max(1);
        crate::phong_cho_gui::thanh(ui, 360.0, lay_i64(t, "con_lai").unwrap_or(0).max(0) as f32 / tong as f32);
        let ban_so = lay_i64(t, "ban_so").unwrap_or(0);
        let vuot = ban_so > tong;
        let chu = format!(
            "Sổ đã bán: {ban_so} vé, {} đơn{}{}",
            so_us(t.get("so_don")),
            if vuot { format!(" – VƯỢT {} vé!", ban_so - tong) } else { String::new() },
            if dung(t, "het_ve") { "  [HẾT VÉ]" } else { "" }
        );
        ui.label(RichText::new(chu).size(11.5).color(if vuot { DO_DAM } else { Color32::BLACK }));
        let s = |k: &str| so_us(t.get(k));
        ui.label(RichText::new(format!("Đang phục vụ tới #{} / đã phát #{} · lượt đã cấp {}, hết hạn {}", s("dang_phuc_vu"), s("so_cap"), s("da_cap_luot"), s("het_han"))).size(11.5));
        let qt = lay_i64(t, "qua_tai").unwrap_or(0);
        ui.label(RichText::new(format!("Mua đang xử lý: {} / {} (max {}) · từ chối quá tải: {qt}", s("dang_xu_ly"), s("gioi_han"), s("xu_ly_max"))).strong().size(11.5).color(if qt > 0 { DO_DAM } else { XANH_DAM }));
        ui.horizontal(|ui| {
            ui.label("Cấp lượt:");
            o_nhap(ui, &mut self.o_toc_do, 44.0, true);
            ui.label("người/s");
            if nut(ui, "Áp dụng ngay", XANH_DUONG, true, 90.0) {
                self.doi_toc_do();
            }
            ui.label(RichText::new(format!("thùng {}/{} token", s("token"), s("dung_luong"))).size(10.5));
        });
        self.ve_tham_so(ui);
        ui.horizontal(|ui| {
            if nut(ui, "So sánh bật/tắt phòng chờ", CAM, !self.dang_pc.load(Ordering::SeqCst), 190.0) {
                self.so_sanh_pc();
            }
        });
        ve_bang_pc(ui, &self.hang_pc);
    }

    fn tham_so_dat_lai(&self) -> Result<Obj, String> {
        let v = |ma: &str| self.o_ts.iter().find(|o| o.0 == ma).map(|o| o.2.trim().to_string()).unwrap_or_default();
        let loi = || "tham số phải là số".to_string();
        Ok(obj(json!({
            "tong_ve": v("tong_ve").parse::<i64>().map_err(|_| loi())?,
            "dung_luong": v("dung_luong").parse::<i64>().map_err(|_| loi())?,
            "luot_s": v("luot_s").parse::<f64>().map_err(|_| loi())?,
            "gioi_han_xu_ly": v("gioi_han_xu_ly").parse::<i64>().map_err(|_| loi())?,
            "toc_do": self.o_toc_do.trim().parse::<f64>().map_err(|_| loi())?,
            "phong_cho": self.phong_cho as i64,
        })))
    }
}

impl TaiNguyenPhongCho {
    fn ve_tham_so(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("ts_pc").num_columns(4).spacing([4.0, 3.0]).show(ui, |ui| {
            for (i, (_, nhan, gt, rong)) in self.o_ts.iter_mut().enumerate() {
                ui.label(format!("{nhan}:"));
                o_nhap(ui, gt, *rong, true);
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.checkbox(&mut self.phong_cho, "Bật phòng chờ ảo (có hiệu lực khi Đặt lại)");
    }
}

/// Bảng hai dòng bật/tắt phòng chờ: dòng có quá tải nền đỏ, không quá tải nền xanh.
fn ve_bang_pc(ui: &mut egui::Ui, hang: &[HangPhongCho]) {
    let cot: [(&str, f32); 6] = [("Phòng chờ", 74.0), ("Mua được", 66.0), ("Hết vé", 52.0), ("Quá tải", 58.0), ("Bỏ cuộc", 60.0), ("p99 ms", 58.0)];
    let rong: f32 = cot.iter().map(|c| c.1).sum();
    let cao = 18.0;
    let (r, _) = ui.allocate_exact_size(egui::vec2(rong, cao * 3.0 + 2.0), egui::Sense::hover());
    let p = ui.painter_at(r);
    p.rect_filled(r, 0.0, Color32::WHITE);
    let phong = egui::FontId::proportional(11.5);
    let ve_dong = |y: f32, chu: [String; 6], nen: Color32, mc: Color32| {
        p.rect_filled(egui::Rect::from_min_size(egui::pos2(r.min.x, y), egui::vec2(rong, cao)), 0.0, nen);
        let mut x = r.min.x;
        for (i, (c, w)) in chu.iter().zip(cot.iter().map(|c| c.1)).enumerate() {
            if i == 0 {
                p.text(egui::pos2(x + 4.0, y + cao / 2.0), egui::Align2::LEFT_CENTER, c, phong.clone(), mc);
            } else {
                p.text(egui::pos2(x + w - 5.0, y + cao / 2.0), egui::Align2::RIGHT_CENTER, c, phong.clone(), mc);
            }
            x += w;
            p.line_segment([egui::pos2(x, y), egui::pos2(x, y + cao)], Stroke::new(1.0_f32, Color32::from_gray(0xd8)));
        }
    };
    ve_dong(r.min.y, cot.map(|c| c.0.to_string()), Color32::from_gray(0xe8), Color32::BLACK);
    for (i, h) in hang.iter().enumerate() {
        let (nen, mc) = if h.qua_tai > 0 { (mau("#ffc8c8"), DO_DAM) } else { (mau("#d8f5d8"), Color32::BLACK) };
        ve_dong(r.min.y + cao * (i + 1) as f32, [h.phong_cho.to_string(), h.mua_duoc.to_string(), h.het_ve.to_string(), h.qua_tai.to_string(), h.bo_cuoc.to_string(), chu_p99(h.p99_ms)], nen, mc);
    }
    p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::from_gray(0x90)));
}

pub type CuaSoServerPhongCho = CuaSoServer<NV, TaiNguyenPhongCho>;

pub fn tao_cua_so(host: &str, cong: u16) -> CuaSoServerPhongCho {
    let mut cs: CuaSoServerPhongCho = CuaSoServer::moi(host, cong, "khoa", 2.0, Obj::new(), 1000, 200, SO_LAN_MAC_DINH);
    cs.tn.lop = Some(Arc::clone(&cs.lop));
    cs.tn.nk = Some(cs.nhat_ky.clone());
    cs
}

/// Vẽ bảng điều khiển: đồng bộ n, k của khung mô phỏng sang khung tài nguyên; trong lúc so sánh bật/tắt
/// phòng chờ thì khóa các nút mô phỏng của khung chung (dang_mo_phong), xong thì mở lại.
pub fn ve_server(cs: &mut CuaSoServerPhongCho, ui: &mut egui::Ui) {
    cs.tn.nk_n = (cs.o_n.clone(), cs.o_k.clone());
    let dang = cs.tn.dang_pc.load(Ordering::SeqCst);
    if dang {
        cs.dang_mo_phong = true;
    } else if cs.tn.bat_dau_pc {
        cs.tn.bat_dau_pc = false;
        cs.xong_mo_phong(Vec::new()); // mở lại nút, "xong"
    }
    cs.ui(ui);
}

pub fn tieu_de() -> String {
    format!("{} – bảng điều khiển", <TaiNguyenPhongCho as TaiNguyen<NV>>::TIEU_DE)
}

struct CuaSoChay(CuaSoServerPhongCho);

impl CuaSo for CuaSoChay {
    fn ve(&mut self, ui: &mut egui::Ui) {
        ve_server(&mut self.0, ui);
    }
    fn dong(&mut self) {
        self.0.dong();
    }
}

/// server_gui [host] [cổng]
pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    chay_cua_so(&tieu_de(), [1060.0, 800.0], CuaSoChay(tao_cua_so(&host, cong)));
}
