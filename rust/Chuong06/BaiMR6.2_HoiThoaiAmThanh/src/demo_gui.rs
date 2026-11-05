//! Kịch bản minh họa giao diện: một cửa sổ chứa bảng điều khiển server và 3 cửa sổ con client
//! (A, B, C) chạy CÙNG một tiến trình; kịch bản tự "bấm nút" theo thời gian (gọi đúng các hàm mà
//! nút bấm gọi), chờ thanh đo mức chạy, in số liệu đo được so với lý thuyết, chụp cửa sổ thành PNG.
//!
//! demo_gui <cổng_tcp> <cổng_udp> [ảnh.png]
//!
//! Mọi micro/loa ở đây là GIẢ LẬP (máy chạy kịch bản có thể không có card âm thanh): A sin 440 Hz,
//! B 620 Hz, C 880 Hz, server 330 Hz – mỗi nguồn biên độ 0,25 toàn thang (-15,1 dBFS).

use crate::gui_chung::*;
use crate::khach_gui::KhachApp;
use crate::server_gui::ServerApp;
use eframe::egui;
use std::time::{Duration, Instant};

struct DemoApp {
    server: ServerApp,
    khach: Vec<KhachApp>,
    t0: Instant,
    buoc: usize,
    anh: String,
    da_xin_chup: bool,
    bat_dau: bool,
}

/// Mốc thời gian (giây) và mô tả từng bước của kịch bản.
const KICH_BAN: &[(f64, &str)] = &[
    (0.5, "Server: bấm Start Server"),
    (1.0, "A, B, C: bấm Connect"),
    (1.8, "A, B, C: chọn \"Giả lập\", bấm Start Mic và Start Audio"),
    (2.6, "Server: Start Mic (giả lập 330 Hz), Mic Volume 50 %, bật Nghe"),
    (3.6, "Server: bấm Khóa ở hàng của B"),
    (4.0, "B: bấm Stop Mic"),
    (4.3, "B: bấm Start Mic (xin bật lại khi đang bị khóa)"),
    (4.6, "Server: kéo độ lợi C = 150 %; A: kéo âm lượng nghe 50 %"),
    (6.8, "Đo mức và chụp cửa sổ"),
];

/// Mức dBFS lý thuyết của tổng các sóng sin khác tần số: RMS² = Σ aᵢ²/2.
fn ly_thuyet(bien_do: &[f64]) -> f64 {
    10.0 * (bien_do.iter().map(|a| a * a / 2.0).sum::<f64>()).log10()
}

impl DemoApp {
    fn thuc_hien(&mut self, i: usize) {
        println!("[demo] t={:.1}s  bước {}: {}", self.t0.elapsed().as_secs_f64(), i + 1, KICH_BAN[i].1);
        match i {
            0 => {
                self.server.bam_start_server();
                println!("[demo]   trạng thái server: {}", self.server.trang_thai);
            }
            1 => {
                for k in self.khach.iter_mut() {
                    k.bam_ket_noi();
                    std::thread::sleep(Duration::from_millis(100)); // vào phòng theo thứ tự A, B, C (mã 1, 2, 3)
                }
            }
            2 => {
                for k in self.khach.iter_mut() {
                    k.bam_start_mic();
                    k.bam_start_audio();
                    println!("[demo]   {}: mic {}; loa {}", k.ten, k.mo_ta_mic, k.mo_ta_loa);
                }
            }
            3 => {
                self.server.dat_mic_vol(0.5);
                self.server.bam_start_mic();
                self.server.bat_nghe(true);
                println!("[demo]   server: mic {}; loa {}", self.server.mo_ta_mic, self.server.mo_ta_loa);
            }
            4 => {
                let ma_b = self.khach[1].trang_thai().ma;
                self.server.khoa(&[ma_b], true);
            }
            5 => self.khach[1].bam_stop_mic(),
            6 => self.khach[1].bam_start_mic(),
            7 => {
                let ma_c = self.khach[2].trang_thai().ma;
                self.server.dat_do_loi(ma_c, 1.5);
                self.khach[0].dat_am_luong_nghe(0.5);
            }
            _ => self.in_so_lieu(),
        }
    }

    fn in_so_lieu(&self) {
        // Biên độ (phần toàn thang) mỗi nguồn sau độ lợi: A 0,25; B bị khóa; C 0,25 × 1,5; server 0,25 × 0,5
        let (a, c, s) = (0.25, 0.375, 0.125);
        let lt = [ly_thuyet(&[c, s]) + 20.0 * 0.5f64.log10(), ly_thuyet(&[a, c, s]), ly_thuyet(&[a, s])];
        println!("[demo] Số liệu client (mức nhận đã nhân âm lượng nghe; lý thuyết = tổng công suất các âm):");
        println!("[demo]   tên mic             Server Vol  mức mic   mức nhận  lý thuyết  gói gửi gói nhận  trạng thái");
        for (k, lt) in self.khach.iter().zip(lt) {
            let c = k.trang_thai();
            println!(
                "[demo]   {:<3} {:<15} {:>8.0}%  {:>8}  {:>8}  {:>8.1}  {:>7} {:>8}  {}",
                k.ten,
                c.ly_do_mic,
                c.server_vol * 100.0,
                chu(c.muc_mic),
                chu(c.muc_nhan),
                lt,
                c.goi_gui,
                c.goi_nhan,
                c.trang_thai
            );
        }
        let (ds, al, muc_mic, muc_nghe, _, _) = self.server.chup_phong();
        println!("[demo] Số liệu server: âm lượng chung {:.0}%, số client {}", al * 100.0, ds.len());
        for k in &ds {
            println!("[demo]   #{} {:<3} mic {:<16} mức vào {:>7}  độ lợi {:>4.0}%  nghe (client báo) {:>4.0}%", k.ma, k.ten, k.mic, chu(k.muc), k.do_loi * 100.0, k.nghe * 100.0);
        }
        println!("[demo]   mic server (330 Hz × 50 %): {} dBFS (lý thuyết {:.1})", chu(muc_mic), ly_thuyet(&[s]));
        println!("[demo]   server nghe (A + C×1,5, không gồm tiếng server): {} dBFS (lý thuyết {:.1})", chu(muc_nghe), ly_thuyet(&[a, c]));
    }
}

fn chu(db: f64) -> String {
    if db.is_finite() && db > -90.0 {
        format!("{db:.1}")
    } else {
        "---".into()
    }
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if self.buoc == 0 && !self.bat_dau {
            self.t0 = Instant::now(); // tính giờ từ khung hình đầu tiên (cửa sổ đã hiện)
            self.bat_dau = true;
        }
        let t = self.t0.elapsed().as_secs_f64();
        while self.buoc < KICH_BAN.len() && t >= KICH_BAN[self.buoc].0 {
            self.thuc_hien(self.buoc);
            self.buoc += 1;
        }
        if self.buoc == KICH_BAN.len() && !self.da_xin_chup {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot); // kết quả về ở Event::Screenshot
            self.da_xin_chup = true;
        }
        let anh = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(anh) = anh {
            match ghi_png(&self.anh, &anh) {
                Ok(()) => println!("[demo] Đã chụp cửa sổ {}x{} -> {}", anh.size[0], anh.size[1], self.anh),
                Err(e) => println!("[demo] Không ghi được {}: {e}", self.anh),
            }
            self.server.bam_stop_server();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // ---- server ở nửa trên, ba client ở nửa dưới (mỗi client một cửa sổ con)
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Bảng điều khiển âm thanh – server");
            self.server.ui(ui);
        });
        for (i, k) in self.khach.iter_mut().enumerate() {
            egui::Window::new(format!("Audio Client – {}", k.ten))
                .id(egui::Id::new(("khach", i)))
                .fixed_pos([8.0 + i as f32 * 540.0, 548.0])
                .fixed_size([510.0, 330.0])
                .collapsible(false)
                .frame(egui::Frame::window(&ctx.style()).fill(NEN))
                .show(ctx, |ui| k.ui(ui));
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

pub fn run(args: &[String]) {
    let tcp: u16 = args.first().and_then(|p| p.parse().ok()).unwrap_or(44625);
    let udp: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(44626);
    let anh = args.get(2).cloned().unwrap_or_else(|| "ket_qua_gui.png".into());
    let mut server = ServerApp::new("127.0.0.1", tcp, udp);
    server.cao_nhat_ky = 80.0;
    let khach = vec![KhachApp::new("A", "127.0.0.1", tcp, 440.0), KhachApp::new("B", "127.0.0.1", tcp, 620.0), KhachApp::new("C", "127.0.0.1", tcp, 880.0)];
    println!("[demo] Một tiến trình: server giao diện + 3 client giao diện, cổng TCP {tcp}, UDP {udp}; micro/loa giả lập");
    let app = DemoApp { server, khach, t0: Instant::now(), buoc: 0, anh, da_xin_chup: false, bat_dau: false };
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1630.0, 900.0]).with_title("Demo giao diện MR6.2 – server và 3 client"),
        ..Default::default()
    };
    eframe::run_native(
        "Demo MR6.2",
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
