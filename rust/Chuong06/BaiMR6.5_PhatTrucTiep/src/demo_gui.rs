//! Bài mở rộng MR6.5 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).
//!
//! Một tiến trình, một cửa sổ egui chứa bốn "cửa sổ con": máy chủ, máy phát P, người xem A và B.
//! P dùng camera giả lập "bảng clapper" và micro giả lập; A vào ngay, đồng bộ tiếng-hình BẬT; B bỏ chọn
//! đồng bộ rồi vào muộn ~3,5 s. Kịch bản gọi đúng các hàm mà nút gọi, theo từng bước (chờ một số giây
//! hoặc chờ tới khi điều kiện đúng – vòng vẽ của egui vẫn chạy trong lúc chờ), in số liệu `[demo] …`,
//! chụp 4 cửa sổ thành ket_qua_gui.png. Có ffmpeg: thêm một lượt "Mẫu thử ffmpeg (testsrc)" và một lượt
//! "Tệp video" (tạo tệp mẫu mp4 có chớp + bíp mỗi giây, phát cả hình lẫn tiếng của tệp; chụp máy phát và
//! người xem A thành ket_qua_gui_tep.png).
//!
//! Chạy:  xvfb-run -a -s "-screen 0 2000x1400x24" mr6_5_phat_truc_tiep demo_gui [host] [cổng_tcp] [cổng_udp] [tệp_ảnh]

use crate::camera::{co_ffmpeg, MAU_THU};
use crate::chung::{bay_gio, chu_ms, ghi_png};
use crate::gui_chung::*;
use crate::phat_gui::PhatApp;
use crate::server_gui::ServerApp;
use crate::tep_video::tao_tep_mau;
use crate::xem_gui::XemApp;
use eframe::egui;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn log(text: &str) {
    println!("[demo] {text}");
}

type DieuKien = fn(&DemoApp) -> bool;

struct DemoApp {
    sv: ServerApp,
    p: PhatApp,
    a: XemApp,
    b: XemApp,
    out: String,
    buoc: usize,
    han: f64,                          // chờ tới giờ này
    dk: Option<(DieuKien, f64)>,       // hoặc chờ điều kiện (kèm hạn 15 s)
    anh: Option<Arc<egui::ColorImage>>, // ảnh chụp cửa sổ vừa nhận
    cho_anh: bool,
    khung: [egui::Rect; 4], // vị trí 4 cửa sổ con (để cắt ảnh)
    ppp: f32,
    bat_dau: bool,
}

fn ten_cs(i: usize) -> &'static str {
    ["Máy chủ phát trực tiếp (SFU) – MR6.5", "Máy phát trực tiếp – MR6.5 (P)", "Người xem trực tiếp – MR6.5 (A)", "Người xem trực tiếp – MR6.5 (B)"][i]
}

impl DemoApp {
    fn cho(&mut self, giay: f64) {
        self.han = bay_gio() + giay;
    }

    fn cho_khi(&mut self, dk: DieuKien) {
        self.dk = Some((dk, bay_gio() + 15.0));
    }

    fn chup(&mut self, ctx: &egui::Context) {
        self.anh = None;
        self.cho_anh = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        self.cho_khi(|d| d.anh.is_some());
    }

    fn vung(&self, ds: &[usize]) -> egui::Rect {
        let mut r = self.khung[ds[0]];
        for &i in &ds[1..] {
            r = r.union(self.khung[i]);
        }
        egui::Rect::from_min_max(((r.min - egui::vec2(4.0, 4.0)) * self.ppp).max(egui::pos2(0.0, 0.0)), (r.max + egui::vec2(4.0, 4.0)) * self.ppp)
    }

    fn in_so_lieu(&self) {
        let t = self.sv.lop.trang_thai();
        log(&format!("   server: vào {:.0}, ra {:.0} kbit/s, hệ số {:.2}", t.kbps_vao, t.kbps_ra, t.he_so));
        for x in &t.nguoi_xem {
            log(&format!(
                "     -> {}: {}, {:.0} kbit/s, I đầu {} ms ({})",
                x.ten,
                x.nhan,
                x.kbps,
                chu_ms(x.khung_dau_ms, false),
                if x.nguon_dau.is_empty() { "–" } else { x.nguon_dau }
            ));
        }
        let tp = self.p.lop.as_ref().unwrap().trang_thai();
        log(&format!("   P: I {} P {}, TB I {:.0} B, P {:.0} B, PLI {}", tp.so_i, tp.so_p, tp.tb_i, tp.tb_p, tp.pli_nhan));
        for (ten, cs) in [("A", &self.a), ("B", &self.b)] {
            let tt = cs.trang_thai_lop().unwrap();
            let g = tt.giai_doan.last().unwrap();
            let lech = if g.n > 0 { Some(g.tong / g.n as f64) } else { None };
            log(&format!("   {ten}: đồng bộ {}, khung đầu {} ms ({})", if tt.dong_bo { "BẬT" } else { "TẮT" }, chu_ms(tt.khung_dau_ms, false), tt.nguon_dau));
            log(&format!("      lệch TB {} ms, trễ {} ms, mất tiếng {}, đứng {}", chu_ms(lech, true), chu_ms(tt.tre_tb, false), tt.mat_am, tt.dung_hinh));
        }
    }

    /// Một bước của kịch bản; mỗi bước tự đặt điều kiện chờ cho bước sau.
    fn buoc(&mut self, ctx: &egui::Context) {
        let n = self.buoc;
        self.buoc += 1;
        match n {
            0 => self.cho(0.5),
            1 => {
                log("1. Server: bấm START SERVER");
                self.sv.bam_start();
                log("2. P: Kết nối; camera giả lập (clapper), micro giả lập");
                self.p.bam_ket_noi();
                self.cho_khi(|d| d.p.da_ket_noi_gui);
            }
            2 => {
                log("   P bấm PHÁT (10 fps, GOP 20)");
                self.p.bam_phat();
                self.cho(1.0);
            }
            3 => {
                log("3. A: Kết nối ngay (đồng bộ tiếng-hình BẬT)");
                self.a.bam_ket_noi();
                self.cho(3.5);
            }
            4 => {
                log("4. B: bỏ chọn 'Đồng bộ', Kết nối (vào muộn 3,5 s)");
                self.b.dong_bo = false;
                self.b.bam_ket_noi();
                self.cho_khi(|d| d.b.trang_thai_lop().is_some_and(|t| t.khung_dau_ms.is_some()));
            }
            5 => self.cho(3.0),
            6 => {
                log("5. Số liệu (bộ đệm GOP bật):");
                self.in_so_lieu();
                log("6. P bấm 'Khung khóa ngay'; A bấm 'Chụp'");
                self.p.bam_khung_khoa();
                self.cho(0.6);
            }
            7 => {
                let duong = self.a.chup().unwrap_or_else(|| "None".into());
                log(&format!("   A đã chụp {duong}"));
                self.cho(0.5);
            }
            8 => {
                log("7. Chụp 4 cửa sổ");
                self.chup(ctx);
            }
            9 => {
                let anh = self.anh.take().unwrap();
                let (w, h, rgb) = rgb_cua_anh(&anh, Some(self.vung(&[0, 1, 2, 3])));
                match ghi_png(&self.out, &rgb, w, h) {
                    Ok(()) => log(&format!("   Đã ghi {} ({} KB)", self.out, std::fs::metadata(&self.out).map_or(0, |m| m.len() / 1024))),
                    Err(e) => log(&format!("   Không ghi được {}: {e}", self.out)),
                }
                log("8. Server: chọn B, bấm 'Ngắt người chọn'");
                self.sv.chon_nguoi_xem("B");
                self.sv.ngat_chon();
                self.cho(0.8);
            }
            10 => {
                log(&format!("   B: {}", self.b.chu_trang_thai()));
                if !co_ffmpeg() {
                    self.buoc = 16;
                    return;
                }
                log("9. P: DỪNG PHÁT, chọn 'Mẫu thử ffmpeg (testsrc)', PHÁT");
                self.p.bam_dung();
                self.cho(0.5);
            }
            11 => {
                self.p.chon_camera(MAU_THU);
                self.p.bam_phat();
                self.cho(4.0);
            }
            12 => {
                let tp = self.p.lop.as_ref().unwrap().trang_thai();
                log(&format!("   P testsrc: I {} P {}, TB I {:.0} B, P {:.0} B", tp.so_i, tp.so_p, tp.tb_i, tp.tb_p));
                log(&format!("   P: {}", tp.camera));
                let ta = self.a.trang_thai_lop().unwrap();
                log(&format!("   A: khung I {}, P {}, lệch {} ms", ta.khung_i, ta.khung_p, chu_ms(ta.lech_hien, true)));
                log("10. P: DỪNG PHÁT, tạo tệp mẫu mp4, nguồn Tệp video, PHÁT");
                self.p.bam_dung();
                let _ = std::fs::create_dir_all("out");
                match tao_tep_mau(&Path::new("out").join("tep_mau.mp4").to_string_lossy(), 10) {
                    Ok(tep) => self.p.dat_tep(&tep),
                    Err(e) => log(&format!("   Không tạo được tệp mẫu: {e}")),
                }
                log(&format!("   {} | {}", self.p.ten_cam(), self.p.ten_mic()));
                self.cho(0.3);
            }
            13 => {
                self.p.bam_phat();
                self.cho(4.4);
            }
            14 => {
                let tp = self.p.lop.as_ref().unwrap().trang_thai();
                log(&format!("   P tệp: I {} P {}, TB I {:.0} B, P {:.0} B", tp.so_i, tp.so_p, tp.tb_i, tp.tb_p));
                let cam: String = tp.camera.chars().take(52).collect();
                log(&format!("   P: {cam}"));
                let ta = self.a.trang_thai_lop().unwrap();
                log(&format!("   A: khung I {}, P {}, lệch {} ms, mức tiếng {:.1} dBFS", ta.khung_i, ta.khung_p, chu_ms(ta.lech_hien, true), ta.muc_am));
                self.chup(ctx);
            }
            15 => {
                let anh = self.anh.take().unwrap();
                let (wp, hp, rp) = rgb_cua_anh(&anh, Some(self.vung(&[1])));
                let (wa, ha, ra_) = rgb_cua_anh(&anh, Some(self.vung(&[2])));
                // ghép ngang: máy phát (trái) + người xem A (phải), nền xám, viền 4
                let (w, h) = (wp + wa + 8 + 4, hp.max(ha) + 8);
                let mut rgb = vec![0xd0u8; w * h * 3];
                for (x0, (ww, hh, src)) in [(4usize, (wp, hp, &rp)), (4 + wp + 4, (wa, ha, &ra_))] {
                    for y in 0..hh {
                        let d = ((4 + y) * w + x0) * 3;
                        rgb[d..d + ww * 3].copy_from_slice(&src[y * ww * 3..(y + 1) * ww * 3]);
                    }
                }
                let ra = Path::new(&self.out).parent().map(|p| p.to_string_lossy().into_owned()).filter(|p| !p.is_empty()).unwrap_or_else(|| ".".into()) + "/ket_qua_gui_tep.png";
                match ghi_png(&ra, &rgb, w, h) {
                    Ok(()) => log(&format!("   Đã ghi {ra} ({} KB)", std::fs::metadata(&ra).map_or(0, |m| m.len() / 1024))),
                    Err(e) => log(&format!("   Không ghi được {ra}: {e}")),
                }
                self.cho(0.0);
            }
            16 => {
                log("11. Server bấm STOP SERVER");
                self.sv.bam_stop();
                self.cho_khi(|d| !d.sv.lop.dang_chay() && !d.sv.dang_dung());
            }
            17 => self.cho(0.6),
            18 => {
                let p: String = format!("Trạng thái: {}", if self.p.dang_phat() { "ĐANG PHÁT".to_string() } else { self.p.nhan_tt.clone() }).chars().take(46).collect();
                let a: String = self.a.chu_trang_thai().chars().take(46).collect();
                log(&format!("   P: {p}"));
                log(&format!("   A: {a}"));
                self.p.dong();
                self.a.dong();
                self.b.dong();
                self.sv.dong();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                self.cho(1e9);
            }
            _ => {}
        }
    }
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if !self.bat_dau {
            self.bat_dau = true;
            self.han = bay_gio();
        }
        self.ppp = ctx.pixels_per_point();
        if self.cho_anh {
            let anh = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if anh.is_some() {
                self.anh = anh;
                self.cho_anh = false;
            }
        }
        // ---- vẽ 4 cửa sổ con (vị trí tính theo kích thước thật của khung lần vẽ trước)
        let k = self.khung;
        let vi_tri = [
            egui::pos2(4.0, 4.0),
            egui::pos2(k[0].max.x + 10.0, 4.0),
            egui::pos2(4.0, k[0].max.y.max(k[1].max.y) + 10.0),
            egui::pos2(k[2].max.x + 10.0, k[0].max.y.max(k[1].max.y) + 10.0),
        ];
        egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::from_gray(0xd0))).show(ctx, |_| {});
        for i in 0..4 {
            let r = egui::Window::new(ten_cs(i))
                .id(egui::Id::new(("cs", i)))
                .current_pos(vi_tri[i])
                .resizable(false)
                .collapsible(false)
                .movable(false)
                .frame(egui::Frame::window(&ctx.style()).fill(NEN).rounding(0.0))
                .show(ctx, |ui| match i {
                    0 => {
                        ui.set_width(900.0);
                        self.sv.ui(ui)
                    }
                    1 => {
                        ui.set_width(820.0);
                        self.p.ui(ui)
                    }
                    2 => {
                        ui.set_width(760.0);
                        self.a.ui(ui)
                    }
                    _ => {
                        ui.set_width(760.0);
                        self.b.ui(ui)
                    }
                });
            if let Some(r) = r {
                self.khung[i] = r.response.rect;
            }
        }
        // ---- kịch bản
        let san_sang = match self.dk {
            Some((dk, han)) => {
                if dk(self) {
                    self.dk = None;
                    true
                } else if bay_gio() > han {
                    log("Kịch bản chờ quá lâu – dừng");
                    self.dk = None;
                    self.buoc = 18;
                    true
                } else {
                    false
                }
            }
            None => bay_gio() >= self.han,
        };
        if san_sang && self.buoc <= 18 {
            self.buoc(ctx);
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

/// demo_gui [host] [cổng_tcp] [cổng_udp] [tệp_ảnh]
pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let tcp: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(44652);
    let udp: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(tcp + 1);
    let out = args.get(3).cloned().unwrap_or_else(|| "ket_qua_gui.png".into());
    let app = DemoApp {
        sv: ServerApp::new(&host, tcp, udp),
        p: PhatApp::new("P", &host, tcp),
        a: XemApp::new("A", &host, tcp),
        b: XemApp::new("B", &host, tcp),
        out,
        buoc: 0,
        han: 0.0,
        dk: None,
        anh: None,
        cho_anh: false,
        khung: [egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(920.0, 640.0)); 4],
        ppp: 1.0,
        bat_dau: false,
    };
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1800.0, 1340.0]).with_title("Demo giao diện MR6.5 – máy chủ, máy phát và 2 người xem"),
        ..Default::default()
    };
    eframe::run_native(
        "Demo MR6.5",
        opt,
        Box::new(|cc| {
            cai_phong_chu(&cc.egui_ctx);
            dat_kieu(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
    .unwrap_or_else(|e| log(&format!("Không mở được cửa sổ: {e}")));
    log("Kết thúc");
    std::process::exit(0);
}
