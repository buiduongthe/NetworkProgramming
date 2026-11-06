
//!
//! Tệp này GIỐNG NHAU ở cả 8 bài (chép nguyên). Gồm:
//!   - Tiện ích (như MR6.5): phông chữ có dấu, kiểu sáng nền xanh, nut, nhom, NhatKy, ThongKeDinhKy,
//!     thanh_trang_thai, o_nhap, PNG viết tay, chụp/cắt/ghép ảnh cửa sổ.
//!   - CuaSoServer<N, T>: bảng điều khiển máy chủ đủ mọi phần chung – START/STOP, 4 nút chọn chế độ, độ
//!     trễ xử lý, chỉ số trực tiếp (yc/s, thành công, từ chối, xung đột, hàng đợi, độ trễ, SAI PHẠM tô
//!     đỏ), khung mô phỏng tải (Bắn đồng loạt, So sánh 4 chế độ + bảng), nhật ký. Bài cụ thể (server_gui.rs)
//!     chỉ vẽ khung "Tài nguyên": cài đặt trait TaiNguyen.
//!   - CuaSoNguoiDung<U>: khung cửa sổ người dùng – kết nối/rời, nhật ký, thanh trạng thái, gửi yêu cầu;
//!     bài cụ thể (ví dụ dau_gia_gui.rs) vẽ phần nghiệp vụ: cài đặt trait UngDung.
//!   - KichBan, MayChup, ghep_anh: dùng cho demo_gui.rs (bấm nút tự động, chụp cửa sổ dưới Xvfb).
//!
//! Quy tắc (egui "immediate mode"): mỗi lần vẽ lại, hàm `ui` dựng lại cửa sổ từ trạng thái hiện tại;
//! CHỈ luồng giao diện chạm vào trạng thái của cửa sổ. Luồng mạng/mô phỏng đưa việc vào hàng sự kiện
//! (Mutex<VecDeque>) hoặc ghi NhatKy rồi gọi request_repaint(); số liệu máy chủ đọc định kỳ (250 ms).

#![allow(dead_code)] // khung chung của 8 bài: mỗi bài không dùng hết

use crate::chung::*;
use crate::khach::KetNoiKhach;
use crate::mo_phong;
use eframe::egui::{self, Color32, RichText};
use serde_json::Value;
use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const NEN: Color32 = Color32::from_rgb(0xb0, 0xc4, 0xde);
pub const XANH_LA: Color32 = Color32::from_rgb(0x90, 0xee, 0x90);
pub const DO: Color32 = Color32::from_rgb(0xf0, 0x80, 0x80);
pub const XANH_DUONG: Color32 = Color32::from_rgb(0xad, 0xd8, 0xe6);
pub const CAM: Color32 = Color32::from_rgb(0xff, 0xb3, 0x47);
pub const VANG: Color32 = Color32::from_rgb(0xff, 0xd7, 0x00);
pub const DO_DAM: Color32 = Color32::from_rgb(0xc0, 0x00, 0x00);
pub const XANH_DAM: Color32 = Color32::from_rgb(0x00, 0x64, 0x00);
pub const XAM_NUT: Color32 = Color32::from_rgb(0xf0, 0xf0, 0xf0);

/// Màu từ chuỗi "#rrggbb".
pub fn mau(hex: &str) -> Color32 {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Phông mặc định của egui thiếu nhiều chữ tiếng Việt có dấu (ế, ộ, ữ…), nên nạp thêm một phông hệ
/// thống có đủ dấu (cả cho chữ đều – monospace). Không tìm thấy thì giữ phông mặc định.
pub fn cai_phong_chu(ctx: &egui::Context) {
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
    let mut fonts = egui::FontDefinitions::default();
    if let Some(du_lieu) = ung_vien.iter().find_map(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("he_thong".into(), egui::FontData::from_owned(du_lieu));
        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "he_thong".into());
        fonts.families.entry(egui::FontFamily::Monospace).or_default().push("he_thong".into());
    }
    for p in ["C:\\Windows\\Fonts\\consola.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", "/usr/share/fonts/TTF/DejaVuSansMono.ttf", "/System/Library/Fonts/Menlo.ttc"] {
        if let Ok(d) = std::fs::read(p) {
            // chữ đều có đủ dấu tiếng Việt (bảng so sánh, danh sách)
            fonts.font_data.insert("deu".into(), egui::FontData::from_owned(d));
            fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "deu".into());
            break;
        }
    }
    ctx.set_fonts(fonts);
}

/// Giao diện sáng, nền xanh nhạt như bản Tkinter.
pub fn dat_kieu(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 3.0);
        s.visuals.panel_fill = NEN;
        s.visuals.window_fill = NEN;
        s.visuals.override_text_color = Some(Color32::BLACK);
    });
}

/// Nút có màu nền; `bat` = false thì mờ, không bấm được.
pub fn nut(ui: &mut egui::Ui, chu: &str, mau: Color32, bat: bool, rong: f32) -> bool {
    ui.add_enabled(bat, egui::Button::new(RichText::new(chu).color(Color32::BLACK)).fill(mau).min_size(egui::vec2(rong, 24.0))).clicked()
}

/// Nút chữ đậm màu trắng trên nền màu (nút chính của ứng dụng: ĐẶT XE, GIỮ CHỖ…).
pub fn nut_chinh(ui: &mut egui::Ui, chu: &str, nen: Color32, bat: bool, rong: f32, cao: f32) -> bool {
    let chu = RichText::new(chu).strong().size(15.0).color(if bat { Color32::WHITE } else { Color32::from_white_alpha(200) });
    ui.add_enabled(bat, egui::Button::new(chu).fill(nen).min_size(egui::vec2(rong, cao))).clicked()
}

/// Khung nhóm có tiêu đề (như LabelFrame của Tk).
pub fn nhom<R>(ui: &mut egui::Ui, tieu_de: &str, noi_dung: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .fill(NEN)
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(tieu_de).strong().size(12.5));
                noi_dung(ui)
            })
            .inner
        })
        .inner
}

/// Nhãn có nền màu (như tk.Label bg=…), rộng cố định.
pub fn nhan_nen(ui: &mut egui::Ui, chu: &str, nen: Color32, mau_chu: Color32, co: f32, rong: f32) {
    egui::Frame::none().fill(nen).inner_margin(egui::Margin::symmetric(6.0, 4.0)).show(ui, |ui| {
        ui.set_width(rong);
        ui.add(egui::Label::new(RichText::new(chu).strong().size(co).color(mau_chu)).wrap());
    });
}

/// Ô nhập một dòng, mờ khi `bat` = false.
pub fn o_nhap(ui: &mut egui::Ui, s: &mut String, rong: f32, bat: bool) {
    ui.add_enabled(bat, egui::TextEdit::singleline(s).desired_width(rong));
}

/// Thanh trạng thái dưới cùng (nền trắng, viền lõm).
pub fn thanh_trang_thai(ui: &mut egui::Ui, chu: &str) {
    egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, Color32::from_gray(0x90))).inner_margin(egui::Margin::symmetric(4.0, 2.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add(egui::Label::new(RichText::new(chu).color(Color32::BLACK)).truncate());
    });
}

/// Ô danh sách chữ đều (như tk.Listbox), mỗi dòng một màu.
pub fn danh_sach(ui: &mut egui::Ui, dong: &[(String, Color32)], so_dong: usize, rong: f32, co: f32) {
    egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, Color32::from_gray(0x90))).inner_margin(egui::Margin::same(2.0)).show(ui, |ui| {
        ui.set_width(rong);
        ui.set_height(so_dong as f32 * (co + 3.0));
        ui.spacing_mut().item_spacing.y = 1.0;
        for (d, m) in dong.iter().take(so_dong) {
            ui.add(egui::Label::new(RichText::new(d).monospace().size(co).color(*m)).truncate());
        }
    });
}

pub fn ms(us: Option<&Value>) -> String {
    match us.and_then(Value::as_f64) {
        None => "–".into(),
        Some(v) => format!("{:.1}", v / 1000.0),
    }
}

/// Luồng nền báo có dữ liệu mới: vẽ lại sau tối đa 100 ms (gộp nhiều báo – cap_nhat, nhật ký – vào một
/// lần vẽ; vẽ phần mềm dưới Xvfb tốn CPU, giành CPU của máy chủ và bộ mô phỏng chạy cùng tiến trình).
pub const NHIP_VE: std::time::Duration = std::time::Duration::from_millis(100);

/// Bật khi chạy riêng từng cửa sổ (không qua demo_gui): mỗi dòng nhật ký cũng in ra dòng lệnh.
pub static IN_RA: AtomicBool = AtomicBool::new(false);

/// Ô nhật ký cuộn (chỉ đọc), mỗi dòng có giờ. Ghi được từ mọi luồng (Mutex).
#[derive(Clone)]
pub struct NhatKy {
    ten: String,
    dong: Arc<Mutex<VecDeque<String>>>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl NhatKy {
    pub fn new(ten: &str) -> NhatKy {
        NhatKy { ten: ten.to_string(), dong: Arc::new(Mutex::new(VecDeque::new())), ctx: Arc::new(Mutex::new(None)) }
    }

    pub fn ghi(&self, text: &str) {
        let gio = gio_hien();
        if IN_RA.load(Ordering::SeqCst) {
            in_tho(&format!("{gio} [{}] {text}", self.ten));
        }
        let mut v = self.dong.lock().unwrap();
        v.push_back(format!("{gio} {text}"));
        while v.len() > 500 {
            v.pop_front();
        }
        drop(v);
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint_after(NHIP_VE); // gộp: vẽ lại tối đa ~10 lần/giây
        }
    }

    /// Hàm báo sự kiện cho lõi (gọi từ luồng nền).
    pub fn bao(&self) -> Bao {
        let nk = self.clone();
        Arc::new(move |t: &str| nk.ghi(t))
    }

    pub fn ve(&self, ui: &mut egui::Ui, so_dong: usize, rong: f32) {
        if self.ctx.lock().unwrap().is_none() {
            *self.ctx.lock().unwrap() = Some(ui.ctx().clone());
        }
        egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, Color32::from_gray(0x90))).inner_margin(egui::Margin::same(3.0)).show(ui, |ui| {
            ui.set_width(rong);
            egui::ScrollArea::vertical().id_salt(("nk", self.ten.clone())).stick_to_bottom(true).auto_shrink([false, false]).max_height(so_dong as f32 * 15.0).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                for d in self.dong.lock().unwrap().iter() {
                    ui.add(egui::Label::new(RichText::new(d).monospace().size(11.0).color(Color32::BLACK)).wrap());
                }
            });
        });
    }
}

/// Khi chạy riêng (IN_RA): cứ `chu_ky` giây in một dòng số liệu ra dòng lệnh.
pub struct ThongKeDinhKy {
    ten: String,
    chu_ky: f64,
    han: f64,
}

impl ThongKeDinhKy {
    pub fn new(ten: &str) -> ThongKeDinhKy {
        ThongKeDinhKy { ten: ten.to_string(), chu_ky: 5.0, han: bay_gio() + 5.0 }
    }

    pub fn in_neu_den_han(&mut self, tao_dong: impl FnOnce() -> String) {
        let bay = bay_gio();
        if !IN_RA.load(Ordering::SeqCst) || bay < self.han {
            return;
        }
        self.han = bay + self.chu_ky;
        in_tho(&format!("{} [{}] {}", gio_hien(), self.ten, tao_dong()));
    }
}

// ===================================================================== bảng điều khiển máy chủ
/// Phần "Tài nguyên" của bảng điều khiển máy chủ (mỗi bài một kiểu).
pub trait TaiNguyen<N: NghiepVu>: Sized {
    const TIEU_DE: &'static str;
    const TEN_TAI_NGUYEN: &'static str;
    fn moi(ts: &Obj) -> Self;
    /// Vẽ khung tài nguyên; tt = cap_nhat mới nhất của máy chủ (None khi chưa chạy), tk = thong_ke.
    fn ve(&mut self, ui: &mut egui::Ui, tt: Option<&Obj>, tk: Option<&Obj>);
    fn tham_so_dat_lai(&self) -> Result<Obj, String>;
}

type KetQuaMoPhong = Arc<Mutex<Option<Vec<HangSoSanh>>>>;

pub struct CuaSoServer<N: NghiepVu, T: TaiNguyen<N>> {
    pub lop: Arc<MayChu<N>>,
    pub tn: T,
    pub o_ip: String,
    pub o_cong: String,
    pub che_do: &'static str,
    pub o_tre: String,
    pub o_n: String,
    pub o_k: String,
    pub o_lan: String,
    pub dang_mo_phong: bool,
    pub hang_so_sanh: Vec<HangSoSanh>,
    bang: Vec<HangSoSanh>,
    pub ket_luan: String,
    pub nhat_ky: NhatKy,
    nhan_mo_phong: (String, Color32),
    dinh_ky: ThongKeDinhKy,
    pub tk: Option<Obj>,
    pub tt: Option<Obj>,
    lan_doc: f64,
    ket_qua: KetQuaMoPhong,
    dang_dung: Arc<AtomicBool>,
    da_chay: bool,
}

impl<N: NghiepVu, T: TaiNguyen<N>> CuaSoServer<N, T> {
    #[allow(clippy::too_many_arguments)]
    pub fn moi(host: &str, cong: u16, che_do: &str, tre_ms: f64, tham_so: Obj, n: usize, k: usize, lan: u32) -> Self {
        let nhat_ky = NhatKy::new("server");
        let lop = MayChu::<N>::new(che_do, tre_ms, nhat_ky.bao(), tham_so).expect("tham số máy chủ không hợp lệ");
        let tn = T::moi(&lop.tham_so());
        CuaSoServer {
            che_do: CHE_DO.iter().find(|x| **x == che_do).copied().unwrap_or("khoa"),
            lop,
            tn,
            o_ip: host.into(),
            o_cong: cong.to_string(),
            o_tre: fmt_g(tre_ms),
            o_n: n.to_string(),
            o_k: k.to_string(),
            o_lan: lan.to_string(),
            dang_mo_phong: false,
            hang_so_sanh: Vec::new(),
            bang: Vec::new(),
            ket_luan: String::new(),
            nhat_ky,
            nhan_mo_phong: (String::new(), Color32::BLACK),
            dinh_ky: ThongKeDinhKy::new("server"),
            tk: None,
            tt: None,
            lan_doc: 0.0,
            ket_qua: Arc::new(Mutex::new(None)),
            dang_dung: Arc::new(AtomicBool::new(false)),
            da_chay: false,
        }
    }

    // ------------------------------------------------------------ nút
    pub fn bam_start(&mut self) {
        let Ok(cong) = self.o_cong.trim().parse::<u16>() else {
            self.nhat_ky.ghi("Không khởi động được: cổng không hợp lệ");
            return;
        };
        if let Err(e) = self.lop.bat_dau(self.o_ip.trim(), cong) {
            self.nhat_ky.ghi(&format!("Không khởi động được: {e}"));
            return;
        }
        self.da_chay = true;
        self.lan_doc = 0.0;
    }

    pub fn bam_stop(&mut self) {
        self.dang_dung.store(true, Ordering::SeqCst);
        let (lop, d) = (Arc::clone(&self.lop), Arc::clone(&self.dang_dung));
        std::thread::spawn(move || {
            // chờ luồng nền dừng: không chặn giao diện
            lop.dung();
            d.store(false, Ordering::SeqCst);
        });
    }

    /// START bấm được (máy chủ đã dừng hẳn).
    pub fn start_bat(&self) -> bool {
        !self.lop.dang_chay() && !self.dang_dung.load(Ordering::SeqCst)
    }

    /// Chọn chế độ như người dùng bấm nút (kịch bản demo).
    pub fn chon_che_do(&mut self, cd: &'static str) {
        self.che_do = cd;
        let lop = Arc::clone(&self.lop);
        std::thread::spawn(move || {
            // có thể chờ hàng đợi cạn
            if let Err(e) = lop.dat_che_do(cd) {
                (lop.bao)(&format!("lỗi: {e}"));
            }
        });
    }

    pub fn bam_tre(&mut self) {
        let kq = self.o_tre.trim().parse::<f64>().map_err(|_| format!("could not convert string to float: '{}'", self.o_tre)).and_then(|ms| self.lop.dat_tre(ms));
        if let Err(e) = kq {
            self.nhat_ky.ghi(&format!("Độ trễ không hợp lệ: {e}"));
        }
    }

    pub fn bam_dat_lai(&mut self) {
        if let Err(e) = self.tn.tham_so_dat_lai().and_then(|ts| self.lop.dat_lai(&ts)) {
            self.nhat_ky.ghi(&format!("Không đặt lại được: {e}"));
        }
        self.lan_doc = 0.0;
    }

    fn doc_mo_phong(&self) -> Result<(usize, usize, u32), String> {
        let (n, k, lan) = (self.o_n.trim().parse::<usize>(), self.o_k.trim().parse::<usize>(), self.o_lan.trim().parse::<u32>());
        match (n, k, lan) {
            (Ok(n), Ok(k), Ok(lan)) if n >= 1 && k >= 1 && lan >= 1 => Ok((n, k.min(n), lan)),
            _ => Err("n, k, lần ≥ 1".into()),
        }
    }

    /// Chạy mô phỏng trên luồng nền (kết nối TCP tới chính máy chủ này như client bên ngoài).
    fn chay_nen(&mut self, so_sanh: bool) {
        if self.dang_mo_phong || !self.lop.dang_chay() {
            self.nhat_ky.ghi("Cần START SERVER và chờ lượt mô phỏng trước xong");
            return;
        }
        let (n, k, lan) = match self.doc_mo_phong() {
            Ok(x) => x,
            Err(e) => {
                self.nhat_ky.ghi(&e);
                return;
            }
        };
        self.dang_mo_phong = true;
        self.nhan_mo_phong = ("đang chạy…".into(), DO_DAM);
        let dc = self.lop.dia_chi();
        let (host, cong) = dc.rsplit_once(':').map(|(h, c)| (h.to_string(), c.parse::<u16>().unwrap_or(0))).unwrap_or_default();
        let (nk, kq) = (self.nhat_ky.clone(), Arc::clone(&self.ket_qua));
        std::thread::spawn(move || {
            let bao = |text: &str| nk.ghi(&format!("[mô phỏng] {text}"));
            let r = if so_sanh {
                mo_phong::so_sanh(&host, cong, n, k, lan, &bao, None)
            } else {
                mo_phong::chay_luot(&host, cong, n, k, lan, &bao, None, false, None).map(|h| h.into_iter().collect())
            };
            let hang = r.unwrap_or_else(|e| {
                bao(&format!("lỗi: {e}"));
                Vec::new()
            });
            *kq.lock().unwrap() = Some(hang);
            nk.ghi_lang();
        });
    }

    pub fn bam_ban(&mut self) {
        self.chay_nen(false);
    }

    pub fn bam_so_sanh(&mut self) {
        self.chay_nen(true);
    }

    /// Hiện kết quả một lượt mô phỏng lên bảng (kịch bản demo gọi lại để hiện lại bảng so sánh 4 chế độ).
    pub fn xong_mo_phong(&mut self, hang: Vec<HangSoSanh>) {
        self.dang_mo_phong = false;
        self.nhan_mo_phong = ("xong".into(), XANH_DAM);
        if hang.is_empty() {
            return;
        }
        if hang.len() == CHE_DO.len() {
            self.hang_so_sanh = hang.clone();
            self.bang.clear();
        }
        for h in &hang {
            match self.bang.iter_mut().find(|x| x.che_do == h.che_do) {
                Some(x) => *x = h.clone(),
                None => self.bang.push(h.clone()),
            }
        }
        let sai: Vec<&str> = hang.iter().filter(|h| h.sai_pham != 0).map(|h| h.che_do.as_str()).collect();
        let dung: Vec<&str> = hang.iter().filter(|h| h.sai_pham == 0).map(|h| h.che_do.as_str()).collect();
        self.ket_luan = format!(
            "{}{}",
            if sai.is_empty() { String::new() } else { format!("Sai phạm: {}. ", sai.join(", ")) },
            if dung.is_empty() { String::new() } else { format!("Không sai phạm: {}.", dung.join(", ")) }
        );
    }

    // ------------------------------------------------------------ đọc định kỳ (250 ms)
    fn doc_so_lieu(&mut self) {
        let xong = self.ket_qua.lock().unwrap().take();
        if let Some(hang) = xong {
            self.xong_mo_phong(hang);
        }
        if !self.lop.dang_chay() || bay_gio() - self.lan_doc < 0.25 {
            return;
        }
        self.lan_doc = bay_gio();
        let tk = self.lop.thong_ke_day_du();
        self.tt = Some(self.lop.trang_thai());
        if let Some(cd) = lay_str(&tk, "che_do") {
            if cd != self.che_do && !self.dang_mo_phong {
                self.che_do = CHE_DO.iter().find(|x| **x == cd).copied().unwrap_or("khoa"); // mô phỏng/quản trị đổi chế độ từ xa
            }
        }
        let s = |k: &str| so_us(tk.get(k));
        self.dinh_ky.in_neu_den_han(|| format!("{}: {} yc, tc {}, {} yc/s, sai phạm {}; {}", s("che_do"), s("so_yc"), s("thanh_cong"), s("yc_s"), s("sai_pham_tong"), s("tom_tat")));
        self.tk = Some(tk);
    }

    // ------------------------------------------------------------ vẽ
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.doc_so_lieu();
        let chay = self.lop.dang_chay();
        let start_bat = self.start_bat();
        ui.horizontal_top(|ui| {
            nhom(ui, "Kết nối", |ui| {
                ui.set_width(150.0);
                ui.set_min_height(196.0);
                egui::Grid::new("kn_sv").num_columns(2).show(ui, |ui| {
                    ui.label("IP:");
                    o_nhap(ui, &mut self.o_ip, 96.0, start_bat);
                    ui.end_row();
                    ui.label("Cổng:");
                    o_nhap(ui, &mut self.o_cong, 60.0, start_bat);
                    ui.end_row();
                });
                ui.add_space(4.0);
                if nut(ui, "START SERVER", XANH_LA, start_bat, 146.0) {
                    self.bam_start();
                }
                if nut(ui, "STOP SERVER", DO, chay && !self.dang_dung.load(Ordering::SeqCst), 146.0) {
                    self.bam_stop();
                }
            });
            nhom(ui, "Cách xử lý tranh chấp (đổi được khi đang chạy)", |ui| {
                ui.set_width(330.0);
                ui.set_min_height(196.0);
                for (i, cd) in CHE_DO.iter().enumerate() {
                    let chu = RichText::new(format!("{cd:<11} {}", TEN_CHE_DO[i])).monospace().size(11.5);
                    if ui.radio(self.che_do == *cd, chu).clicked() && self.che_do != *cd {
                        self.chon_che_do(cd);
                    }
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Độ trễ xử lý: ngẫu nhiên 0 –");
                    o_nhap(ui, &mut self.o_tre, 34.0, true);
                    ui.label("ms");
                    if nut(ui, "Áp dụng", XANH_DUONG, true, 60.0) {
                        self.bam_tre();
                    }
                });
            });
            nhom(ui, "Chỉ số trực tiếp (từ lần đặt lại)", |ui| {
                ui.set_min_height(196.0);
                ui.set_width(ui.available_width().max(420.0));
                ui.horizontal_top(|ui| {
                    let tk = self.tk.clone().unwrap_or_default();
                    let s = |k: &str| so_us(tk.get(k));
                    let co = !tk.is_empty();
                    let gia_tri: Vec<(&str, String)> = vec![
                        ("Kết nối / người", if co { format!("{} / {}", s("so_ket_noi"), s("so_nguoi")) } else { "–".into() }),
                        ("Yêu cầu/giây", if co { format!("{}  (TB {})", s("yc_s"), s("thong_luong")) } else { "–".into() }),
                        ("Tổng yêu cầu", if co { s("so_yc") } else { "–".into() }),
                        ("Thành công", if co { s("thanh_cong") } else { "–".into() }),
                        ("Từ chối", if co { tu_choi_chu(&tk) } else { "–".into() }),
                        ("Xung đột / thử lại", if co { format!("{} / {} (hết lượt {})", s("xung_dot"), s("thu_lai"), s("het_lan_thu")) } else { "–".into() }),
                        ("Hàng đợi (lớn nhất)", if co { format!("{}  ({})", s("hang_doi"), s("hang_doi_max")) } else { "–".into() }),
                        ("Xử lý p50/p95/p99", if co { format!("{} / {} / {} ms", ms(tk.get("p50_us")), ms(tk.get("p95_us")), ms(tk.get("p99_us"))) } else { "–".into() }),
                    ];
                    ui.vertical(|ui| {
                        ui.set_width(290.0);
                        egui::Grid::new("chi_so").num_columns(2).spacing([6.0, 3.0]).show(ui, |ui| {
                            for (chu, v) in gia_tri {
                                ui.label(format!("{chu}:"));
                                ui.add(egui::Label::new(RichText::new(v).strong()).truncate());
                                ui.end_row();
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        let sp = tk.get("sai_pham_tong").and_then(Value::as_i64);
                        let (nen, chu) = match sp {
                            None => (Color32::from_gray(0xe0), Color32::BLACK),
                            Some(0) => (mau("#70d070"), Color32::BLACK),
                            Some(_) => (mau("#ff6060"), Color32::WHITE),
                        };
                        nhan_nen(ui, &format!("SAI PHẠM: {}", sp.map_or("–".into(), |x| x.to_string())), nen, chu, 17.0, 180.0);
                        if co {
                            let mut ct = vec![tk.get("sai_pham").and_then(Value::as_array).map(|a| a.iter().map(|x| format!("{} {}", so_us(x.get("ten")), so_us(x.get("so")))).collect::<Vec<_>>().join(", ")).unwrap_or_default()];
                            for v in tk.get("vi_du_sai_pham").and_then(Value::as_array).into_iter().flatten().take(2) {
                                ct.push(format!("• {}", py_str(v)));
                            }
                            ui.set_max_width(190.0);
                            ui.add(egui::Label::new(RichText::new(ct.join("\n")).size(11.0)).wrap());
                        }
                    });
                });
            });
        });
        ui.horizontal_top(|ui| {
            nhom(ui, T::TEN_TAI_NGUYEN, |ui| {
                ui.set_width(420.0);
                ui.set_min_height(230.0);
                self.tn.ve(ui, self.tt.as_ref().filter(|_| self.da_chay), self.tk.as_ref());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if nut(ui, "Đặt lại", CAM, true, 80.0) {
                        self.bam_dat_lai();
                    }
                });
            });
            nhom(ui, "Mô phỏng tải (n người ảo trên k kết nối, rào xuất phát)", |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(230.0);
                ui.horizontal(|ui| {
                    ui.label("n:");
                    o_nhap(ui, &mut self.o_n, 52.0, true);
                    ui.label("  k:");
                    o_nhap(ui, &mut self.o_k, 44.0, true);
                    ui.label("  lần/người:");
                    o_nhap(ui, &mut self.o_lan, 30.0, true);
                });
                ui.horizontal(|ui| {
                    if nut(ui, "Bắn đồng loạt", CAM, !self.dang_mo_phong, 120.0) {
                        self.bam_ban();
                    }
                    if nut(ui, "So sánh 4 chế độ", XANH_DUONG, !self.dang_mo_phong, 140.0) {
                        self.bam_so_sanh();
                    }
                    ui.label(RichText::new(&self.nhan_mo_phong.0).color(self.nhan_mo_phong.1));
                });
                ve_bang(ui, &self.bang);
                ui.add(egui::Label::new(RichText::new(&self.ket_luan).size(11.0)).wrap());
            });
        });
        nhom(ui, "Nhật ký", |ui| {
            self.nhat_ky.ve(ui, 6, ui.available_width());
        });
        let tt = if chay {
            let tk = self.tk.clone().unwrap_or_default();
            format!("Trạng thái: máy chủ chạy tại {} – chế độ {}, {} kết nối – {}", self.lop.dia_chi(), so_us(tk.get("che_do")), so_us(tk.get("so_ket_noi")), so_us(tk.get("tom_tat")))
        } else {
            "Trạng thái: máy chủ chưa chạy".into()
        };
        thanh_trang_thai(ui, &tt);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }

    pub fn dong(&mut self) {
        self.lop.dung();
    }
}

fn tu_choi_chu(tk: &Obj) -> String {
    let mut s = so_us(tk.get("tu_choi"));
    if let Some(m) = tk.get("ma_loi").and_then(Value::as_object).filter(|m| !m.is_empty()) {
        let mut ds: Vec<(&String, &Value)> = m.iter().collect();
        ds.sort_by(|a, b| a.0.cmp(b.0));
        s += &format!("  ({})", ds.iter().map(|(k, v)| format!("{k} {}", py_str(v))).collect::<Vec<_>>().join(", "));
    }
    s
}

/// Bảng kết quả so sánh (như ttk.Treeview): dòng có sai phạm nền đỏ, không sai phạm nền xanh.
fn ve_bang(ui: &mut egui::Ui, bang: &[HangSoSanh]) {
    let cot: [(&str, f32); 6] = [("Chế độ", 96.0), ("Thành công", 82.0), ("Sai phạm", 72.0), ("Xung đột", 72.0), ("yc/s", 62.0), ("p99 ms", 70.0)];
    let rong: f32 = cot.iter().map(|c| c.1).sum();
    let cao_dong = 19.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(rong, cao_dong * 5.0 + 2.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, Color32::WHITE);
    let phong = egui::FontId::proportional(12.0);
    let mut y = rect.min.y;
    let ve_dong = |y: f32, chu: [String; 6], nen: Color32, mau_chu: Color32| {
        p.rect_filled(egui::Rect::from_min_size(egui::pos2(rect.min.x, y), egui::vec2(rong, cao_dong)), 0.0, nen);
        let mut x = rect.min.x;
        for (i, (c, w)) in chu.iter().zip(cot.iter().map(|c| c.1)).enumerate() {
            if i == 0 {
                p.text(egui::pos2(x + 4.0, y + cao_dong / 2.0), egui::Align2::LEFT_CENTER, c, phong.clone(), mau_chu);
            } else {
                p.text(egui::pos2(x + w - 5.0, y + cao_dong / 2.0), egui::Align2::RIGHT_CENTER, c, phong.clone(), mau_chu);
            }
            x += w;
            p.line_segment([egui::pos2(x, y), egui::pos2(x, y + cao_dong)], egui::Stroke::new(1.0_f32, Color32::from_gray(0xd8)));
        }
    };
    ve_dong(y, cot.map(|c| c.0.to_string()), Color32::from_gray(0xe8), Color32::BLACK);
    y += cao_dong;
    for h in bang {
        let (nen, chu) = if h.sai_pham != 0 { (mau("#ffc8c8"), DO_DAM) } else { (mau("#d8f5d8"), Color32::BLACK) };
        ve_dong(y, [h.che_do.clone(), h.thanh_cong.to_string(), h.sai_pham.to_string(), h.xung_dot.to_string(), h.yc_s.to_string(), chu_p99(h.p99_ms)], nen, chu);
        y += cao_dong;
    }
    p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, Color32::from_gray(0x90)));
}

impl NhatKy {
    /// Chỉ yêu cầu vẽ lại (không thêm dòng) – luồng nền báo có kết quả mới.
    pub fn ghi_lang(&self) {
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint_after(NHIP_VE); // gộp: vẽ lại tối đa ~10 lần/giây
        }
    }
}

// ===================================================================== cửa sổ người dùng
/// Việc luồng mạng gửi sang luồng giao diện.
pub enum SuKien {
    DaKetNoi(Arc<KetNoiKhach>),
    KhongKetNoi(String),
    CapNhat(Obj),
    Tin(Obj),
    Dong(usize),
    Kq { loai: String, truong: Obj, tin: Obj, rtt: f64 },
}

type HangSuKien = Arc<Mutex<VecDeque<SuKien>>>;

/// Phần chung của cửa sổ người dùng (truyền cho các hàm của UngDung).
pub struct NguoiDung {
    pub ten: String,
    pub o_ten: String,
    pub o_ip: String,
    pub o_cong: String,
    pub kn: Option<Arc<KetNoiKhach>>,
    pub tt: Option<Obj>, // cap_nhat mới nhất
    pub rtt_cuoi: Option<f64>,
    pub nhat_ky: NhatKy,
    pub thanh_tt: String,
    dang_ket_noi: bool,
    hang: HangSuKien,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl NguoiDung {
    fn bao_ve(ctx: &Arc<Mutex<Option<egui::Context>>>) {
        if let Some(c) = ctx.lock().unwrap().as_ref() {
            c.request_repaint_after(NHIP_VE); // gộp: vẽ lại tối đa ~10 lần/giây
        }
    }

    /// Gửi một yêu cầu; kết quả về UngDung::ket_qua (luồng đọc -> hàng sự kiện -> luồng giao diện).
    pub fn gui_yc(&mut self, loai: &str, truong: Obj) {
        let Some(kn) = self.kn.clone() else {
            self.nhat_ky.ghi("Chưa kết nối");
            return;
        };
        let (h, c, l, t) = (Arc::clone(&self.hang), Arc::clone(&self.ctx), loai.to_string(), truong.clone());
        let kq = kn.gui_yc(
            loai,
            truong,
            Some(Box::new(move |tin: &Obj, rtt: f64| {
                h.lock().unwrap().push_back(SuKien::Kq { loai: l, truong: t, tin: tin.clone(), rtt });
                Self::bao_ve(&c);
            })),
        );
        if let Err(e) = kq {
            self.nhat_ky.ghi(&format!("Gửi lỗi: {e}"));
        }
    }

    fn cap_nhat_thanh_tt(&mut self) {
        if let (Some(_), Some(tt)) = (&self.kn, &self.tt) {
            let rtt = self.rtt_cuoi.map_or(String::new(), |r| format!(", khứ hồi {r:.1} ms"));
            self.thanh_tt = format!("Kết nối {}:{} – máy chủ chế độ {}{rtt}", self.o_ip, self.o_cong, so_us(tt.get("che_do")));
        }
    }
}

/// Phần nghiệp vụ của cửa sổ người dùng (mỗi bài một kiểu).
pub trait UngDung: Sized {
    const TIEU_DE: &'static str;
    fn moi(ten: &str) -> Self;
    fn ve(&mut self, ui: &mut egui::Ui, nd: &mut NguoiDung);
    fn cap_nhat(&mut self, nd: &mut NguoiDung, tt: &Obj);
    fn ket_qua(&mut self, nd: &mut NguoiDung, loai: &str, truong: &Obj, tin: &Obj, rtt: f64);
    fn su_kien(&mut self, nd: &mut NguoiDung, text: &str) {
        nd.nhat_ky.ghi(&format!("* {text}"));
    }
    /// Gọi mỗi 200 ms trên luồng giao diện (đồng hồ đếm ngược…).
    fn moi_nhip(&mut self, _nd: &mut NguoiDung) {}
    fn dong_dinh_ky(&self, nd: &NguoiDung) -> String;
    fn da_ket_noi(&mut self, _nd: &mut NguoiDung) {}
    fn mat_ket_noi(&mut self, _nd: &mut NguoiDung) {}
}

pub struct CuaSoNguoiDung<U: UngDung> {
    pub nd: NguoiDung,
    pub ud: U,
    lan_nhip: f64,
    dinh_ky: ThongKeDinhKy,
}

impl<U: UngDung> CuaSoNguoiDung<U> {
    pub fn moi(ten: &str, host: &str, cong: u16) -> Self {
        CuaSoNguoiDung {
            nd: NguoiDung {
                ten: ten.into(),
                o_ten: ten.into(),
                o_ip: host.into(),
                o_cong: cong.to_string(),
                kn: None,
                tt: None,
                rtt_cuoi: None,
                nhat_ky: NhatKy::new(ten),
                thanh_tt: "Chưa kết nối".into(),
                dang_ket_noi: false,
                hang: Arc::new(Mutex::new(VecDeque::new())),
                ctx: Arc::new(Mutex::new(None)),
            },
            ud: U::moi(ten),
            lan_nhip: 0.0,
            dinh_ky: ThongKeDinhKy::new(ten),
        }
    }

    pub fn tieu_de(&self) -> String {
        format!("{} – {}", U::TIEU_DE, self.nd.ten)
    }

    // ------------------------------------------------------------ kết nối
    pub fn bam_ket_noi(&mut self) {
        let nd = &mut self.nd;
        let (ten, host) = (nd.o_ten.trim().to_string(), nd.o_ip.trim().to_string());
        let Ok(cong) = nd.o_cong.trim().parse::<u16>() else {
            nd.nhat_ky.ghi("Cổng không hợp lệ");
            return;
        };
        nd.ten = ten.clone();
        nd.dang_ket_noi = true;
        let kn = Arc::new(KetNoiKhach::default());
        let id = Arc::as_ptr(&kn) as usize;
        for (loai, h, c) in [(0, Arc::clone(&nd.hang), Arc::clone(&nd.ctx)), (1, Arc::clone(&nd.hang), Arc::clone(&nd.ctx))] {
            let f: crate::khach::GoiLai = Box::new(move |tin: &Obj| {
                h.lock().unwrap().push_back(if loai == 0 { SuKien::CapNhat(tin.clone()) } else { SuKien::Tin(tin.clone()) });
                NguoiDung::bao_ve(&c);
            });
            if loai == 0 {
                kn.dat_khi_cap_nhat(f);
            } else {
                kn.dat_khi_tin(f);
            }
        }
        let (h, c) = (Arc::clone(&nd.hang), Arc::clone(&nd.ctx));
        kn.dat_khi_dong(Box::new(move || {
            h.lock().unwrap().push_back(SuKien::Dong(id));
            NguoiDung::bao_ve(&c);
        }));
        let (h, c) = (Arc::clone(&nd.hang), Arc::clone(&nd.ctx));
        std::thread::spawn(move || {
            // kết nối trên luồng nền: không đơ giao diện
            let su = match kn.ket_noi(&host, cong, &ten, "nguoi_dung", None, 5.0) {
                Ok(_) => SuKien::DaKetNoi(kn),
                Err(e) => SuKien::KhongKetNoi(e),
            };
            h.lock().unwrap().push_back(su);
            NguoiDung::bao_ve(&c);
        });
    }

    pub fn bam_roi(&mut self) {
        if let Some(kn) = self.nd.kn.take() {
            kn.dong_im();
            self.mat_ket_noi(None, "Đã rời");
        }
    }

    fn mat_ket_noi(&mut self, id: Option<usize>, ly_do: &str) {
        if let Some(id) = id {
            if self.nd.kn.as_ref().map(|k| Arc::as_ptr(k) as usize) != Some(id) {
                return;
            }
        }
        self.nd.kn = None;
        self.nd.nhat_ky.ghi(ly_do);
        self.nd.thanh_tt = ly_do.into();
        self.ud.mat_ket_noi(&mut self.nd);
    }

    fn nhan_cap_nhat(&mut self, tt: Obj) {
        self.nd.tt = Some(tt.clone());
        self.ud.cap_nhat(&mut self.nd, &tt);
        self.nd.cap_nhat_thanh_tt();
    }

    /// Xử lý mọi việc luồng mạng gửi sang (gọi đầu mỗi lần vẽ; kịch bản demo cũng gọi được).
    pub fn xu_ly_su_kien(&mut self) {
        loop {
            let Some(su) = self.nd.hang.lock().unwrap().pop_front() else { break };
            match su {
                SuKien::DaKetNoi(kn) => {
                    self.nd.dang_ket_noi = false;
                    self.nd.kn = Some(Arc::clone(&kn));
                    let chao = kn.chao.lock().unwrap().clone().unwrap_or_default();
                    self.nd.nhat_ky.ghi(&format!("Đã kết nối {} (chế độ {})", so_us(chao.get("may_chu")), so_us(chao.get("che_do"))));
                    if let Some(tt) = kn.trang_thai() {
                        self.nhan_cap_nhat(tt);
                    }
                    kn.bat_dau_doc();
                    self.ud.da_ket_noi(&mut self.nd);
                }
                SuKien::KhongKetNoi(loi) => {
                    self.nd.dang_ket_noi = false;
                    self.nd.nhat_ky.ghi(&format!("Không kết nối được: {loi}"));
                }
                SuKien::CapNhat(tt) => {
                    if self.nd.kn.is_some() {
                        self.nhan_cap_nhat(tt);
                    }
                }
                SuKien::Tin(tin) => match lay_str(&tin, "loai") {
                    Some("su_kien") => {
                        let text = lay_str(&tin, "noi_dung").unwrap_or("").to_string();
                        self.ud.su_kien(&mut self.nd, &text);
                    }
                    Some("loi") => self.nd.nhat_ky.ghi(&format!("Lỗi: {}", so_us(tin.get("ly_do")))),
                    _ => {}
                },
                SuKien::Dong(id) => self.mat_ket_noi(Some(id), "Mất kết nối máy chủ"),
                SuKien::Kq { loai, truong, tin, rtt } => {
                    self.nd.rtt_cuoi = Some(rtt);
                    self.ud.ket_qua(&mut self.nd, &loai, &truong, &tin, rtt);
                    self.nd.cap_nhat_thanh_tt();
                }
            }
        }
        if bay_gio() - self.lan_nhip >= 0.2 {
            self.lan_nhip = bay_gio();
            self.ud.moi_nhip(&mut self.nd);
            if self.nd.kn.is_some() && self.nd.tt.is_some() {
                let (ud, nd) = (&self.ud, &self.nd);
                self.dinh_ky.in_neu_den_han(|| ud.dong_dinh_ky(nd));
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if self.nd.ctx.lock().unwrap().is_none() {
            *self.nd.ctx.lock().unwrap() = Some(ui.ctx().clone());
        }
        self.xu_ly_su_kien();
        let rang_buoc = self.nd.kn.is_none() && !self.nd.dang_ket_noi;
        ui.horizontal(|ui| {
            ui.label("Tên:");
            o_nhap(ui, &mut self.nd.o_ten, 64.0, rang_buoc);
            ui.label(" IP:");
            o_nhap(ui, &mut self.nd.o_ip, 84.0, rang_buoc);
            ui.label(" Cổng:");
            o_nhap(ui, &mut self.nd.o_cong, 48.0, rang_buoc);
            if nut(ui, "Kết nối", XANH_LA, rang_buoc, 70.0) {
                self.bam_ket_noi();
            }
            if nut(ui, "Rời", DO, self.nd.kn.is_some(), 44.0) {
                self.bam_roi();
            }
        });
        self.ud.ve(ui, &mut self.nd);
        nhom(ui, "Nhật ký", |ui| {
            self.nd.nhat_ky.ve(ui, 4, ui.available_width());
        });
        thanh_trang_thai(ui, &self.nd.thanh_tt.clone());
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }

    pub fn dong(&mut self) {
        if let Some(kn) = self.nd.kn.take() {
            kn.dong_im();
        }
    }
}

// ===================================================================== chạy riêng một cửa sổ
/// Một cửa sổ chạy riêng (server_gui, <nghiệp vụ>_gui).
pub trait CuaSo {
    fn ve(&mut self, ui: &mut egui::Ui);
    fn dong(&mut self);
}

impl<N: NghiepVu, T: TaiNguyen<N>> CuaSo for CuaSoServer<N, T> {
    fn ve(&mut self, ui: &mut egui::Ui) {
        self.ui(ui);
    }
    fn dong(&mut self) {
        CuaSoServer::dong(self);
    }
}

impl<U: UngDung> CuaSo for CuaSoNguoiDung<U> {
    fn ve(&mut self, ui: &mut egui::Ui) {
        self.ui(ui);
    }
    fn dong(&mut self) {
        CuaSoNguoiDung::dong(self);
    }
}

struct VoCuaSo<C: CuaSo>(C);

impl<C: CuaSo> eframe::App for VoCuaSo<C> {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| self.0.ve(ui));
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.0.dong();
    }
}

/// Mở một cửa sổ egui; chạy riêng nên nhật ký cũng in ra dòng lệnh (IN_RA).
pub fn chay_cua_so<C: CuaSo + 'static>(tieu_de: &str, co: [f32; 2], cs: C) {
    IN_RA.store(true, Ordering::SeqCst);
    let opt = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size(co).with_title(tieu_de), ..Default::default() };
    eframe::run_native(
        tieu_de,
        opt,
        Box::new(|cc| {
            cai_phong_chu(&cc.egui_ctx);
            dat_kieu(&cc.egui_ctx);
            Ok(Box::new(VoCuaSo(cs)))
        }),
    )
    .unwrap_or_else(|e| eprintln!("Không mở được cửa sổ: {e}"));
    std::process::exit(0);
}

// ===================================================================== kịch bản demo (Xvfb)
pub type DieuKien<A> = fn(&A) -> bool;

/// Bộ điều phối kịch bản: mỗi bước tự đặt "chờ x giây" (cho) hoặc "chờ tới khi điều kiện đúng"
/// (cho_khi, tối đa het_gio giây); vòng vẽ của egui vẫn chạy trong lúc chờ.
pub struct KichBan<A> {
    pub buoc: usize,
    han: f64,
    dk: Option<(DieuKien<A>, f64)>,
    pub het_gio: f64,
}

impl<A> Default for KichBan<A> {
    fn default() -> Self {
        KichBan { buoc: 0, han: 0.0, dk: None, het_gio: 90.0 }
    }
}

impl<A> KichBan<A> {
    pub fn cho(&mut self, giay: f64) {
        self.han = bay_gio() + giay;
        self.dk = None;
    }
    pub fn cho_khi(&mut self, dk: DieuKien<A>) {
        self.dk = Some((dk, bay_gio() + self.het_gio));
    }
    /// Some(true): tới bước kế; Some(false): còn chờ; None: chờ quá lâu.
    pub fn toi_luot(&mut self, app: &A) -> Option<bool> {
        match self.dk {
            Some((dk, han)) => {
                if dk(app) {
                    self.dk = None;
                    Some(true)
                } else if bay_gio() > han {
                    self.dk = None;
                    None
                } else {
                    Some(false)
                }
            }
            None => Some(bay_gio() >= self.han),
        }
    }
}

/// Ảnh RGB (rộng, cao, dữ liệu).
pub type Anh = (usize, usize, Vec<u8>);

/// Chụp cả cửa sổ egui (ViewportCommand::Screenshot), ảnh về ở một lần vẽ sau.
#[derive(Default)]
pub struct MayChup {
    pub anh: Option<Arc<egui::ColorImage>>,
    cho: bool,
}

impl MayChup {
    pub fn yeu_cau(&mut self, ctx: &egui::Context) {
        self.anh = None;
        self.cho = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
    }
    /// Gọi mỗi lần vẽ: nhận ảnh nếu đã có.
    pub fn nhan(&mut self, ctx: &egui::Context) {
        if !self.cho {
            return;
        }
        let anh = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if anh.is_some() {
            self.anh = anh;
            self.cho = false;
        }
    }
    /// Cắt một vùng (đơn vị điểm egui, nhân pixels_per_point) của ảnh đã chụp.
    pub fn cat(&self, vung: egui::Rect, ppp: f32) -> Anh {
        let anh = self.anh.as_ref().expect("chưa có ảnh chụp");
        let (w, h) = (anh.size[0], anh.size[1]);
        let r = egui::Rect::from_min_max(((vung.min - egui::vec2(2.0, 2.0)) * ppp).max(egui::pos2(0.0, 0.0)), (vung.max + egui::vec2(2.0, 2.0)) * ppp);
        let (x0, y0) = ((r.min.x as usize).min(w), (r.min.y as usize).min(h));
        let (x1, y1) = ((r.max.x.ceil() as usize).min(w), (r.max.y.ceil() as usize).min(h));
        let mut rgb = Vec::with_capacity((x1 - x0) * (y1 - y0) * 3);
        for y in y0..y1 {
            for p in &anh.pixels[y * w + x0..y * w + x1] {
                rgb.extend_from_slice(&[p.r(), p.g(), p.b()]);
            }
        }
        (x1 - x0, y1 - y0, rgb)
    }
}

/// Ghép ảnh như `convert ( a b +append ) ( c d +append ) -append -border 4` của bản Python: mỗi hàng
/// xếp ngang (canh trên), các hàng xếp dọc (canh trái), nền và viền xám #d0d0d0, cách nhau 6 px.
pub fn ghep_anh(hang: &[Vec<Anh>]) -> Anh {
    let (vien, khe) = (4usize, 6usize);
    let rong_hang: Vec<usize> = hang.iter().map(|h| h.iter().map(|a| a.0).sum::<usize>() + khe * h.len().saturating_sub(1)).collect();
    let cao_hang: Vec<usize> = hang.iter().map(|h| h.iter().map(|a| a.1).max().unwrap_or(0)).collect();
    let w = rong_hang.iter().copied().max().unwrap_or(0) + 2 * vien;
    let h = cao_hang.iter().sum::<usize>() + khe * hang.len().saturating_sub(1) + 2 * vien;
    let mut rgb = vec![0xd0u8; w * h * 3];
    let mut y0 = vien;
    for (i, ds) in hang.iter().enumerate() {
        let mut x0 = vien;
        for (aw, ah, a) in ds {
            for y in 0..*ah {
                let d = ((y0 + y) * w + x0) * 3;
                rgb[d..d + aw * 3].copy_from_slice(&a[y * aw * 3..(y + 1) * aw * 3]);
            }
            x0 += aw + khe;
        }
        y0 += cao_hang[i] + khe;
    }
    (w, h, rgb)
}

/// CRC-32 (đa thức 0xEDB88320, như zlib.crc32) dùng cho các khối PNG.
fn crc32(du_lieu: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in du_lieu {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// PNG viết tay: chữ ký 8 byte + khối IHDR, IDAT (zlib các hàng), IEND; mỗi khối CRC-32. Mỗi hàng chọn
/// một bộ lọc PNG (0 None, 1 Sub, 2 Up, 4 Paeth) cho tổng |byte| nhỏ nhất – cách chọn thường dùng của libpng.
pub fn ghi_png(duong_dan: &str, anh: &Anh) -> io::Result<()> {
    let (w, h, anh) = (anh.0, anh.1, &anh.2);
    let khoi = |out: &mut Vec<u8>, kieu: &[u8], du_lieu: &[u8]| {
        out.extend_from_slice(&(du_lieu.len() as u32).to_be_bytes());
        let mut kd = kieu.to_vec();
        kd.extend_from_slice(du_lieu);
        out.extend_from_slice(&kd);
        out.extend_from_slice(&crc32(&kd).to_be_bytes());
    };
    let n = w * 3;
    let mut hang = Vec::with_capacity((n + 1) * h);
    let khong = vec![0u8; n];
    for y in 0..h {
        let cur = &anh[y * n..(y + 1) * n];
        let tren = if y > 0 { &anh[(y - 1) * n..y * n] } else { &khong[..] };
        let mut tot: Option<(u64, u8, Vec<u8>)> = None;
        for loc in [0u8, 1, 2, 4] {
            let v: Vec<u8> = (0..n)
                .map(|i| {
                    let a = if i >= 3 { cur[i - 3] } else { 0 };
                    let b = tren[i];
                    let c = if i >= 3 { tren[i - 3] } else { 0 };
                    let du_doan = match loc {
                        0 => 0,
                        1 => a,
                        2 => b,
                        _ => {
                            let p = a as i16 + b as i16 - c as i16;
                            let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
                            if pa <= pb && pa <= pc {
                                a
                            } else if pb <= pc {
                                b
                            } else {
                                c
                            }
                        }
                    };
                    cur[i].wrapping_sub(du_doan)
                })
                .collect();
            let diem: u64 = v.iter().map(|&x| (x as i8).unsigned_abs() as u64).sum();
            if tot.as_ref().is_none_or(|t| diem < t.0) {
                tot = Some((diem, loc, v));
            }
        }
        let (_, loc, v) = tot.unwrap();
        hang.push(loc);
        hang.extend_from_slice(&v);
    }
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

/// Ghi ảnh và báo "Đã ghi <tên> (n KB)" qua log.
pub fn ghi_va_bao(duong_dan: &str, anh: &Anh, log: impl Fn(&str)) {
    let ten = std::path::Path::new(duong_dan).file_name().map_or(duong_dan.to_string(), |t| t.to_string_lossy().into_owned());
    match ghi_png(duong_dan, anh) {
        Ok(()) => log(&format!("   Đã ghi {ten} ({} KB)", std::fs::metadata(duong_dan).map_or(0, |m| m.len() / 1024))),
        Err(e) => log(&format!("   Không ghi được {ten}: {e}")),
    }
}

// ===================================================================== bút vẽ (Canvas của Tk)
/// Đổi tọa độ cục bộ (gốc trên-trái của khung vẽ) sang tọa độ màn hình.
pub fn p(goc: egui::Pos2, x: f32, y: f32) -> egui::Pos2 {
    egui::pos2(goc.x + x, goc.y + y)
}

/// Đa giác bất kỳ (kể cả lõm) tô bằng các dải ngang 1 px (egui chỉ tô sẵn đa giác lồi).
pub fn to_da_giac(painter: &egui::Painter, diem: &[egui::Pos2], mau_to: Color32) {
    if diem.len() < 3 {
        return;
    }
    let (y0, y1) = diem.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
    let mut y = y0.floor() + 0.5;
    while y < y1 {
        let mut x: Vec<f32> = Vec::new();
        for i in 0..diem.len() {
            let (a, b) = (diem[i], diem[(i + 1) % diem.len()]);
            if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                x.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
            }
        }
        x.sort_by(f32::total_cmp);
        for c in x.chunks(2) {
            if c.len() == 2 {
                painter.rect_filled(egui::Rect::from_min_max(egui::pos2(c[0], y - 0.5), egui::pos2(c[1], y + 0.5)), 0.0, mau_to);
            }
        }
        y += 1.0;
    }
}

/// Đường cong trơn qua các điểm điều khiển (B-spline bậc hai như smooth=True của Tk).
pub fn duong_tron(diem: &[egui::Pos2], khep: bool) -> Vec<egui::Pos2> {
    if diem.len() < 3 {
        return diem.to_vec();
    }
    let n = diem.len();
    let mut ra = Vec::new();
    let mid = |a: egui::Pos2, b: egui::Pos2| egui::pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let dem = if khep { n } else { n - 2 };
    if !khep {
        ra.push(diem[0]);
    }
    for i in 0..dem {
        let (a, b, c) = (diem[i % n], diem[(i + 1) % n], diem[(i + 2) % n]);
        let (m0, m1) = (if !khep && i == 0 { a } else { mid(a, b) }, if !khep && i == dem - 1 { c } else { mid(b, c) });
        for k in 0..=8 {
            let t = k as f32 / 8.0;
            let u = 1.0 - t;
            ra.push(egui::pos2(u * u * m0.x + 2.0 * u * t * b.x + t * t * m1.x, u * u * m0.y + 2.0 * u * t * b.y + t * t * m1.y));
        }
    }
    ra
}

/// Chữ xoay (góc radian) – tên phố dọc.
pub fn chu_xoay(painter: &egui::Painter, tam: egui::Pos2, chu: &str, co: f32, mau_chu: Color32, goc: f32) {
    let g = painter.layout_no_wrap(chu.to_string(), egui::FontId::proportional(co), mau_chu);
    let (w, h) = (g.size().x, g.size().y);
    // vị trí góc trên-trái sau khi xoay quanh góc đó sao cho tâm chữ rơi vào `tam`
    let (c, s) = (goc.cos(), goc.sin());
    let dx = c * w / 2.0 - s * h / 2.0;
    let dy = s * w / 2.0 + c * h / 2.0;
    painter.add(egui::epaint::TextShape::new(egui::pos2(tam.x - dx, tam.y - dy), g, mau_chu).with_angle(goc));
}
