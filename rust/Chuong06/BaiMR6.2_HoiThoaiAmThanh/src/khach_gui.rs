//! Client hội thoại âm thanh có giao diện (egui) – "Audio Client".
//!
//! Bố cục: nhóm Kết nối (IP, cổng, tên, Connect/Disconnect); nhóm Nhận âm thanh (âm lượng nghe
//! 0–200 %, Start/Stop Audio = phát/không phát ra loa, "Server Vol: xx%", thanh đo mức tiếng nhận);
//! nhóm Micro (chọn thiết bị thật hoặc "Giả lập (sin 440 Hz)", Start/Stop Mic, Mic Volume, thanh đo
//! mức); dòng trạng thái.
//!
//! Luồng:
//!   - luồng giao diện: vẽ, nhận thao tác; mở/đóng luồng cpal (micro, loa) vì cpal::Stream không Send;
//!   - luồng kết nối: connect + "vao" + chờ "chao" (không làm đơ cửa sổ khi server chậm/không có);
//!   - luồng đọc TCP: thông điệp mic / do_loi / su_kien / tu_choi -> sửa trạng thái chung;
//!   - luồng nhận UDP: gói RTP luồng trộn -> bộ đệm jitter;
//!   - luồng nhịp 20 ms: lấy 320 mẫu từ nguồn micro, gửi nếu mic mở; lấy một khung từ bộ đệm jitter,
//!     nhân âm lượng nghe, đo mức, đẩy ra loa nếu đang Start Audio.
//! Mọi luồng nền chỉ sửa `ChiaSe` (trong Mutex) rồi gọi ctx.request_repaint(); chỉ luồng giao diện
//! đọc `ChiaSe` để vẽ.
//!
//! Lịch sự/bảo mật: kết nối xong client gửi ngay "tu_tat_mic" – micro (thật hay giả lập) chỉ mở khi
//! người dùng bấm Start Mic.

use crate::chung::*;
use crate::gui_chung::*;
use crate::thiet_bi::{self, HangMau, NguonMic};
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::net::{Shutdown, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Trạng thái dùng chung giữa luồng giao diện và các luồng nền của client.
pub struct ChiaSe {
    phien: u64, // tăng mỗi lần kết nối: luồng của phiên cũ thấy khác thì tự thoát
    pub trang_thai: String,
    pub loi: bool,
    pub ket_noi: bool,
    pub ma: u32,
    tcp: Option<TcpStream>,
    pub mic_mo: bool, // theo server (quy tắc 1)
    pub ly_do_mic: String,
    pub server_vol: f64,
    pub su_kien: VecDeque<String>,
    pub thu: bool, // người dùng đã bấm Start Mic
    nguon: Option<NguonMic>,
    pub mic_vol: f64,
    pub nghe_vol: f64,
    loa: Option<HangMau>, // Some = đang Start Audio
    bo_dem: BoDemJitter,
    cs_mic: f64, // công suất làm mượt để vẽ thanh đo
    cs_nhan: f64,
    pub muc_mic: f64,
    pub muc_nhan: f64,
    pub goi_gui: u64,
    pub goi_nhan: u64,
}

impl ChiaSe {
    fn gui(&self, obj: Value) {
        if let Some(t) = &self.tcp {
            let _ = gui_json(t, &obj);
        }
    }
}

pub struct KhachApp {
    pub ten: String,
    pub ip: String,
    pub cong: String,
    tan_so: f64, // tần số nguồn giả lập
    cs: Arc<Mutex<ChiaSe>>,
    ds_mic: Arc<Mutex<Option<Vec<String>>>>, // None = đang dò thiết bị
    chon_mic: usize,                         // 0 = giả lập, i = ds_mic[i-1]
    luong_mic: Option<cpal::Stream>,
    luong_loa: Option<cpal::Stream>,
    pub mo_ta_mic: String,
    pub mo_ta_loa: String,
    keo_nghe: Option<f32>,
    ctx: Option<egui::Context>,
}

/// Dò micro thật ở luồng nền (không chặn giao diện).
pub fn do_micro_nen(ds: &Arc<Mutex<Option<Vec<String>>>>, ctx: Option<egui::Context>) {
    *ds.lock().unwrap() = None;
    let ds = Arc::clone(ds);
    thread::spawn(move || {
        let v = thiet_bi::ds_micro();
        *ds.lock().unwrap() = Some(v);
        if let Some(c) = ctx {
            c.request_repaint();
        }
    });
}

impl KhachApp {
    pub fn new(ten: &str, ip: &str, cong: u16, tan_so: f64) -> KhachApp {
        let app = KhachApp {
            ten: ten.into(),
            ip: ip.into(),
            cong: cong.to_string(),
            tan_so,
            cs: Arc::new(Mutex::new(ChiaSe {
                phien: 0,
                trang_thai: "Chưa kết nối".into(),
                loi: false,
                ket_noi: false,
                ma: 0,
                tcp: None,
                mic_mo: false,
                ly_do_mic: "-".into(),
                server_vol: 1.0,
                su_kien: VecDeque::new(),
                thu: false,
                nguon: None,
                mic_vol: 1.0,
                nghe_vol: 1.0,
                loa: None,
                bo_dem: BoDemJitter::new(3),
                cs_mic: 0.0,
                cs_nhan: 0.0,
                muc_mic: f64::NEG_INFINITY,
                muc_nhan: f64::NEG_INFINITY,
                goi_gui: 0,
                goi_nhan: 0,
            })),
            ds_mic: Arc::new(Mutex::new(None)),
            chon_mic: 0,
            luong_mic: None,
            luong_loa: None,
            mo_ta_mic: "chưa bật".into(),
            mo_ta_loa: "chưa bật".into(),
            keo_nghe: None,
            ctx: None,
        };
        do_micro_nen(&app.ds_mic, None);
        app
    }

    pub fn trang_thai(&self) -> std::sync::MutexGuard<'_, ChiaSe> {
        self.cs.lock().unwrap()
    }

    // ------------------------------------------------------------ Thao tác (nút bấm và kịch bản demo)
    pub fn bam_ket_noi(&mut self) {
        let cong: u16 = self.cong.trim().parse().unwrap_or(0);
        let phien = {
            let mut c = self.cs.lock().unwrap();
            if c.ket_noi {
                return;
            }
            c.phien += 1;
            c.trang_thai = format!("Đang kết nối {}:{cong}…", self.ip.trim());
            c.loi = false;
            c.phien
        };
        let (cs, ten, host, ctx) = (Arc::clone(&self.cs), self.ten.trim().to_string(), self.ip.trim().to_string(), self.ctx.clone());
        thread::spawn(move || {
            if let Err(e) = ket_noi(&cs, phien, &ten, &host, cong, ctx.clone()) {
                let mut c = cs.lock().unwrap();
                if c.phien == phien {
                    c.trang_thai = format!("Không kết nối được: {e}");
                    c.loi = true;
                    c.ket_noi = false;
                }
            }
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    pub fn bam_ngat(&mut self) {
        let mut c = self.cs.lock().unwrap();
        c.phien += 1; // các luồng của phiên này tự thoát
        if let Some(t) = c.tcp.take() {
            let _ = t.shutdown(Shutdown::Both);
        }
        c.ket_noi = false;
        c.mic_mo = false;
        c.ly_do_mic = "-".into();
        c.trang_thai = "Đã ngắt kết nối".into();
        c.loi = false;
    }

    pub fn bam_start_mic(&mut self) {
        let ds = self.ds_mic.lock().unwrap().clone().unwrap_or_default();
        let mut nguon = NguonMic::gia_lap(self.tan_so);
        self.luong_mic = None;
        self.mo_ta_mic = format!("GIẢ LẬP – sin {} Hz", self.tan_so);
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
        let mut c = self.cs.lock().unwrap();
        c.nguon = Some(nguon);
        c.thu = true;
        c.gui(json!({"loai": "tu_bat_mic"})); // tự bật: chờ server cho phép (quy tắc 2)
    }

    pub fn bam_stop_mic(&mut self) {
        self.luong_mic = None; // drop Stream = đóng micro thật
        self.mo_ta_mic = "đã tắt".into();
        let mut c = self.cs.lock().unwrap();
        c.thu = false;
        c.nguon = None;
        c.mic_mo = false; // tự tắt có hiệu lực ngay, không cần server đồng ý
        c.gui(json!({"loai": "tu_tat_mic"}));
    }

    pub fn bam_start_audio(&mut self) {
        let hang = thiet_bi::hang_moi();
        match thiet_bi::mo_loa(Arc::clone(&hang)) {
            Ok((s, mt)) => {
                self.luong_loa = Some(s);
                self.mo_ta_loa = format!("THẬT – {mt}");
            }
            Err(e) => {
                self.luong_loa = None;
                self.mo_ta_loa = format!("không có loa ({e}) – chỉ đo mức");
            }
        }
        self.cs.lock().unwrap().loa = Some(hang);
    }

    pub fn bam_stop_audio(&mut self) {
        self.luong_loa = None;
        self.mo_ta_loa = "đã tắt".into();
        self.cs.lock().unwrap().loa = None;
    }

    /// Âm lượng nghe: áp dụng tại client (quy tắc 5), báo server để hiển thị.
    pub fn dat_am_luong_nghe(&mut self, g: f64) {
        let mut c = self.cs.lock().unwrap();
        c.nghe_vol = g;
        c.gui(json!({"loai": "am_luong_nghe", "gia_tri": g}));
    }

    pub fn dat_mic_vol(&mut self, g: f64) {
        self.cs.lock().unwrap().mic_vol = g;
    }

    // ------------------------------------------------------------ Vẽ
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if self.ctx.is_none() {
            self.ctx = Some(ui.ctx().clone());
        }
        let (ket_noi, thu, dang_phat) = {
            let c = self.cs.lock().unwrap();
            (c.ket_noi, c.thu, c.loa.is_some())
        };
        nhom(ui, "Kết nối", |ui| {
            ui.horizontal(|ui| {
                ui.label("IP:");
                ui.add_enabled(!ket_noi, egui::TextEdit::singleline(&mut self.ip).desired_width(110.0));
                ui.label("Cổng:");
                ui.add_enabled(!ket_noi, egui::TextEdit::singleline(&mut self.cong).desired_width(50.0));
                ui.label("Tên:");
                ui.add_enabled(!ket_noi, egui::TextEdit::singleline(&mut self.ten).desired_width(60.0));
            });
            ui.horizontal(|ui| {
                if nut(ui, "Connect", XANH_LA, !ket_noi) {
                    self.bam_ket_noi();
                }
                if nut(ui, "Disconnect", DO_NHAT, ket_noi) {
                    self.bam_ngat();
                }
            });
        });
        nhom(ui, "Nhận âm thanh", |ui| {
            let (nghe, sv, muc) = {
                let c = self.cs.lock().unwrap();
                (c.nghe_vol, c.server_vol, c.muc_nhan)
            };
            ui.horizontal(|ui| {
                let mut v = self.keo_nghe.unwrap_or((nghe * 100.0) as f32);
                ui.label(format!("Volume: {v:.0}%"));
                let r = ui.add(egui::Slider::new(&mut v, 0.0..=200.0).show_value(false));
                if r.dragged() {
                    self.keo_nghe = Some(v);
                }
                if r.drag_stopped() || (r.changed() && !r.dragged()) {
                    self.keo_nghe = None;
                    self.dat_am_luong_nghe(v as f64 / 100.0);
                }
                ui.label(RichText::new(format!("Server Vol: {:.0}%", sv * 100.0)).strong());
            });
            ui.horizontal(|ui| {
                if nut(ui, "Start Audio", XANH_LA, !dang_phat) {
                    self.bam_start_audio();
                }
                if nut(ui, "Stop Audio", DO_NHAT, dang_phat) {
                    self.bam_stop_audio();
                }
                thanh_do_muc(ui, muc, 170.0);
            });
            ui.label(RichText::new(format!("Loa: {}", self.mo_ta_loa)).small());
        });
        nhom(ui, "Micro", |ui| {
            let ds = self.ds_mic.lock().unwrap().clone();
            ui.horizontal(|ui| {
                let ten_chon = |i: usize| -> String {
                    match (&ds, i) {
                        (_, 0) => format!("Giả lập (sin {} Hz)", self.tan_so),
                        (Some(v), i) => format!("{}: {}", i - 1, v[i - 1]),
                        _ => "?".into(),
                    }
                };
                let so = ds.as_ref().map_or(0, |v| v.len());
                ui.add_enabled_ui(!thu, |ui| {
                    egui::ComboBox::from_id_salt(("mic_khach", self.ten.clone()))
                        .width(260.0)
                        .selected_text(ten_chon(self.chon_mic))
                        .show_ui(ui, |ui| {
                            for i in 0..=so {
                                ui.selectable_value(&mut self.chon_mic, i, ten_chon(i));
                            }
                        });
                });
                if ds.is_none() {
                    ui.label("(đang dò thiết bị…)");
                } else if ui.add_enabled(!thu, egui::Button::new("↻")).on_hover_text("Dò lại thiết bị").clicked() {
                    do_micro_nen(&self.ds_mic, self.ctx.clone());
                }
            });
            let (mv, muc, mo, ly_do) = {
                let c = self.cs.lock().unwrap();
                (c.mic_vol, c.muc_mic, c.mic_mo, c.ly_do_mic.clone())
            };
            ui.horizontal(|ui| {
                if nut(ui, "Start Mic", XANH_LA, !thu) {
                    self.bam_start_mic();
                }
                if nut(ui, "Stop Mic", DO_NHAT, thu) {
                    self.bam_stop_mic();
                }
                thanh_do_muc(ui, muc, 170.0);
            });
            ui.horizontal(|ui| {
                let mut v = (mv * 100.0) as f32;
                ui.label(format!("Mic Volume: {v:.0}%"));
                if ui.add(egui::Slider::new(&mut v, 0.0..=200.0).show_value(false)).changed() {
                    self.dat_mic_vol(v as f64 / 100.0);
                }
                let mau = if mo { Color32::DARK_GREEN } else { Color32::DARK_RED };
                ui.label(RichText::new(format!("Mic: {ly_do}")).color(mau).strong());
            });
            ui.label(RichText::new(format!("Nguồn: {}", self.mo_ta_mic)).small());
        });
        let c = self.cs.lock().unwrap();
        let mau = if c.loi { Color32::DARK_RED } else if c.ket_noi { Color32::DARK_GREEN } else { Color32::BLACK };
        ui.label(RichText::new(&c.trang_thai).color(mau).background_color(Color32::WHITE));
        if let Some(sk) = c.su_kien.back() {
            ui.label(RichText::new(format!("Sự kiện: {sk}")).small());
        }
    }
}

// ---------------------------------------------------------------- Luồng nền
/// Chạy trên luồng kết nối: connect, vào phòng, chờ "chao", rồi khởi động các luồng của phiên.
fn ket_noi(cs: &Arc<Mutex<ChiaSe>>, phien: u64, ten: &str, host: &str, cong: u16, ctx: Option<egui::Context>) -> std::io::Result<()> {
    let addr = (host, cong).to_socket_addrs()?.next().ok_or(std::io::ErrorKind::AddrNotAvailable)?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    tcp.set_nodelay(true).ok();
    gui_json(&tcp, &json!({"loai": "vao", "ten": ten}))?;
    tcp.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut lines = BufReader::new(tcp.try_clone()?).lines();
    let chao: Value = lines.next().and_then(|l| l.ok()).and_then(|l| serde_json::from_str(&l).ok()).unwrap_or(Value::Null);
    if chao["loai"] != "chao" {
        return Err(std::io::Error::other("server không chào (sai giao thức?)"));
    }
    tcp.set_read_timeout(None)?;
    let ma = chao["ma"].as_u64().unwrap_or(0) as u32;
    let dich = (host, chao["cong_udp"].as_u64().unwrap_or(0) as u16).to_socket_addrs()?.next().ok_or(std::io::ErrorKind::AddrNotAvailable)?;
    let udp = Arc::new(UdpSocket::bind(if dich.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" })?);
    udp.connect(dich)?; // chỉ nhận gói từ đúng địa chỉ server
    udp.set_read_timeout(Some(Duration::from_millis(200)))?;
    {
        let mut c = cs.lock().unwrap();
        if c.phien != phien {
            return Ok(()); // người dùng đã bấm Disconnect trong lúc chờ
        }
        c.tcp = Some(tcp.try_clone()?);
        c.ket_noi = true;
        c.ma = ma;
        c.trang_thai = format!("Đã kết nối – mã {ma}");
        c.bo_dem = BoDemJitter::new(3);
        c.goi_gui = 0;
        c.goi_nhan = 0;
        if !c.thu {
            c.gui(json!({"loai": "tu_tat_mic"})); // micro tắt cho tới khi người dùng bấm Start Mic
        }
        if c.nghe_vol != 1.0 {
            c.gui(json!({"loai": "am_luong_nghe", "gia_tri": c.nghe_vol}));
        }
    }
    // luồng đọc kênh điều khiển
    {
        let (cs, ctx) = (Arc::clone(cs), ctx.clone());
        thread::spawn(move || {
            for line in lines.map_while(Result::ok) {
                let Ok(tin) = serde_json::from_str::<Value>(&line) else { break };
                let mut c = cs.lock().unwrap();
                if c.phien != phien {
                    return;
                }
                match tin["loai"].as_str() {
                    Some("mic") => {
                        c.mic_mo = tin["mo"].as_bool().unwrap_or(false);
                        c.ly_do_mic = tin["ly_do"].as_str().unwrap_or("").to_string();
                        if c.mic_mo {
                            c.loi = false;
                            c.trang_thai = format!("Đã kết nối – mã {ma}");
                        }
                    }
                    Some("do_loi") => c.server_vol = tin["gia_tri"].as_f64().unwrap_or(1.0),
                    Some("su_kien") => {
                        c.su_kien.push_back(tin["noi_dung"].as_str().unwrap_or("").to_string());
                        if c.su_kien.len() > 20 {
                            c.su_kien.pop_front();
                        }
                    }
                    Some("tu_choi") => {
                        c.trang_thai = format!("Bị từ chối: {}", tin["ly_do"].as_str().unwrap_or(""));
                        c.loi = true;
                    }
                    _ => {}
                }
                drop(c);
                if let Some(x) = &ctx {
                    x.request_repaint();
                }
            }
            let mut c = cs.lock().unwrap();
            if c.phien == phien {
                c.ket_noi = false;
                c.tcp = None;
                c.mic_mo = false;
                c.trang_thai = "Mất kết nối với server".into();
                c.loi = true;
            }
        });
    }
    // luồng nhận âm thanh trộn từ server
    {
        let (cs, udp) = (Arc::clone(cs), Arc::clone(&udp));
        let t0 = Instant::now();
        thread::spawn(move || {
            let mut buf = [0u8; 2048];
            loop {
                let n = udp.recv(&mut buf);
                let mut c = cs.lock().unwrap();
                if c.phien != phien {
                    return;
                }
                let Ok(n) = n else { continue };
                if let Some(r) = tach_rtp(&buf[..n]).filter(|r| r.pt == PT_AM_THANH) {
                    c.bo_dem.nhan_goi(r.seq, r.ts, r.payload, t0.elapsed().as_secs_f64());
                    c.goi_nhan += 1;
                }
            }
        });
    }
    // luồng nhịp 20 ms: thu – gửi, nhận – phát
    {
        let cs = Arc::clone(cs);
        thread::spawn(move || {
            let (mut seq, mut ts, mut n) = (0u32, 0u32, 0u64);
            let giu = dong_goi_rtp(PT_GIU_KET_NOI, 0, 0, ma, &[]);
            let _ = udp.send(&giu); // đăng ký địa chỉ UDP với server ngay từ đầu
            let mut ke_tiep = Instant::now();
            loop {
                n += 1;
                {
                    let mut guard = cs.lock().unwrap();
                    let c = &mut *guard;
                    if c.phien != phien || !c.ket_noi {
                        return;
                    }
                    // 1. Thu: lấy 20 ms từ nguồn (kể cả khi bị khóa, để thanh đo mức vẫn chạy)
                    let khung = c.nguon.as_mut().map(|g| thiet_bi::nhan_he_so(&g.lay_khung(), c.mic_vol));
                    let db = khung.as_ref().map_or(f64::NEG_INFINITY, |k| muc_dbfs(k));
                    c.muc_mic = lam_muot(&mut c.cs_mic, db);
                    match khung {
                        Some(k) if c.mic_mo && c.thu => {
                            let _ = udp.send(&dong_goi_rtp(PT_AM_THANH, seq, ts, ma, &pcm_sang_byte(&k)));
                            seq = seq.wrapping_add(1);
                            c.goi_gui += 1;
                        }
                        _ if n % 50 == 0 => {
                            let _ = udp.send(&giu); // mic tắt: mỗi giây một gói giữ kết nối
                        }
                        _ => {}
                    }
                    ts = ts.wrapping_add(MAU_MOI_KHUNG as u32);
                    // 2. Nhận: một khung từ bộ đệm jitter (thiếu -> khoảng lặng), nhân âm lượng nghe
                    let mau = c.bo_dem.lay().map(|p| byte_sang_pcm(&p)).unwrap_or_else(|| vec![0; MAU_MOI_KHUNG]);
                    let mau = thiet_bi::nhan_he_so(&mau, c.nghe_vol);
                    c.muc_nhan = lam_muot(&mut c.cs_nhan, muc_dbfs(&mau));
                    if let Some(loa) = &c.loa {
                        thiet_bi::day_ra_loa(loa, &mau);
                    }
                }
                ke_tiep += Duration::from_millis(KHUNG_MS);
                thread::sleep(ke_tiep.saturating_duration_since(Instant::now()));
            }
        });
    }
    Ok(())
}

// ---------------------------------------------------------------- Cửa sổ riêng
struct CuaSoKhach(KhachApp);

impl eframe::App for CuaSoKhach {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().frame(egui::Frame::central_panel(&ctx.style()).fill(NEN)).show(ctx, |ui| self.0.ui(ui));
        ctx.request_repaint_after(Duration::from_millis(100)); // thanh đo mức ~10 lần/giây
    }
}

/// khach_gui [tên] [ip] [cổng_tcp] [--tan-so 440]
pub fn run(a: crate::Args) {
    let ten = a.pos.first().cloned().unwrap_or_else(|| "A".into());
    let ip = a.pos.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong: u16 = a.pos.get(2).and_then(|p| p.parse().ok()).unwrap_or(41620);
    let app = KhachApp::new(&ten, &ip, cong, a.f64("tan-so", 440.0));
    let opt = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 470.0]).with_title(format!("Audio Client – {ten}")), ..Default::default() };
    eframe::run_native(
        "Audio Client",
        opt,
        Box::new(|cc| {
            cai_phong_chu(&cc.egui_ctx);
            dat_kieu(&cc.egui_ctx);
            Ok(Box::new(CuaSoKhach(app)))
        }),
    )
    .unwrap_or_else(|e| eprintln!("Không mở được cửa sổ: {e}"));
    std::process::exit(0);
}
