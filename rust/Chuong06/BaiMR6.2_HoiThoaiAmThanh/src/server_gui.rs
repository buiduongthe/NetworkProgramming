//! Server hội thoại âm thanh có giao diện (egui) – "Bảng điều khiển âm thanh".
//!
//! Hàng trên: nhóm Kết nối (IP, cổng TCP/UDP, Start/Stop Server); Điều khiển chung (Mở mic tất cả =
//! gỡ khóa, Tắt mic tất cả = khóa, Âm lượng chung); Micro của server (chọn thiết bị thật hoặc
//! "Giả lập (sin 330 Hz)", Start/Stop Mic, Mic Volume); Nghe (bật/tắt phát loa server).
//! Dưới: dòng trạng thái, mỗi client một hàng (mic, thanh đo mức, Khóa/Gỡ khóa, độ lợi 0–200 %,
//! âm lượng nghe client báo), nhật ký sự kiện cuộn.
//!
//! Lõi server (luồng nhận kết nối, luồng nhận UDP, luồng trộn 20 ms) là đúng phần lõi của bản dòng
//! lệnh (server.rs); giao diện chỉ gọi các hàm của lõi và đọc trạng thái phòng ~10 lần/giây để vẽ.

use crate::gui_chung::*;
use crate::khach_gui::do_micro_nen;
use crate::server::{self, MayChu};
use crate::thiet_bi::{self, NguonMic};
use eframe::egui::{self, Color32, RichText};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const TAN_SO_SERVER: f64 = 330.0;

/// Bản sao gọn một client để vẽ (đọc dưới khóa rồi thả khóa ngay).
pub struct HangKhach {
    pub ma: u32,
    pub ten: String,
    pub mic: &'static str,
    pub khoa: bool,
    pub muc: f64,
    pub do_loi: f64,
    pub nghe: f64,
}

pub struct ServerApp {
    pub ip: String,
    pub cong_tcp: String,
    pub cong_udp: String,
    pub mc: Option<Arc<MayChu>>,
    nhat_ky: Arc<Mutex<VecDeque<String>>>,
    pub trang_thai: String,
    loi: bool,
    ds_mic: Arc<Mutex<Option<Vec<String>>>>,
    chon_mic: usize,
    luong_mic: Option<cpal::Stream>,
    luong_loa: Option<cpal::Stream>,
    pub mo_ta_mic: String,
    pub mo_ta_loa: String,
    mic_vol: f64,
    keo: Option<(u32, f32)>, // thanh trượt đang kéo: (mã client, giá trị) – u32::MAX = âm lượng chung
    cs_muc: std::collections::HashMap<u32, f64>,
    pub cao_nhat_ky: f32, // chiều cao khung nhật ký
}

impl ServerApp {
    pub fn new(ip: &str, cong_tcp: u16, cong_udp: u16) -> ServerApp {
        let app = ServerApp {
            ip: ip.into(),
            cong_tcp: cong_tcp.to_string(),
            cong_udp: cong_udp.to_string(),
            mc: None,
            nhat_ky: Arc::new(Mutex::new(VecDeque::new())),
            trang_thai: "Server chưa chạy".into(),
            loi: false,
            ds_mic: Arc::new(Mutex::new(None)),
            chon_mic: 0,
            luong_mic: None,
            luong_loa: None,
            mo_ta_mic: "chưa bật".into(),
            mo_ta_loa: "chưa bật".into(),
            mic_vol: 1.0,
            keo: None,
            cs_muc: Default::default(),
            cao_nhat_ky: 260.0,
        };
        do_micro_nen(&app.ds_mic, None);
        app
    }

    fn ghi(&self, text: &str) {
        ghi_nhat_ky(&self.nhat_ky, text);
    }

    // ------------------------------------------------------------ Thao tác (nút bấm và kịch bản demo)
    pub fn bam_start_server(&mut self) {
        if self.mc.is_some() {
            return;
        }
        let (tcp, udp) = (self.cong_tcp.trim().parse::<u16>(), self.cong_udp.trim().parse::<u16>());
        let (Ok(tcp), Ok(udp)) = (tcp, udp) else {
            self.trang_thai = "Cổng không hợp lệ".into();
            self.loi = true;
            return;
        };
        let nk = Arc::clone(&self.nhat_ky);
        match server::khoi_dong(self.ip.trim(), tcp, udp, Box::new(move |t| ghi_nhat_ky(&nk, t))) {
            Ok(mc) => {
                self.trang_thai = format!("Server đang chạy tại {}:{tcp} (UDP {udp})", self.ip.trim());
                self.loi = false;
                self.mc = Some(mc);
            }
            Err(e) => {
                self.trang_thai = format!("Không mở được cổng: {e}");
                self.loi = true;
            }
        }
    }

    pub fn bam_stop_server(&mut self) {
        if let Some(mc) = self.mc.take() {
            {
                let p = mc.phong.lock().unwrap();
                server::in_tong_ket(&mc, &p);
            }
            mc.dung_lai();
            self.ghi("Dừng server");
        }
        // đóng micro/loa của server cùng lúc
        (self.luong_mic, self.luong_loa) = (None, None);
        self.mo_ta_mic = "đã tắt".into();
        self.mo_ta_loa = "đã tắt".into();
        self.trang_thai = "Server đã dừng".into();
    }

    /// Khóa (true) / gỡ khóa (false) mic của các client `ds`; ds rỗng = tất cả.
    pub fn khoa(&mut self, ds: &[u32], khoa: bool) {
        let Some(mc) = &self.mc else { return };
        let mut p = mc.phong.lock().unwrap();
        let (ds, ai): (Vec<u32>, String) = if ds.is_empty() {
            (p.khach.keys().copied().collect(), "tất cả".into())
        } else {
            (ds.to_vec(), ds.iter().filter_map(|m| p.khach.get(m)).map(|k| k.ten.clone()).collect::<Vec<_>>().join(", "))
        };
        if !ds.is_empty() {
            mc.khoa_mic(&mut p, &ds, khoa, &ai);
        }
    }

    pub fn dat_do_loi(&mut self, ma: u32, g: f64) {
        let Some(mc) = &self.mc else { return };
        let mut p = mc.phong.lock().unwrap();
        let ten = p.khach.get(&ma).map(|k| k.ten.clone()).unwrap_or_default();
        mc.dat_do_loi(&mut p, &[ma], g, &ten);
    }

    pub fn dat_am_luong_chung(&mut self, g: f64) {
        let Some(mc) = &self.mc else { return };
        let mut p = mc.phong.lock().unwrap();
        mc.dat_am_luong_chung(&mut p, g);
    }

    /// Server tự nói: nguồn giả lập 330 Hz hoặc micro thật của máy server.
    pub fn bam_start_mic(&mut self) {
        let Some(mc) = self.mc.clone() else { return };
        let ds = self.ds_mic.lock().unwrap().clone().unwrap_or_default();
        let mut nguon = NguonMic::gia_lap(TAN_SO_SERVER);
        self.luong_mic = None;
        self.mo_ta_mic = format!("GIẢ LẬP – sin {TAN_SO_SERVER} Hz");
        if self.chon_mic > 0 {
            let ten = &ds[self.chon_mic - 1];
            let hang = thiet_bi::hang_moi();
            match thiet_bi::mo_micro(ten, Arc::clone(&hang)) {
                Ok((s, mt)) => {
                    self.luong_mic = Some(s);
                    nguon = NguonMic::ThietBi(hang);
                    self.mo_ta_mic = format!("THẬT – {ten} ({mt})");
                }
                Err(e) => self.mo_ta_mic = format!("GIẢ LẬP – không mở được micro thật: {e}"),
            }
        }
        mc.log(&format!("Server bật micro: {}", self.mo_ta_mic));
        let mut p = mc.phong.lock().unwrap();
        p.mic_server = Some(nguon);
        p.mic_server_vol = self.mic_vol;
    }

    pub fn bam_stop_mic(&mut self) {
        self.luong_mic = None;
        self.mo_ta_mic = "đã tắt".into();
        if let Some(mc) = &self.mc {
            mc.log("Server tắt micro");
            let mut p = mc.phong.lock().unwrap();
            p.mic_server = None;
            p.muc_mic_server = f64::NEG_INFINITY;
        }
    }

    pub fn dat_mic_vol(&mut self, g: f64) {
        self.mic_vol = g;
        if let Some(mc) = &self.mc {
            mc.phong.lock().unwrap().mic_server_vol = g;
        }
    }

    /// Nghe: phát ra loa máy server tổng tiếng các client (không gồm tiếng server).
    pub fn bat_nghe(&mut self, bat: bool) {
        let Some(mc) = &self.mc else { return };
        let mut p = mc.phong.lock().unwrap();
        if !bat {
            self.luong_loa = None;
            p.loa_server = None;
            self.mo_ta_loa = "đã tắt".into();
            return;
        }
        let hang = thiet_bi::hang_moi();
        match thiet_bi::mo_loa(Arc::clone(&hang)) {
            Ok((s, mt)) => {
                self.luong_loa = Some(s);
                self.mo_ta_loa = format!("THẬT – {mt}");
            }
            Err(e) => self.mo_ta_loa = format!("không có loa ({e}) – chỉ đo mức"),
        }
        p.loa_server = Some(hang);
    }

    /// Đọc trạng thái phòng (dưới khóa) thành bản sao để vẽ.
    pub fn chup_phong(&self) -> (Vec<HangKhach>, f64, f64, f64, bool, bool) {
        let Some(mc) = &self.mc else { return (Vec::new(), 1.0, f64::NEG_INFINITY, f64::NEG_INFINITY, false, false) };
        let p = mc.phong.lock().unwrap();
        let ds = p
            .khach
            .values()
            .map(|k| HangKhach {
                ma: k.ma,
                ten: k.ten.clone(),
                mic: match (k.mic_mo(), k.server_khoa, k.tu_tat) {
                    (true, _, _) => "mở",
                    (_, true, true) => "bị khóa + tự tắt",
                    (_, true, _) => "bị khóa",
                    _ => "tự tắt",
                },
                khoa: k.server_khoa,
                muc: k.muc_vao,
                do_loi: k.do_loi,
                nghe: k.am_luong_nghe,
            })
            .collect();
        (ds, p.am_luong_chung, p.muc_mic_server, p.muc_nghe_server, p.mic_server.is_some(), p.loa_server.is_some())
    }

    // ------------------------------------------------------------ Vẽ
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let chay = self.mc.is_some();
        let (ds, al_chung, muc_mic, muc_nghe, dang_noi, dang_nghe) = self.chup_phong();
        ui.horizontal_top(|ui| {
            nhom(ui, "Kết nối", |ui| {
                egui::Grid::new("ket_noi_sv").show(ui, |ui| {
                    ui.label("IP:");
                    ui.add_enabled_ui(!chay, |ui| ui.add_sized([110.0, 20.0], egui::TextEdit::singleline(&mut self.ip)));
                    ui.end_row();
                    ui.label("Cổng TCP:");
                    ui.add_enabled_ui(!chay, |ui| ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut self.cong_tcp)));
                    ui.end_row();
                    ui.label("Cổng UDP:");
                    ui.add_enabled_ui(!chay, |ui| ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut self.cong_udp)));
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if nut(ui, "Start Server", XANH_LA, !chay) {
                        self.bam_start_server();
                    }
                    if nut(ui, "Stop Server", DO_NHAT, chay) {
                        self.bam_stop_server();
                    }
                });
            });
            nhom(ui, "Điều khiển chung", |ui| {
                ui.horizontal(|ui| {
                    if nut(ui, "Mở mic tất cả", XANH_NHAT, chay) {
                        self.khoa(&[], false);
                    }
                    if nut(ui, "Tắt mic tất cả", CAM, chay) {
                        self.khoa(&[], true);
                    }
                });
                let mut v = match self.keo {
                    Some((u32::MAX, v)) => v,
                    _ => (al_chung * 100.0) as f32,
                };
                ui.label(format!("Âm lượng chung: {v:.0}%"));
                let r = ui.add_enabled(chay, egui::Slider::new(&mut v, 0.0..=200.0).show_value(false));
                if r.dragged() {
                    self.keo = Some((u32::MAX, v));
                }
                if r.drag_stopped() || (r.changed() && !r.dragged()) {
                    self.keo = None;
                    self.dat_am_luong_chung(v as f64 / 100.0);
                }
            });
            nhom(ui, "Micro của server", |ui| {
                let dsm = self.ds_mic.lock().unwrap().clone();
                let ten_chon = |i: usize| -> String {
                    match (&dsm, i) {
                        (_, 0) => format!("Giả lập (sin {TAN_SO_SERVER} Hz)"),
                        (Some(v), i) => format!("{}: {}", i - 1, v[i - 1]),
                        _ => "?".into(),
                    }
                };
                let so = dsm.as_ref().map_or(0, |v| v.len());
                ui.add_enabled_ui(!dang_noi, |ui| {
                    egui::ComboBox::from_id_salt("mic_server").width(240.0).selected_text(ten_chon(self.chon_mic)).show_ui(ui, |ui| {
                        for i in 0..=so {
                            ui.selectable_value(&mut self.chon_mic, i, ten_chon(i));
                        }
                    });
                });
                ui.horizontal(|ui| {
                    if nut(ui, "Start Mic", XANH_LA, chay && !dang_noi) {
                        self.bam_start_mic();
                    }
                    if nut(ui, "Stop Mic", DO_NHAT, dang_noi) {
                        self.bam_stop_mic();
                    }
                });
                ui.horizontal(|ui| {
                    let mut v = (self.mic_vol * 100.0) as f32;
                    ui.label(format!("Mic Volume: {v:.0}%"));
                    if ui.add(egui::Slider::new(&mut v, 0.0..=200.0).show_value(false)).changed() {
                        self.dat_mic_vol(v as f64 / 100.0);
                    }
                });
                thanh_do_muc(ui, if dang_noi { muc_mic } else { f64::NEG_INFINITY }, 240.0);
                ui.label(RichText::new(&self.mo_ta_mic).small());
            });
            nhom(ui, "Nghe", |ui| {
                let mut nghe = dang_nghe;
                if ui.add_enabled(chay, egui::Checkbox::new(&mut nghe, "Phát loa server")).changed() {
                    self.bat_nghe(nghe);
                }
                ui.label("Tổng tiếng các client:");
                thanh_do_muc(ui, muc_nghe, 160.0);
                ui.label(RichText::new(&self.mo_ta_loa).small());
            });
        });
        ui.separator();
        let mau = if self.loi { Color32::DARK_RED } else if chay { Color32::DARK_GREEN } else { Color32::BLACK };
        ui.label(RichText::new(&self.trang_thai).color(mau).strong());
        ui.label(format!("Số client: {}", ds.len()));
        ui.separator();
        // ---- mỗi client một hàng
        egui::Grid::new("ds_khach").striped(true).min_col_width(40.0).show(ui, |ui| {
            for t in ["Mã", "Tên", "Mic", "Mức vào", "", "Độ lợi", "Nghe (client báo)"] {
                ui.label(RichText::new(t).strong());
            }
            ui.end_row();
            for k in &ds {
                ui.label(format!("#{}", k.ma));
                ui.label(RichText::new(&k.ten).strong());
                let mau = if k.mic == "mở" { Color32::DARK_GREEN } else { Color32::DARK_RED };
                ui.label(RichText::new(k.mic).color(mau));
                let cs = self.cs_muc.entry(k.ma).or_insert(0.0);
                thanh_do_muc(ui, lam_muot(cs, k.muc), 170.0);
                if k.khoa {
                    if nut(ui, "Gỡ khóa", XANH_LA, true) {
                        self.khoa(&[k.ma], false);
                    }
                } else if nut(ui, "Khóa", DO_NHAT, true) {
                    self.khoa(&[k.ma], true);
                }
                let mut v = match self.keo {
                    Some((m, v)) if m == k.ma => v,
                    _ => (k.do_loi * 100.0) as f32,
                };
                ui.horizontal(|ui| {
                    let r = ui.add(egui::Slider::new(&mut v, 0.0..=200.0).show_value(false));
                    ui.label(format!("{v:.0}%"));
                    if r.dragged() {
                        self.keo = Some((k.ma, v));
                    }
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        self.keo = None;
                        self.dat_do_loi(k.ma, v as f64 / 100.0);
                    }
                });
                ui.label(format!("{:.0}%", k.nghe * 100.0));
                ui.end_row();
            }
        });
        ui.separator();
        ui.label(RichText::new("Nhật ký sự kiện").strong());
        egui::ScrollArea::vertical().stick_to_bottom(true).max_height(self.cao_nhat_ky).auto_shrink([false, false]).show(ui, |ui| {
            for d in self.nhat_ky.lock().unwrap().iter() {
                ui.label(RichText::new(d).monospace().size(11.0));
            }
        });
    }
}

/// Nhật ký của server giao diện: giữ 300 dòng gần nhất để hiện trong khung cuộn, đồng thời in ra stdout.
pub fn ghi_nhat_ky(nk: &Mutex<VecDeque<String>>, text: &str) {
    println!("[server] {text}");
    let mut v = nk.lock().unwrap();
    v.push_back(text.to_string());
    while v.len() > 300 {
        v.pop_front();
    }
}

// ---------------------------------------------------------------- Cửa sổ riêng
struct CuaSoServer(ServerApp);

impl eframe::App for CuaSoServer {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| self.0.ui(ui));
        ctx.request_repaint_after(Duration::from_millis(100)); // thanh đo mức ~10 lần/giây
    }
}

/// server_gui [ip] [cổng_tcp] [cổng_udp] – mặc định nghe 127.0.0.1 (chỉ máy này).
pub fn run(args: &[String]) {
    let ip = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let tcp: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(41620);
    let udp: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(41621);
    let app = ServerApp::new(&ip, tcp, udp);
    let opt = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1150.0, 640.0]).with_title("Bảng điều khiển âm thanh – server"), ..Default::default() };
    eframe::run_native(
        "Audio Server",
        opt,
        Box::new(|cc| {
            cai_phong_chu(&cc.egui_ctx);
            dat_kieu(&cc.egui_ctx);
            Ok(Box::new(CuaSoServer(app)))
        }),
    )
    .unwrap_or_else(|e| eprintln!("Không mở được cửa sổ: {e}"));
    std::process::exit(0);
}
