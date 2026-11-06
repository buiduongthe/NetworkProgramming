//! Kịch bản minh họa giao diện: một cửa sổ chứa "Camera Server" và 3 cửa sổ con "Camera Client"
//! (A, B, C) trong CÙNG một tiến trình; kịch bản tự bấm nút theo thời gian (gọi đúng các hàm mà nút
//! gọi), chờ khung hình chạy, in fps/kbit/s, chụp cửa sổ thành PNG.
//!
//! demo_gui <cổng_tcp> <cổng_udp> [ảnh.png]
//!
//! A, B dùng "Camera giả lập"; C dùng "Mẫu thử ffmpeg (testsrc)" nếu máy có ffmpeg – tức là đi đúng
//! đường camera thật (tiến trình ffmpeg -> khung rgb24 160×120 -> nén -> chia mảnh), chỉ khác nguồn.

use crate::camera_ffmpeg::MAU_THU;
use crate::gui_chung::*;
use crate::khach_gui::KhachApp;
use crate::server_gui::ServerApp;
use eframe::egui;
use std::time::{Duration, Instant};

struct DemoApp {
    server: ServerApp,
    khach: Vec<KhachApp>,
    t0: Instant,
    bat_dau: bool,
    buoc: usize,
    anh: String,
    da_xin_chup: bool,
}

const KICH_BAN: &[(f64, &str)] = &[
    (0.5, "Server: bấm START SERVER"),
    (1.0, "A, B, C: bấm Kết nối"),
    (2.0, "A, B: chọn \"Camera giả lập\", C: chọn \"Mẫu thử ffmpeg (testsrc)\"; cả ba bấm START"),
    (3.0, "Server: chọn ô B, bấm Tắt camera"),
    (3.4, "B: bấm CANCEL"),
    (3.7, "B: bấm START (xin bật lại khi server đang tắt)"),
    (4.0, "Server: C độ phân giải gửi 1/2; A fps gửi tối đa 5"),
    (4.4, "A: chọn nhận lưới 1/2, fps nhận 8"),
    (7.0, "Đo số liệu và chụp cửa sổ"),
];

impl DemoApp {
    fn ma(&self, i: usize) -> u16 {
        self.khach[i].trang_thai().ma
    }

    fn thuc_hien(&mut self, i: usize) {
        println!("[demo] t={:.1}s  bước {}: {}", self.t0.elapsed().as_secs_f64(), i + 1, KICH_BAN[i].1);
        match i {
            0 => {
                self.server.bam_start();
                println!("[demo]   {}", self.server.trang_thai);
            }
            1 => {
                for k in self.khach.iter_mut() {
                    k.bam_ket_noi();
                    std::thread::sleep(Duration::from_millis(100)); // vào phòng theo thứ tự A, B, C (mã 1, 2, 3)
                }
            }
            2 => {
                if !self.khach[2].chon_camera(MAU_THU) {
                    println!("[demo]   C: không có ffmpeg – dùng camera giả lập");
                }
                for k in self.khach.iter_mut() {
                    k.bam_start();
                    println!("[demo]   {}: camera {}", k.ten, k.mo_ta_cam);
                }
            }
            3 => {
                let b = self.ma(1);
                self.server.chon = Some(b);
                self.server.tat_camera(b, true);
            }
            4 => self.khach[1].bam_cancel(),
            5 => self.khach[1].bam_start(),
            6 => {
                let (c, a) = (self.ma(2), self.ma(0));
                self.server.dat_do_phan_giai(c, 2);
                self.server.dat_fps_toi_da(a, 5);
            }
            7 => {
                self.khach[0].dat_nhan_chia(2);
                self.khach[0].dat_nhan_fps(8);
            }
            _ => self.in_so_lieu(),
        }
    }

    fn in_so_lieu(&self) {
        println!("[demo] Số liệu client (mỗi giây gần nhất):");
        println!("[demo]   tên camera        gửi fps kbit/s  giới hạn  nhận fps kbit/s  lưới      trạng thái");
        for k in &self.khach {
            let c = k.trang_thai();
            println!(
                "[demo]   {:<3} {:<13} {:>7.0} {:>7.0}  {:>2}fps 1/{}  {:>8.0} {:>7.0}  {:>3}x{:<4}  {}",
                k.ten,
                c.ly_do,
                c.fps_gui,
                c.kbps_gui,
                c.fps_toi_da,
                c.chia,
                c.fps_nhan,
                c.kbps_nhan,
                c.co_luoi.0,
                c.co_luoi.1,
                c.trang_thai
            );
        }
        let (ds, fc) = self.server.chup_phong();
        println!("[demo] Số liệu server: fps chung {fc}, số client {}", ds.len());
        for k in &ds {
            println!(
                "[demo]   #{} {:<3} camera {:<13} giới hạn {:>2} fps 1/{}  vào {:>3.0} fps {:>5.0} kbit/s  ra (lưới) {:>3.0} fps {:>5.0} kbit/s",
                k.ma, k.ten, k.ly_do, k.fps_toi_da, k.chia, k.fps_vao, k.kbps_vao, k.fps_ra, k.kbps_ra
            );
        }
    }
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        vua_man_hinh(ctx, 1520.0, 1110.0);
        if !self.bat_dau {
            self.t0 = Instant::now(); // tính giờ từ khung hình đầu tiên
            self.bat_dau = true;
        }
        let t = self.t0.elapsed().as_secs_f64();
        while self.buoc < KICH_BAN.len() && t >= KICH_BAN[self.buoc].0 {
            self.thuc_hien(self.buoc);
            self.buoc += 1;
        }
        if self.buoc == KICH_BAN.len() && !self.da_xin_chup {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            self.da_xin_chup = true;
        }
        let anh = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(anh) = anh {
            match ghi_png_anh(&self.anh, &anh) {
                Ok(()) => println!("[demo] Đã chụp cửa sổ {}x{} -> {}", anh.size[0], anh.size[1], self.anh),
                Err(e) => println!("[demo] Không ghi được {}: {e}", self.anh),
            }
            for k in self.khach.iter_mut() {
                k.bam_cancel(); // dừng tiến trình ffmpeg
            }
            self.server.bam_stop();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Camera Server");
            self.server.ui(ui);
        });
        let vi_tri = [[1000.0, 8.0], [8.0, 600.0], [540.0, 600.0]];
        for (i, k) in self.khach.iter_mut().enumerate() {
            egui::Window::new(format!("Camera Client – {}", k.ten))
                .id(egui::Id::new(("khach", i)))
                .fixed_pos(vi_tri[i])
                .fixed_size([510.0, 420.0])
                .collapsible(false)
                .frame(egui::Frame::window(&ctx.style()).fill(NEN))
                .show(ctx, |ui| k.ui(ui));
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

pub fn run(args: &[String]) {
    let tcp: u16 = args.first().and_then(|p| p.parse().ok()).unwrap_or(44635);
    let udp: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(44636);
    let anh = args.get(2).cloned().unwrap_or_else(|| "ket_qua_gui.png".into());
    let mut server = ServerApp::new("127.0.0.1", tcp, udp);
    server.co_o = 1.0;
    let mut khach: Vec<KhachApp> = ["A", "B", "C"].iter().map(|t| KhachApp::new(t, "127.0.0.1", tcp)).collect();
    for k in khach.iter_mut() {
        k.co_xem = 1.0;
    }
    println!("[demo] Một tiến trình: Camera Server + 3 Camera Client, cổng TCP {tcp}, UDP {udp}");
    let app = DemoApp { server, khach, t0: Instant::now(), bat_dau: false, buoc: 0, anh, da_xin_chup: false };
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1520.0, 1110.0]).with_title("Demo giao diện MR6.3 – server và 3 client"),
        ..Default::default()
    };
    eframe::run_native(
        "Demo MR6.3",
        opt,
        Box::new(|cc| {
            cai_phong_chu(&cc.egui_ctx);
            dat_kieu(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
    .unwrap_or_else(|e| println!("[demo] Không mở được cửa sổ: {e}"));
    println!("[demo] Kết thúc");
    std::process::exit(0);
}

/// Màn hình thật (Windows) thấp hơn khung demo dựng cho Xvfb 2600×1700: thu nhỏ toàn bộ giao diện (zoom) một lần
/// cho vừa màn hình, để ảnh chụp đủ các cửa sổ con. Màn hình đủ lớn (Xvfb) thì giữ nguyên.
fn vua_man_hinh(ctx: &egui::Context, rong: f32, cao: f32) {
    let id = egui::Id::new("da_vua_man_hinh");
    if ctx.data(|d| d.get_temp::<bool>(id)).is_some() {
        return;
    }
    let Some(m) = ctx.input(|i| i.viewport().monitor_size) else { return };
    let z0 = ctx.zoom_factor();
    let z = ((m.y * z0 - 90.0) / cao).min((m.x * z0 - 20.0) / rong).min(1.0);
    ctx.data_mut(|d| d.insert_temp(id, true));
    if z < 0.999 {
        ctx.set_zoom_factor(z);
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(0.0, 0.0)));
    }
}
