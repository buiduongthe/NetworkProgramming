//! Bài mở rộng MR6.5 – Máy chủ phát trực tiếp có giao diện (egui).
//!
//! Lõi MayChuPhat (may_chu.rs) chạy các luồng mạng; cửa sổ chỉ gọi hàm của lõi và cứ 100 ms đọc
//! trang_thai() để vẽ lại. Server bật `xem_truoc`: tự giải mã MỘT bản luồng video (chỉ để hiện ảnh xem
//! trước, việc chuyển tiếp vẫn là nhân bản nguyên byte – không giải nén).
//!
//! Chạy:  server_gui [host] [cổng_tcp] [cổng_udp]

use crate::chung::{CAO, RONG};
use crate::gui_chung::*;
use crate::may_chu::MayChuPhat;
use eframe::egui::{self, Color32, RichText};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct ServerApp {
    pub ip: String,
    pub cong_tcp: String,
    pub cong_udp: String,
    pub lop: Arc<MayChuPhat>,
    pub bo_dem: bool,
    pub nhat_ky: NhatKy,
    dinh_ky: ThongKeDinhKy,
    tex: Option<egui::TextureHandle>,
    so_da_ve: Option<(u32, u32)>,
    dong_ho: String,
    pub chon: Option<u32>,
    dang_dung: Arc<AtomicBool>,
}

impl ServerApp {
    pub fn new(host: &str, cong_tcp: u16, cong_udp: u16) -> ServerApp {
        let nhat_ky = NhatKy::new("server");
        let lop = MayChuPhat::new(nhat_ky.bao());
        lop.dat_xem_truoc(true);
        ServerApp {
            ip: host.into(),
            cong_tcp: cong_tcp.to_string(),
            cong_udp: cong_udp.to_string(),
            lop,
            bo_dem: true,
            nhat_ky,
            dinh_ky: ThongKeDinhKy::new("server"),
            tex: None,
            so_da_ve: None,
            dong_ho: "Đồng hồ: –".into(),
            chon: None,
            dang_dung: Arc::new(AtomicBool::new(false)),
        }
    }

    // ------------------------------------------------------------ nút
    pub fn bam_start(&mut self) {
        let (tcp, udp) = (self.cong_tcp.trim().parse::<u16>(), self.cong_udp.trim().parse::<u16>());
        let (Ok(tcp), Ok(udp)) = (tcp, udp) else {
            self.nhat_ky.ghi("Không khởi động được: cổng không hợp lệ");
            return;
        };
        self.lop.khoa.lock().unwrap().bo_dem_bat = self.bo_dem;
        if let Err(e) = self.lop.bat_dau(self.ip.trim(), tcp, udp) {
            self.nhat_ky.ghi(&format!("Không khởi động được: {e}"));
        }
    }

    pub fn bam_stop(&mut self) {
        self.dang_dung.store(true, Ordering::SeqCst);
        let (lop, d) = (Arc::clone(&self.lop), Arc::clone(&self.dang_dung));
        std::thread::spawn(move || {
            lop.dung(); // chờ luồng nền dừng: không chặn giao diện
            d.store(false, Ordering::SeqCst);
        });
    }

    pub fn dang_dung(&self) -> bool {
        self.dang_dung.load(Ordering::SeqCst)
    }

    pub fn ngat_chon(&mut self) {
        if let Some(m) = self.chon {
            self.lop.ngat(None, Some(m));
        }
    }

    /// Chọn dòng theo tên (dùng trong kịch bản demo).
    pub fn chon_nguoi_xem(&mut self, ten: &str) {
        if let Some(x) = self.lop.trang_thai().nguoi_xem.iter().find(|x| x.ten == ten) {
            self.chon = Some(x.ma);
        }
    }

    // ------------------------------------------------------------ vẽ
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let chay = self.lop.dang_chay();
        let tt = self.lop.trang_thai();
        if chay {
            self.dinh_ky.in_neu_den_han(|| {
                let ds: String = tt.nguoi_xem.iter().map(|x| format!(" {} {} {:.0}", x.ten, x.nhan, x.kbps)).collect();
                format!(
                    "vào {:.0} kbit/s (tiếng {:.0}, hình {:.0}), ra {:.0}, hệ số {:.2}, {};{ds}",
                    tt.kbps_vao,
                    tt.kbps_vao_am,
                    tt.kbps_vao_hinh,
                    tt.kbps_ra,
                    tt.he_so,
                    if tt.dang_phat { "đang phát" } else { "chưa phát" }
                )
            });
        }
        // ---- hàng trên: kết nối, bộ đệm GOP, chỉ số
        ui.horizontal_top(|ui| {
            nhom(ui, "Kết nối", |ui| {
                ui.set_min_height(92.0);
                ui.horizontal(|ui| {
                    ui.add_sized([34.0, 18.0], egui::Label::new("IP:"));
                    o_nhap(ui, &mut self.ip, 150.0, !chay);
                });
                ui.horizontal(|ui| {
                    ui.add_sized([34.0, 18.0], egui::Label::new("TCP:"));
                    o_nhap(ui, &mut self.cong_tcp, 52.0, !chay);
                    ui.label("  UDP:");
                    o_nhap(ui, &mut self.cong_udp, 52.0, !chay);
                });
                ui.horizontal(|ui| {
                    if nut(ui, "START SERVER", XANH_LA, !chay && !self.dang_dung(), 128.0) {
                        self.bam_start();
                    }
                    if nut(ui, "STOP SERVER", DO, chay, 128.0) {
                        self.bam_stop();
                    }
                });
            });
            nhom(ui, "Bộ đệm GOP (GOP cache)", |ui| {
                ui.set_min_height(92.0);
                ui.set_width(250.0);
                if ui.checkbox(&mut self.bo_dem, "Bật – người vào muộn nhận ngay\nkhung I mới nhất + các P sau nó").changed() {
                    self.lop.dat_bo_dem(self.bo_dem);
                }
                let giu = if !chay {
                    "Đang giữ: –".to_string()
                } else if tt.bo_dem_bat && tt.bo_dem_so_i.is_some() {
                    format!("Đang giữ: I #{} và các P sau: {} gói, {:.0} KB", tt.bo_dem_so_i.unwrap(), tt.bo_dem_goi, tt.bo_dem_byte as f64 / 1024.0)
                } else if tt.bo_dem_bat {
                    "Đang giữ: – (chờ khung I)".into()
                } else {
                    "Tắt: người vào muộn chờ khung I".into()
                };
                ui.label(giu);
            });
            nhom(ui, "Chỉ số (mỗi giây)", |ui| {
                ui.set_min_height(92.0);
                ui.set_width(ui.available_width().max(260.0) - 12.0);
                let dam = |s: String| RichText::new(s).strong().size(13.0);
                if chay {
                    ui.label(dam(format!("Vào: {:.0} kbit/s (tiếng {:.0}, hình {:.0})", tt.kbps_vao, tt.kbps_vao_am, tt.kbps_vao_hinh)));
                    ui.label(dam(format!("Ra: {:.0} kbit/s tới {} người xem", tt.kbps_ra, tt.nguoi_xem.len())));
                    ui.label(dam(format!("Hệ số nhân bản (ra/vào): {:.2}", tt.he_so)).color(Color32::from_rgb(0x00, 0x3a, 0x80)));
                    ui.label(format!("PLI chuyển: {} (bỏ {} do lọc 500 ms)", tt.pli_chuyen, tt.pli_bo));
                } else {
                    ui.label(dam("Vào: –".into()));
                    ui.label(dam("Ra: –".into()));
                    ui.label(dam("Hệ số nhân bản (ra/vào): –".into()).color(Color32::from_rgb(0x00, 0x3a, 0x80)));
                    ui.label("PLI chuyển: 0 (bỏ 0)");
                }
            });
        });
        // ---- hàng giữa: xem trước, bảng người xem
        match &tt.anh {
            Some((w, h, rgb, ts, so)) if chay => {
                if self.so_da_ve != Some((*ts, *so)) {
                    dat_texture(ui.ctx(), &mut self.tex, "sv_xem_truoc", *w, *h, rgb);
                    self.dong_ho = format!("Đồng hồ phương tiện: t={:7.3} s, khung #{so}", *ts as f64 / 1000.0);
                    self.so_da_ve = Some((*ts, *so));
                }
            }
            None => {
                self.tex = None;
                self.so_da_ve = None;
            }
            _ => {}
        }
        if self.chon.is_some_and(|m| !tt.nguoi_xem.iter().any(|x| x.ma == m)) {
            self.chon = None;
        }
        ui.horizontal_top(|ui| {
            nhom(ui, "Xem trước luồng (server tự giải mã)", |ui| {
                o_hinh(ui, self.tex.as_ref(), 2.0 * RONG as f32, 2.0 * CAO as f32, "Chưa có luồng");
                ui.label(RichText::new(&self.dong_ho).monospace().size(12.0));
            });
            nhom(ui, "Người xem", |ui| {
                ui.set_width(ui.available_width() - 12.0);
                egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, Color32::from_gray(0x90))).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_height(210.0);
                    egui::Grid::new("sv_bang").striped(true).min_col_width(40.0).spacing([10.0, 3.0]).show(ui, |ui| {
                        for (t, w) in [("Tên", 60.0), ("IP:cổng UDP", 120.0), ("Nhận", 40.0), ("Gói gửi", 55.0), ("kbit/s", 45.0), ("Khung I đầu", 120.0)] {
                            ui.add_sized([w, 18.0], egui::Label::new(RichText::new(t).strong().color(Color32::BLACK)));
                        }
                        ui.end_row();
                        for x in &tt.nguoi_xem {
                            let dau = x.khung_dau_ms.map_or("–".to_string(), |ms| format!("{ms:.0} ms ({})", x.nguon_dau));
                            if ui.selectable_label(self.chon == Some(x.ma), &x.ten).clicked() {
                                self.chon = Some(x.ma);
                            }
                            ui.label(&x.udp);
                            ui.label(x.nhan);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(x.goi_ra.to_string()));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(format!("{:.0}", x.kbps)));
                            ui.label(dau);
                            ui.end_row();
                        }
                    });
                });
                ui.horizontal(|ui| {
                    if nut(ui, "Ngắt người chọn", CAM, true, 140.0) {
                        self.ngat_chon();
                    }
                    let phat = format!("  Máy phát: {}{}", tt.ten_phat.clone().unwrap_or("–".into()), if tt.dang_phat { " (đang phát)" } else { "" });
                    ui.label(if chay { phat } else { "  Máy phát: –".into() });
                });
            });
        });
        nhom(ui, "Nhật ký", |ui| {
            self.nhat_ky.ve(ui, 7, ui.available_width() - 12.0);
        });
        let trang = if chay {
            format!(
                "Trạng thái: server chạy tại {} (UDP {}) – {}, {} người xem",
                self.lop.dia_chi.lock().unwrap(),
                self.lop.cong_udp(),
                if tt.dang_phat { "đang phát" } else { "chưa phát" },
                tt.nguoi_xem.len()
            )
        } else {
            "Trạng thái: server chưa chạy".to_string()
        };
        thanh_trang_thai(ui, &trang);
    }

    pub fn dong(&mut self) {
        self.lop.dung();
    }
}

// ---------------------------------------------------------------- Cửa sổ riêng
struct CuaSoServer(ServerApp);

impl eframe::App for CuaSoServer {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().frame(egui::Frame::central_panel(&ctx.style()).fill(NEN)).show(ctx, |ui| self.0.ui(ui));
        ctx.request_repaint_after(Duration::from_millis(100));
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.0.dong();
    }
}

/// server_gui [host] [cổng_tcp] [cổng_udp] – mặc định 127.0.0.1 44652 44653.
pub fn run(args: &[String]) {
    IN_RA.store(true, Ordering::SeqCst); // chạy riêng: nhật ký cũng ra dòng lệnh
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let tcp: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(44652);
    let udp: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(tcp + 1);
    let app = ServerApp::new(&host, tcp, udp);
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([930.0, 640.0]).with_title("Máy chủ phát trực tiếp (SFU) – MR6.5"),
        ..Default::default()
    };
    eframe::run_native(
        "Máy chủ phát trực tiếp",
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
