//! Client hội thoại video có giao diện (egui) – "Camera Client".
//!
//! Bố cục: nhóm Kết nối server (IP, cổng, tên, Kết nối/Ngắt kết nối); nhóm Camera (chọn camera thật
//! qua ffmpeg hoặc "Camera giả lập", START/CANCEL = tự bật/tắt camera); khung xem trước camera của
//! mình; khung "Lưới nhận từ server" (fps nhận, độ phân giải nhận 1/1, 1/2); thanh trạng thái.
//!
//! Luồng:
//!   - luồng giao diện: vẽ, nhận thao tác, chuyển khung RGB thành texture;
//!   - luồng kết nối: connect + "vao" + chờ "chao" (không làm đơ cửa sổ);
//!   - luồng đọc TCP: camera / nhan / su_kien / tu_choi -> trạng thái chung;
//!   - luồng nhận UDP: ghép mảnh thành khung lưới, giải nén, giữ khung mới nhất;
//!   - luồng gửi (nhịp 5 ms): lấy khung từ camera (giả lập hoặc ffmpeg), theo fps server cho phép:
//!     thu nhỏ, nén zlib, chia mảnh ≤ 1200 byte, gửi UDP – đúng đường gửi của client dòng lệnh;
//!   - (camera thật) luồng đọc stdout của tiến trình ffmpeg (camera_ffmpeg.rs).
//! Luồng nền chỉ sửa `ChiaSe` trong Mutex rồi request_repaint(); chỉ luồng giao diện vẽ.
//!
//! Lịch sự/bảo mật: kết nối xong client gửi ngay "tu_tat_camera"; camera chỉ mở khi bấm START.

use crate::camera_ffmpeg::{self, CameraFfmpeg};
use crate::chung::*;
use crate::gui_chung::*;
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const FPS_CAMERA: u32 = 15; // tốc độ thu tối đa của camera (server thường giới hạn thấp hơn)

pub enum NguonCamera {
    GiaLap(CameraTongHop),
    Ffmpeg(CameraFfmpeg),
}

pub struct ChiaSe {
    phien: u64,
    pub trang_thai: String,
    pub loi: bool,
    pub ket_noi: bool,
    pub ma: u16,
    tcp: Option<TcpStream>,
    pub cam_mo: bool, // theo server (quy tắc 1)
    pub ly_do: String,
    pub fps_toi_da: u32,
    pub chia: usize,
    pub nhan_fps: u32,
    pub nhan_chia: usize,
    pub bat: bool, // người dùng đã bấm START
    nguon: Option<NguonCamera>,
    xem_truoc: Option<Vec<u8>>, // khung 160×120 mới của camera mình (chờ giao diện lấy)
    luoi: Option<(usize, usize, Vec<u8>)>, // khung lưới mới nhận (chờ giao diện lấy)
    pub co_luoi: (usize, usize),
    ghep: GhepKhung,
    pub fps_gui: f64,
    pub kbps_gui: f64,
    pub fps_nhan: f64,
    pub kbps_nhan: f64,
    pub manh_mat: u64,
    pub khung_gui: u64,
    dem: [u64; 4], // trong giây hiện tại: khung gửi, byte gửi, khung nhận, byte nhận
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
    cs: Arc<Mutex<ChiaSe>>,
    ds_cam: Arc<Mutex<Option<Vec<String>>>>,
    chon_cam: usize, // 0 = giả lập
    pub mo_ta_cam: String,
    tex_xem: Option<egui::TextureHandle>,
    tex_luoi: Option<egui::TextureHandle>,
    ctx: Option<egui::Context>,
    pub co_xem: f32, // hệ số phóng khung xem trước
    keo_fps: Option<u32>,
}

fn do_camera_nen(ds: &Arc<Mutex<Option<Vec<String>>>>, ctx: Option<egui::Context>) {
    *ds.lock().unwrap() = None;
    let ds = Arc::clone(ds);
    thread::spawn(move || {
        let v = camera_ffmpeg::liet_ke();
        *ds.lock().unwrap() = Some(v);
        if let Some(c) = ctx {
            c.request_repaint();
        }
    });
}

impl KhachApp {
    pub fn new(ten: &str, ip: &str, cong: u16) -> KhachApp {
        let app = KhachApp {
            ten: ten.into(),
            ip: ip.into(),
            cong: cong.to_string(),
            cs: Arc::new(Mutex::new(ChiaSe {
                phien: 0,
                trang_thai: "Chưa kết nối".into(),
                loi: false,
                ket_noi: false,
                ma: 0,
                tcp: None,
                cam_mo: false,
                ly_do: "-".into(),
                fps_toi_da: 10,
                chia: 1,
                nhan_fps: 10,
                nhan_chia: 1,
                bat: false,
                nguon: None,
                xem_truoc: None,
                luoi: None,
                co_luoi: (0, 0),
                ghep: GhepKhung::new(),
                fps_gui: 0.0,
                kbps_gui: 0.0,
                fps_nhan: 0.0,
                kbps_nhan: 0.0,
                manh_mat: 0,
                khung_gui: 0,
                dem: [0; 4],
            })),
            ds_cam: Arc::new(Mutex::new(None)),
            chon_cam: 0,
            mo_ta_cam: "chưa bật".into(),
            tex_xem: None,
            tex_luoi: None,
            ctx: None,
            co_xem: 1.5,
            keo_fps: None,
        };
        do_camera_nen(&app.ds_cam, None);
        app
    }

    pub fn trang_thai(&self) -> std::sync::MutexGuard<'_, ChiaSe> {
        self.cs.lock().unwrap()
    }

    /// Chọn camera theo tên trong danh sách (kịch bản demo dùng); false nếu chưa có.
    pub fn chon_camera(&mut self, ten: &str) -> bool {
        let ds = self.ds_cam.lock().unwrap().clone().unwrap_or_default();
        match ds.iter().position(|t| t == ten) {
            Some(i) => {
                self.chon_cam = i + 1;
                true
            }
            None => false,
        }
    }

    // ------------------------------------------------------------ Thao tác
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
                }
            }
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    pub fn bam_ngat(&mut self) {
        let mut c = self.cs.lock().unwrap();
        c.phien += 1;
        if let Some(t) = c.tcp.take() {
            let _ = t.shutdown(Shutdown::Both);
        }
        c.ket_noi = false;
        c.cam_mo = false;
        c.trang_thai = "Đã ngắt kết nối".into();
        c.loi = false;
    }

    /// START: mở camera (thật qua ffmpeg, lỗi thì giả lập) và xin server bật camera.
    pub fn bam_start(&mut self) {
        let ds = self.ds_cam.lock().unwrap().clone().unwrap_or_default();
        let ma = self.cs.lock().unwrap().ma;
        let mut nguon = NguonCamera::GiaLap(CameraTongHop::new(&self.ten, ma.max(1)));
        self.mo_ta_cam = "GIẢ LẬP – khung tổng hợp".into();
        if self.chon_cam > 0 {
            let ten = &ds[self.chon_cam - 1];
            match CameraFfmpeg::mo(ten, FPS_CAMERA) {
                Ok(c) => {
                    nguon = NguonCamera::Ffmpeg(c);
                    self.mo_ta_cam = format!("THẬT (ffmpeg) – {ten}");
                }
                Err(e) => self.mo_ta_cam = format!("GIẢ LẬP – không chạy được ffmpeg: {e}"),
            }
        }
        let mut c = self.cs.lock().unwrap();
        c.nguon = Some(nguon);
        c.bat = true;
        c.gui(json!({"loai": "tu_bat_camera"})); // tự bật: chờ server cho phép
    }

    /// CANCEL: tự tắt camera – có hiệu lực ngay; đóng tiến trình ffmpeg.
    pub fn bam_cancel(&mut self) {
        self.mo_ta_cam = "đã tắt".into();
        let mut c = self.cs.lock().unwrap();
        c.bat = false;
        c.nguon = None; // drop CameraFfmpeg = kill ffmpeg
        c.cam_mo = false;
        c.gui(json!({"loai": "tu_tat_camera"}));
    }

    pub fn dat_nhan_fps(&mut self, n: u32) {
        self.cs.lock().unwrap().gui(json!({"loai": "nhan", "fps": n}));
    }

    pub fn dat_nhan_chia(&mut self, d: usize) {
        self.cs.lock().unwrap().gui(json!({"loai": "nhan", "chia": d}));
    }

    // ------------------------------------------------------------ Vẽ
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if self.ctx.is_none() {
            self.ctx = Some(ui.ctx().clone());
        }
        // lấy khung mới (nếu có) ra khỏi trạng thái chung, cập nhật texture
        let (xem, luoi) = {
            let mut c = self.cs.lock().unwrap();
            (c.xem_truoc.take(), c.luoi.take())
        };
        if let Some(x) = xem {
            let a = anh_egui(RONG, CAO, &x);
            match &mut self.tex_xem {
                Some(t) => t.set(a, egui::TextureOptions::NEAREST),
                None => self.tex_xem = Some(ui.ctx().load_texture(format!("xem_{}", self.ten), a, egui::TextureOptions::NEAREST)),
            }
        }
        if let Some((w, h, x)) = luoi {
            let a = anh_egui(w, h, &x);
            match &mut self.tex_luoi {
                Some(t) => t.set(a, egui::TextureOptions::NEAREST),
                None => self.tex_luoi = Some(ui.ctx().load_texture(format!("luoi_{}", self.ten), a, egui::TextureOptions::NEAREST)),
            }
        }
        let (ket_noi, bat, nhan_fps, nhan_chia) = {
            let c = self.cs.lock().unwrap();
            (c.ket_noi, c.bat, c.nhan_fps, c.nhan_chia)
        };
        ui.horizontal_top(|ui| {
            nhom(ui, "Kết nối server", |ui| {
                egui::Grid::new(("kn", self.ten.clone())).show(ui, |ui| {
                    ui.label("Server IP:");
                    ui.add_enabled_ui(!ket_noi, |ui| ui.add_sized([110.0, 20.0], egui::TextEdit::singleline(&mut self.ip)));
                    ui.end_row();
                    ui.label("Port:");
                    ui.add_enabled_ui(!ket_noi, |ui| ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut self.cong)));
                    ui.end_row();
                    ui.label("Tên:");
                    ui.add_enabled_ui(!ket_noi, |ui| ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut self.ten)));
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if nut(ui, "Kết nối", XANH_LA, !ket_noi) {
                        self.bam_ket_noi();
                    }
                    if nut(ui, "Ngắt kết nối", DO_NHAT, ket_noi) {
                        self.bam_ngat();
                    }
                });
            });
            nhom(ui, "Camera", |ui| {
                let ds = self.ds_cam.lock().unwrap().clone();
                let ten_chon = |i: usize| -> String {
                    match (&ds, i) {
                        (_, 0) => "Camera giả lập".into(),
                        (Some(v), i) => v[i - 1].clone(),
                        _ => "?".into(),
                    }
                };
                ui.label("Chọn camera:");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!bat, |ui| {
                        egui::ComboBox::from_id_salt(("cam", self.ten.clone())).width(190.0).selected_text(ten_chon(self.chon_cam)).show_ui(ui, |ui| {
                            for i in 0..=ds.as_ref().map_or(0, |v| v.len()) {
                                ui.selectable_value(&mut self.chon_cam, i, ten_chon(i));
                            }
                        });
                    });
                    if ds.is_none() {
                        ui.label("(đang dò…)");
                    } else if ui.add_enabled(!bat, egui::Button::new("↻")).on_hover_text("Dò lại camera").clicked() {
                        do_camera_nen(&self.ds_cam, self.ctx.clone());
                    }
                });
                ui.horizontal(|ui| {
                    if nut(ui, "START", XANH_LA, !bat) {
                        self.bam_start();
                    }
                    if nut(ui, "CANCEL", DO_NHAT, bat) {
                        self.bam_cancel();
                    }
                });
                ui.label(RichText::new(&self.mo_ta_cam).small());
            });
        });
        let co = self.co_xem;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Camera của tôi").strong());
                let kt = egui::vec2(RONG as f32 * co, CAO as f32 * co);
                match (&self.tex_xem, bat) {
                    (Some(t), true) => {
                        ui.add(egui::Image::new(t).fit_to_exact_size(kt));
                    }
                    _ => {
                        let (r, _) = ui.allocate_exact_size(kt, egui::Sense::hover());
                        ui.painter().rect_filled(r, 0.0, Color32::from_gray(60));
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "camera tắt", egui::FontId::proportional(14.0), Color32::WHITE);
                    }
                }
            });
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Lưới nhận từ server").strong());
                    let mut d = nhan_chia;
                    if ui.add_enabled(ket_noi, egui::RadioButton::new(d == 1, "1/1")).clicked() {
                        d = 1;
                    }
                    if ui.add_enabled(ket_noi, egui::RadioButton::new(d == 2, "1/2")).clicked() {
                        d = 2;
                    }
                    if d != nhan_chia {
                        self.dat_nhan_chia(d);
                    }
                });
                ui.horizontal(|ui| {
                    let mut f = self.keo_fps.unwrap_or(nhan_fps);
                    ui.label("fps nhận:");
                    let r = ui.add_enabled(ket_noi, egui::Slider::new(&mut f, 1..=15));
                    if r.dragged() {
                        self.keo_fps = Some(f);
                    }
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        self.keo_fps = None;
                        self.dat_nhan_fps(f);
                    }
                });
                let kt = egui::vec2(320.0, 240.0);
                match &self.tex_luoi {
                    Some(t) if ket_noi => {
                        ui.add(egui::Image::new(t).max_size(kt).maintain_aspect_ratio(true).fit_to_exact_size(kt));
                    }
                    _ => {
                        let (r, _) = ui.allocate_exact_size(kt, egui::Sense::hover());
                        ui.painter().rect_filled(r, 0.0, Color32::from_gray(30));
                    }
                }
            });
        });
        let c = self.cs.lock().unwrap();
        let dang_gui = c.bat && c.cam_mo && c.ket_noi;
        let chu = if !c.ket_noi || c.loi {
            c.trang_thai.clone()
        } else if dang_gui {
            "Đang truyền dữ liệu camera…".to_string()
        } else {
            format!("{} – camera {}", c.trang_thai, c.ly_do)
        };
        let mau = if c.loi { Color32::DARK_RED } else if dang_gui { Color32::DARK_GREEN } else { Color32::BLACK };
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Trạng thái: {chu}")).color(mau).background_color(Color32::WHITE));
        });
        ui.label(RichText::new(format!(
            "Gửi {:.0} fps, {:.0} kbit/s (tối đa {} fps, 1/{}) | nhận {:.0} fps, {:.0} kbit/s, lưới {}x{}, mảnh mất {}",
            c.fps_gui, c.kbps_gui, c.fps_toi_da, c.chia, c.fps_nhan, c.kbps_nhan, c.co_luoi.0, c.co_luoi.1, c.manh_mat
        ))
        .small());
    }
}

// ---------------------------------------------------------------- Luồng nền
fn ket_noi(cs: &Arc<Mutex<ChiaSe>>, phien: u64, ten: &str, host: &str, cong: u16, ctx: Option<egui::Context>) -> std::io::Result<()> {
    let addr = (host, cong).to_socket_addrs()?.next().ok_or(std::io::ErrorKind::AddrNotAvailable)?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    tcp.set_nodelay(true).ok();
    gui_json(&tcp, &json!({"loai": "vao", "ten": ten}))?;
    // Thời hạn đặt trên đúng bản sao dùng để đọc: trên Windows, mỗi bản sao (try_clone) giữ thời hạn riêng
    let mut doc = BufReader::new(tcp.try_clone()?);
    doc.get_ref().set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut dong = String::new();
    let chao: Value = doc.read_line(&mut dong).ok().and_then(|_| serde_json::from_str(dong.trim_end()).ok()).unwrap_or(Value::Null);
    if chao["loai"] != "chao" {
        return Err(std::io::Error::other("server không chào (sai giao thức?)"));
    }
    doc.get_ref().set_read_timeout(None)?;
    let lines = doc.lines();
    let ma = chao["ma"].as_u64().unwrap_or(0) as u16;
    let dich = (host, chao["cong_udp"].as_u64().unwrap_or(0) as u16).to_socket_addrs()?.next().ok_or(std::io::ErrorKind::AddrNotAvailable)?;
    let udp = Arc::new(udp_dem_lon((if dich.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).parse().unwrap())?);
    udp.connect(dich)?;
    udp.set_read_timeout(Some(Duration::from_millis(200)))?;
    {
        let mut c = cs.lock().unwrap();
        if c.phien != phien {
            return Ok(());
        }
        c.tcp = Some(tcp.try_clone()?);
        c.ket_noi = true;
        c.ma = ma;
        c.trang_thai = format!("Đã kết nối – mã {ma}");
        c.ghep = GhepKhung::new();
        if !c.bat {
            c.gui(json!({"loai": "tu_tat_camera"})); // camera tắt cho tới khi người dùng bấm START
        }
        if let Some(NguonCamera::GiaLap(_)) = c.nguon {
            c.nguon = Some(NguonCamera::GiaLap(CameraTongHop::new(ten, ma))); // màu riêng theo mã client
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
                    Some("camera") => {
                        c.cam_mo = tin["mo"].as_bool().unwrap_or(false);
                        c.ly_do = tin["ly_do"].as_str().unwrap_or("").to_string();
                        c.fps_toi_da = tin["fps"].as_u64().unwrap_or(10).max(1) as u32;
                        c.chia = tin["chia"].as_u64().unwrap_or(1).max(1) as usize;
                        if c.cam_mo {
                            c.loi = false;
                            c.trang_thai = format!("Đã kết nối – mã {ma}");
                        }
                    }
                    Some("nhan") => {
                        c.nhan_fps = tin["fps"].as_u64().unwrap_or(10) as u32;
                        c.nhan_chia = tin["chia"].as_u64().unwrap_or(1) as usize;
                    }
                    Some("su_kien") => {}
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
                c.cam_mo = false;
                c.trang_thai = "Mất kết nối với server".into();
                c.loi = true;
            }
        });
    }
    // luồng nhận lưới
    {
        let (cs, udp, ctx) = (Arc::clone(cs), Arc::clone(&udp), ctx.clone());
        thread::spawn(move || {
            let mut buf = [0u8; 2048];
            loop {
                let n = udp.recv(&mut buf);
                if cs.lock().unwrap().phien != phien {
                    return;
                }
                let Ok(n) = n else { continue };
                let Some(m) = tach_header(&buf[..n]).filter(|m| m.loai == LOAI_MANH) else { continue };
                let khung = cs.lock().unwrap().ghep.them(&m, bay_gio());
                if let Some((w, h, du_lieu)) = khung {
                    let Some(anh) = giai_nen(&du_lieu) else { continue };
                    let (w, h) = (w as usize, h as usize);
                    if w * h * 3 != anh.len() {
                        continue;
                    }
                    let mut c = cs.lock().unwrap();
                    c.co_luoi = (w, h);
                    c.luoi = Some((w, h, anh));
                    c.dem[2] += 1;
                    c.dem[3] += du_lieu.len() as u64;
                    c.manh_mat = c.ghep.manh_mat;
                    drop(c);
                    if let Some(x) = &ctx {
                        x.request_repaint();
                    }
                }
            }
        });
    }
    // luồng gửi
    {
        let cs = Arc::clone(cs);
        thread::spawn(move || {
            let _ = udp.send(&goi_giu(ma));
            let (mut so_khung, mut so_tao, mut lan_gui, mut han_xem, mut han_giu) = (0u32, 0u32, -1.0f64, 0.0f64, bay_gio() + 1.0);
            let mut moc_do = bay_gio();
            loop {
                thread::sleep(Duration::from_millis(5));
                let now = bay_gio();
                let mut guard = cs.lock().unwrap();
                let c = &mut *guard;
                if c.phien != phien || !c.ket_noi {
                    return;
                }
                let fps = FPS_CAMERA.min(c.fps_toi_da).max(1) as f64;
                // 1. lấy khung từ camera: giả lập tạo khung theo nhịp 10 fps; ffmpeg đưa khung khi có
                let khung = match c.nguon.as_mut() {
                    Some(NguonCamera::GiaLap(g)) if c.bat && now >= han_xem => {
                        han_xem = (han_xem + 0.1).max(now);
                        so_tao = so_tao.wrapping_add(1);
                        Some(g.lay_khung(so_tao))
                    }
                    Some(NguonCamera::Ffmpeg(f)) if c.bat => {
                        if f.da_dung() {
                            c.trang_thai = "ffmpeg đã dừng (camera bận/sai tên?)".into();
                            c.loi = true;
                        }
                        f.lay()
                    }
                    _ => None,
                };
                if let Some(k) = khung {
                    // 2. gửi nếu server cho phép và đã cách lần gửi trước ≥ 0,9/fps (server bỏ khung
                    //    đến sớm hơn 0,8/fps – quy tắc 6), nên khung camera 15 fps được "tỉa" bớt
                    if c.cam_mo && now - lan_gui >= 0.9 / fps {
                        lan_gui = now;
                        let (anh, w, h) = thu_nho(&k, RONG, CAO, c.chia);
                        let goi_nen = nen(&anh, MUC_NHANH);
                        for g in chia_manh(ma, so_khung, w, h, &goi_nen) {
                            let _ = udp.send(&g);
                        }
                        so_khung = so_khung.wrapping_add(1);
                        c.khung_gui += 1;
                        c.dem[0] += 1;
                        c.dem[1] += goi_nen.len() as u64;
                    }
                    c.xem_truoc = Some(k);
                }
                if !(c.bat && c.cam_mo) && now >= han_giu {
                    let _ = udp.send(&goi_giu(ma)); // giữ kết nối khi tắt camera
                    han_giu = now + 1.0;
                }
                if now - moc_do >= 1.0 {
                    moc_do += 1.0;
                    c.fps_gui = c.dem[0] as f64;
                    c.kbps_gui = c.dem[1] as f64 * 8.0 / 1000.0;
                    c.fps_nhan = c.dem[2] as f64;
                    c.kbps_nhan = c.dem[3] as f64 * 8.0 / 1000.0;
                    c.dem = [0; 4];
                }
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
        ctx.request_repaint_after(Duration::from_millis(66)); // ~15 lần/giây
    }
}

/// khach_gui [tên] [ip] [cổng_tcp]
pub fn run(args: &[String]) {
    let ten = args.first().cloned().unwrap_or_else(|| "A".into());
    let ip = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(41630);
    let app = KhachApp::new(&ten, &ip, cong);
    let opt = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 520.0]).with_title(format!("Camera Client – {ten}")), ..Default::default() };
    eframe::run_native(
        "Camera Client",
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
