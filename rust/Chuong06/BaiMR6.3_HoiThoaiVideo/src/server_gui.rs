//! Server hội thoại video có giao diện (egui) – "Camera Server".
//!
//! Bên trái: lưới 4×4 ô, mỗi client một ô (hình ô do lõi server ghép – đúng ô mà lưới gửi cho client
//! dùng), hiện tên, fps nhận được, chấm đỏ khi camera tắt; bấm vào ô để chọn client.
//! Bên phải: START SERVER / STOP SERVER, danh sách client, điều khiển client đang chọn (Tắt/Bật camera
//! = khóa của server, giới hạn fps gửi 1–15, độ phân giải gửi 1/1, 1/2, 1/4), fps chung, nhật ký.
//! Dưới cùng: thanh trạng thái.
//!
//! Lõi server (nhận kết nối, nhận UDP, ghép lưới và gửi) là đúng phần lõi của bản dòng lệnh
//! (server.rs); giao diện chỉ gọi hàm của lõi và đọc trạng thái phòng để vẽ (~10 lần/giây).

use crate::chung::{CAO, RONG};
use crate::gui_chung::*;
use crate::server::{self, MayChu};
use eframe::egui::{self, Color32, RichText};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Bản sao gọn một client để vẽ.
pub struct OKhach {
    pub ma: u16,
    pub ten: String,
    pub mo: bool,
    pub ly_do: &'static str,
    pub fps_toi_da: u32,
    pub chia: usize,
    pub fps_vao: f64,
    pub kbps_vao: f64,
    pub fps_ra: f64,
    pub kbps_ra: f64,
    o: Option<Vec<u8>>,
}

pub struct ServerApp {
    pub ip: String,
    pub cong_tcp: String,
    pub cong_udp: String,
    pub mc: Option<Arc<MayChu>>,
    nhat_ky: Arc<Mutex<VecDeque<String>>>,
    pub trang_thai: String,
    loi: bool,
    pub chon: Option<u16>,
    tex: HashMap<u16, egui::TextureHandle>,
    pub co_o: f32, // hệ số phóng mỗi ô 160×120
    keo: Option<(u16, u32)>,
}

pub fn ghi_nhat_ky(nk: &Mutex<VecDeque<String>>, text: &str) {
    println!("[server] {text}");
    let mut v = nk.lock().unwrap();
    v.push_back(text.to_string());
    while v.len() > 300 {
        v.pop_front();
    }
}

impl ServerApp {
    pub fn new(ip: &str, cong_tcp: u16, cong_udp: u16) -> ServerApp {
        ServerApp {
            ip: ip.into(),
            cong_tcp: cong_tcp.to_string(),
            cong_udp: cong_udp.to_string(),
            mc: None,
            nhat_ky: Arc::new(Mutex::new(VecDeque::new())),
            trang_thai: "Server chưa chạy".into(),
            loi: false,
            chon: None,
            tex: HashMap::new(),
            co_o: 1.5,
            keo: None,
        }
    }

    // ------------------------------------------------------------ Thao tác
    pub fn bam_start(&mut self) {
        if self.mc.is_some() {
            return;
        }
        let (Ok(tcp), Ok(udp)) = (self.cong_tcp.trim().parse::<u16>(), self.cong_udp.trim().parse::<u16>()) else {
            self.trang_thai = "Cổng không hợp lệ".into();
            self.loi = true;
            return;
        };
        let nk = Arc::clone(&self.nhat_ky);
        match server::khoi_dong(self.ip.trim(), tcp, udp, Box::new(move |t| ghi_nhat_ky(&nk, t))) {
            Ok(mc) => {
                self.trang_thai = format!("Server đã được khởi động tại {}:{tcp} (UDP {udp})", self.ip.trim());
                self.loi = false;
                self.mc = Some(mc);
            }
            Err(e) => {
                self.trang_thai = format!("Không mở được cổng: {e}");
                self.loi = true;
            }
        }
    }

    pub fn bam_stop(&mut self) {
        if let Some(mc) = self.mc.take() {
            server::in_tong_ket(&mc, &mc.phong.lock().unwrap());
            mc.dung_lai();
            ghi_nhat_ky(&self.nhat_ky, "Dừng server");
        }
        self.trang_thai = "Server đã dừng".into();
        self.tex.clear();
    }

    fn voi<R>(&self, f: impl FnOnce(&MayChu, &mut server::Phong) -> R) -> Option<R> {
        let mc = self.mc.as_ref()?;
        let mut p = mc.phong.lock().unwrap();
        Some(f(mc, &mut p))
    }

    fn ten(p: &server::Phong, ma: u16) -> String {
        p.khach.get(&ma).map(|k| k.ten.clone()).unwrap_or_default()
    }

    pub fn tat_camera(&mut self, ma: u16, tat: bool) {
        self.voi(|mc, p| {
            let t = Self::ten(p, ma);
            mc.tat_camera(p, &[ma], tat, &t)
        });
    }

    pub fn dat_fps_toi_da(&mut self, ma: u16, n: u32) {
        self.voi(|mc, p| {
            let t = Self::ten(p, ma);
            mc.dat_fps_toi_da(p, &[ma], n as i64, &n.to_string(), &t)
        });
    }

    pub fn dat_do_phan_giai(&mut self, ma: u16, d: usize) {
        self.voi(|mc, p| {
            let t = Self::ten(p, ma);
            mc.dat_do_phan_giai(p, &[ma], d, &t)
        });
    }

    pub fn dat_fps_chung(&mut self, n: u32) {
        self.voi(|mc, p| mc.dat_fps_chung(p, n as i64));
    }

    /// Đọc trạng thái phòng (dưới khóa) thành bản sao để vẽ.
    pub fn chup_phong(&self) -> (Vec<OKhach>, u32) {
        self.voi(|_, p| {
            let ds = p
                .khach
                .values()
                .map(|k| OKhach {
                    ma: k.ma,
                    ten: k.ten.clone(),
                    mo: k.cam_mo(),
                    ly_do: k.ly_do(),
                    fps_toi_da: k.fps_toi_da,
                    chia: k.chia,
                    fps_vao: k.fps_vao,
                    kbps_vao: k.kbps_vao,
                    fps_ra: k.fps_ra,
                    kbps_ra: k.kbps_ra,
                    o: k.o.clone(),
                })
                .collect();
            (ds, p.fps_chung)
        })
        .unwrap_or((Vec::new(), 10))
    }

    // ------------------------------------------------------------ Vẽ
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let chay = self.mc.is_some();
        let (ds, fps_chung) = self.chup_phong();
        if self.chon.is_none_or(|m| !ds.iter().any(|k| k.ma == m)) {
            self.chon = ds.first().map(|k| k.ma);
        }
        ui.horizontal_top(|ui| {
            // ---- lưới 4×4
            let o = egui::vec2(RONG as f32 * self.co_o, CAO as f32 * self.co_o);
            egui::Grid::new("luoi_cam").spacing([4.0, 4.0]).show(ui, |ui| {
                for i in 0..16 {
                    let (r, phan_hoi) = ui.allocate_exact_size(o, egui::Sense::click());
                    let p = ui.painter();
                    p.rect_filled(r, 0.0, Color32::from_gray(225));
                    p.rect_stroke(r, 0.0, egui::Stroke::new(2.0_f32, Color32::from_gray(140)));
                    if let Some(k) = ds.get(i) {
                        if let Some(anh) = &k.o {
                            let a = anh_egui(RONG, CAO, anh);
                            let t = self.tex.entry(k.ma).or_insert_with(|| ui.ctx().load_texture(format!("o{}", k.ma), a.clone(), egui::TextureOptions::NEAREST));
                            t.set(a, egui::TextureOptions::NEAREST);
                            p.image(t.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                        }
                        let nhan = format!("{} – {:.0} fps", k.ten, k.fps_vao);
                        let vi_tri = r.right_top() + egui::vec2(-24.0, 4.0); // góc phải trên, cạnh chấm đỏ
                        p.text(vi_tri + egui::vec2(1.0, 1.0), egui::Align2::RIGHT_TOP, &nhan, egui::FontId::proportional(13.0), Color32::BLACK);
                        p.text(vi_tri, egui::Align2::RIGHT_TOP, &nhan, egui::FontId::proportional(13.0), Color32::WHITE);
                        if !k.mo {
                            p.circle_filled(r.right_top() + egui::vec2(-12.0, 12.0), 7.0, Color32::RED); // chấm đỏ: camera tắt
                        }
                        if self.chon == Some(k.ma) {
                            p.rect_stroke(r, 0.0, egui::Stroke::new(3.0_f32, Color32::from_rgb(30, 110, 230)));
                        }
                        if phan_hoi.clicked() {
                            self.chon = Some(k.ma);
                        }
                    }
                    if i % 4 == 3 {
                        ui.end_row();
                    }
                }
            });
            // ---- khung phải
            ui.vertical(|ui| {
                ui.set_width(300.0);
                ui.horizontal(|ui| {
                    ui.label("IP:");
                    ui.add_enabled_ui(!chay, |ui| ui.add_sized([100.0, 20.0], egui::TextEdit::singleline(&mut self.ip)));
                    ui.label("TCP/UDP:");
                    ui.add_enabled_ui(!chay, |ui| ui.add_sized([46.0, 20.0], egui::TextEdit::singleline(&mut self.cong_tcp)));
                    ui.add_enabled_ui(!chay, |ui| ui.add_sized([46.0, 20.0], egui::TextEdit::singleline(&mut self.cong_udp)));
                });
                ui.horizontal(|ui| {
                    if nut(ui, "START SERVER", XANH_LA, !chay) {
                        self.bam_start();
                    }
                    if nut(ui, "STOP SERVER", DO_NHAT, chay) {
                        self.bam_stop();
                    }
                });
                ui.separator();
                ui.label(RichText::new("Danh sách client").strong());
                for k in &ds {
                    let chu = format!("#{} {} – cam {} – vào {:.0} fps {:.0} kbit/s – ra {:.0} fps", k.ma, k.ten, k.ly_do, k.fps_vao, k.kbps_vao, k.fps_ra);
                    if ui.selectable_label(self.chon == Some(k.ma), RichText::new(chu).size(12.0)).clicked() {
                        self.chon = Some(k.ma);
                    }
                }
                ui.separator();
                if let Some(k) = self.chon.and_then(|m| ds.iter().find(|k| k.ma == m)) {
                    ui.label(RichText::new(format!("Client đang chọn: {}", k.ten)).strong());
                    let ma = k.ma;
                    ui.horizontal(|ui| {
                        if nut(ui, "Tắt camera", DO_NHAT, true) {
                            self.tat_camera(ma, true);
                        }
                        if nut(ui, "Bật camera", XANH_LA, true) {
                            self.tat_camera(ma, false);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("fps gửi tối đa:");
                        let mut f = match self.keo {
                            Some((m, f)) if m == ma => f,
                            _ => k.fps_toi_da,
                        };
                        let r = ui.add(egui::Slider::new(&mut f, 1..=15));
                        if r.dragged() {
                            self.keo = Some((ma, f));
                        }
                        if r.drag_stopped() || (r.changed() && !r.dragged()) {
                            self.keo = None;
                            self.dat_fps_toi_da(ma, f);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Độ phân giải gửi:");
                        for d in [1usize, 2, 4] {
                            if ui.radio(k.chia == d, format!("1/{d}")).clicked() && k.chia != d {
                                self.dat_do_phan_giai(ma, d);
                            }
                        }
                    });
                } else {
                    ui.label("(chưa có client)");
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("fps chung (lưới gửi đi):");
                    let mut f = match self.keo {
                        Some((u16::MAX, f)) => f,
                        _ => fps_chung,
                    };
                    let r = ui.add_enabled(chay, egui::Slider::new(&mut f, 1..=15));
                    if r.dragged() {
                        self.keo = Some((u16::MAX, f));
                    }
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        self.keo = None;
                        self.dat_fps_chung(f);
                    }
                });
                ui.separator();
                ui.label(RichText::new("Nhật ký").strong());
                egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).max_height(o.y * 4.0 - 300.0).show(ui, |ui| {
                    for d in self.nhat_ky.lock().unwrap().iter() {
                        ui.label(RichText::new(d).monospace().size(10.5));
                    }
                });
            });
        });
        let mau = if self.loi { Color32::DARK_RED } else { Color32::BLACK };
        ui.label(RichText::new(format!("Trạng thái: {} | Số client: {}", self.trang_thai, ds.len())).color(mau));
    }
}

// ---------------------------------------------------------------- Cửa sổ riêng
struct CuaSoServer(ServerApp);

impl eframe::App for CuaSoServer {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| self.0.ui(ui));
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

/// server_gui [ip] [cổng_tcp] [cổng_udp] – mặc định 127.0.0.1.
pub fn run(args: &[String]) {
    let ip = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let tcp: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(41630);
    let udp: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(41631);
    let app = ServerApp::new(&ip, tcp, udp);
    let opt = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1330.0, 800.0]).with_title("Camera Server"), ..Default::default() };
    eframe::run_native(
        "Camera Server",
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
