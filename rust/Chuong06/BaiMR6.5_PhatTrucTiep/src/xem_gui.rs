//! Bài mở rộng MR6.5 – Người xem phát trực tiếp có giao diện (egui).
//!
//! Lõi NguoiXem (nguoi_xem.rs) nhận, giải mã, phát âm thanh (đồng hồ chủ) và chọn khung để hiện; cửa
//! sổ cứ 50 ms đọc khung đang hiện để vẽ (phóng ×2) và cứ 200 ms cập nhật thanh mức, thống kê.
//! Điều khiển: Kết nối/Ngắt, công tắc Đồng bộ tiếng-hình, chọn nhận Âm thanh / Video / Cả hai, độ sâu
//! bộ đệm jitter, Phát ra loa (có thiết bị phát qua cpal), Chụp (lưu out/<tên>_gui_chup_N.png).
//!
//! Chạy:  xem_gui [tên] [host] [cổng_tcp]

use crate::chung::{bay_gio, ghi_png, CAO, DO_SAU_MAC_DINH, RONG};
use crate::gui_chung::*;
use crate::nguoi_xem::{anh_chup, NguoiXem, TrangThai};
use crate::thiet_bi::{mo_loa_xem, HangMau};
use eframe::egui::{self, Color32, RichText};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct XemApp {
    pub ten: String,
    pub ip: String,
    pub cong: String,
    pub lop: Option<Arc<NguoiXem>>,
    _luong_loa: Option<cpal::Stream>,
    hang_loa: Option<HangMau>,
    ly_do_loa: String,
    pub loa: bool,
    pub dong_bo: bool,
    pub nhan: &'static str, // "am_thanh" | "video" | "ca_hai"
    pub do_sau: usize,
    pub nhat_ky: NhatKy,
    dinh_ky: ThongKeDinhKy,
    thanh: ThanhMuc,
    tex: Option<egui::TextureHandle>,
    so_da_ve: Option<u64>,
    nhan_khung: String,
    so_chup: usize,
    pub nhan_tt: String,
    ket_noi_kq: Arc<Mutex<Option<Result<u32, String>>>>,
    pub dang_ket_noi: bool,
    pub da_ket_noi_gui: bool,
    tt: Option<TrangThai>,
    moc_tt: f64,
}

impl XemApp {
    pub fn new(ten: &str, host: &str, cong: u16) -> XemApp {
        let (loa, ly_do_loa) = mo_loa_xem();
        let (luong, hang) = match loa {
            Some((l, h)) => (Some(l), Some(h)),
            None => (None, None),
        };
        XemApp {
            ten: ten.into(),
            ip: host.into(),
            cong: cong.to_string(),
            lop: None,
            loa: hang.is_some(),
            _luong_loa: luong,
            hang_loa: hang,
            ly_do_loa,
            dong_bo: true,
            nhan: "ca_hai",
            do_sau: DO_SAU_MAC_DINH,
            nhat_ky: NhatKy::new(&format!("xem {ten}")),
            dinh_ky: ThongKeDinhKy::new(&format!("xem {ten}")),
            thanh: ThanhMuc::new(),
            tex: None,
            so_da_ve: None,
            nhan_khung: "Khung: –".into(),
            so_chup: 0,
            nhan_tt: "chưa kết nối".into(),
            ket_noi_kq: Arc::new(Mutex::new(None)),
            dang_ket_noi: false,
            da_ket_noi_gui: false,
            tt: None,
            moc_tt: -1.0,
        }
    }

    // ------------------------------------------------------------ kết nối
    pub fn bam_ket_noi(&mut self) {
        if let Some(l) = self.lop.as_ref().filter(|l| l.da_ket_noi()) {
            l.ngat();
            self.da_ngat("đã ngắt kết nối");
            return;
        }
        let Ok(cong) = self.cong.trim().parse::<u16>() else {
            self.nhan_tt = "cổng không hợp lệ".into();
            return;
        };
        if let Some(l) = self.lop.take() {
            l.dong();
        }
        let loa = if self.loa { self.hang_loa.clone() } else { None };
        let lop = NguoiXem::new(self.ten.trim(), self.nhat_ky.bao(), self.do_sau, self.dong_bo, self.nhan != "video", self.nhan != "am_thanh", 0.0, loa);
        self.lop = Some(Arc::clone(&lop));
        self.dang_ket_noi = true;
        self.so_da_ve = None;
        let (kq, host) = (Arc::clone(&self.ket_noi_kq), self.ip.trim().to_string());
        std::thread::spawn(move || {
            let r = lop.ket_noi(&host, cong);
            *kq.lock().unwrap() = Some(r);
        });
    }

    fn da_ngat(&mut self, text: &str) {
        self.da_ket_noi_gui = false;
        self.dang_ket_noi = false;
        self.nhan_tt = text.into();
    }

    // ------------------------------------------------------------ điều khiển
    pub fn dat_dong_bo(&mut self, bat: bool) {
        self.dong_bo = bat;
        if let Some(l) = &self.lop {
            l.dat_dong_bo(bat);
        }
    }

    fn ap_dang_ky(&mut self) {
        if let Some(l) = &self.lop {
            l.dang_ky(self.nhan != "video", self.nhan != "am_thanh");
        }
    }

    fn ap_do_sau(&mut self) {
        if let Some(l) = &self.lop {
            l.dat_do_sau(self.do_sau);
            self.nhat_ky.ghi(&format!("Bộ đệm jitter: {} khung = {} ms", self.do_sau, self.do_sau * 20));
        }
    }

    pub fn chup(&mut self) -> Option<String> {
        let Some(c) = self.lop.as_ref().and_then(|l| l.chup()) else {
            self.nhat_ky.ghi("Chưa có hình để chụp");
            return None;
        };
        let (w, h, rgb) = anh_chup(&c);
        let _ = std::fs::create_dir_all("out");
        self.so_chup += 1;
        let duong = format!("out/{}_gui_chup_{}.png", self.lop.as_ref().unwrap().ten, self.so_chup);
        match ghi_png(&duong, &rgb, w, h) {
            Ok(()) => self.nhat_ky.ghi(&format!("Đã chụp {duong}")),
            Err(e) => self.nhat_ky.ghi(&format!("Không ghi được {duong}: {e}")),
        }
        Some(duong)
    }

    pub fn trang_thai_lop(&self) -> Option<TrangThai> {
        self.lop.as_ref().map(|l| l.trang_thai())
    }

    pub fn chu_trang_thai(&self) -> String {
        let trang = match &self.tt {
            Some(t) if t.da_ket_noi => format!("đang xem – nhận {:.0} kbit/s TB", t.kbps),
            _ => self.nhan_tt.clone(),
        };
        format!("Trạng thái: {trang}")
    }

    // ------------------------------------------------------------ vẽ
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let kq = self.ket_noi_kq.lock().unwrap().take();
        if let Some(r) = kq {
            match r {
                Ok(ma) => {
                    self.dang_ket_noi = false;
                    self.da_ket_noi_gui = true;
                    self.nhan_tt = format!("đã kết nối, mã {ma}");
                }
                Err(e) => self.da_ngat(&format!("không kết nối được – {e}")),
            }
        }
        // khung đang hiện (mỗi lần vẽ ~50 ms)
        if let Some(l) = &self.lop {
            let (hien, so) = {
                let t = l.khoa.lock().unwrap();
                (t.hien.clone(), t.so_hien)
            };
            match hien {
                Some(h) if self.so_da_ve != Some(so) => {
                    dat_texture(ui.ctx(), &mut self.tex, &format!("xem_{}", self.ten), h.w, h.h, &h.anh);
                    self.nhan_khung = format!("Khung #{}  hình t={:.3} s  tiếng t={}", h.so, h.ts as f64 / 1000.0, h.am.map_or("–".into(), |a| format!("{:.3} s", a / 1000.0)));
                    self.so_da_ve = Some(so);
                }
                None => self.tex = None,
                _ => {}
            }
        }
        // số liệu (mỗi 200 ms)
        if bay_gio() - self.moc_tt >= 0.2 {
            self.moc_tt = bay_gio();
            self.tt = self.trang_thai_lop();
            if let Some(t) = &self.tt {
                if t.da_ket_noi {
                    let loa = self.hang_loa.is_some() && self.loa;
                    self.dinh_ky.in_neu_den_han(|| {
                        format!(
                            "tiếng {} gói, mất {}, mức {:.0} dBFS, loa {} | hình I {} P {}, đứng {}, lệch {}, trễ {} | đồng bộ {}",
                            t.goi_am,
                            t.mat_am,
                            t.muc_am,
                            if loa { "BẬT" } else { "tắt" },
                            t.khung_i,
                            t.khung_p,
                            t.dung_hinh,
                            chu_ms(t.lech_hien, true),
                            chu_ms(t.tre_hien, false),
                            if t.dong_bo { "BẬT" } else { "TẮT" }
                        )
                    });
                }
                if self.da_ket_noi_gui && !t.da_ket_noi {
                    self.da_ngat("mất kết nối server");
                }
            }
        }
        let ket_noi = self.da_ket_noi_gui;
        let bat_nhap = !ket_noi && !self.dang_ket_noi;
        nhom(ui, "Kết nối server", |ui| {
            ui.set_width(ui.available_width() - 12.0);
            ui.horizontal(|ui| {
                ui.label("IP:");
                o_nhap(ui, &mut self.ip, 110.0, bat_nhap);
                ui.label(" Port:");
                o_nhap(ui, &mut self.cong, 50.0, bat_nhap);
                ui.label(" Tên:");
                o_nhap(ui, &mut self.ten, 70.0, bat_nhap);
                ui.add_space(8.0);
                let (chu, mau) = if self.dang_ket_noi {
                    ("Đang kết nối…", XANH_LA)
                } else if ket_noi {
                    ("Ngắt kết nối", DO)
                } else {
                    ("Kết nối", XANH_LA)
                };
                if nut(ui, chu, mau, !self.dang_ket_noi, 130.0) {
                    self.bam_ket_noi();
                }
            });
        });
        ui.horizontal_top(|ui| {
            nhom(ui, "Hình (phóng ×2)", |ui| {
                let cho = if ket_noi { "Chờ khung hình…" } else { "Chưa kết nối" };
                o_hinh(ui, self.tex.as_ref(), 2.0 * RONG as f32, 2.0 * CAO as f32, cho);
                ui.label(RichText::new(&self.nhan_khung).monospace().size(11.5));
                ui.horizontal(|ui| {
                    ui.label("Tiếng:");
                    let muc = self.tt.as_ref().map_or(f64::NEG_INFINITY, |t| t.muc_am);
                    self.thanh.ve(ui, muc, 200.0);
                    let co_loa = self.hang_loa.is_some();
                    if ui.add_enabled(co_loa, egui::Checkbox::new(&mut self.loa, "Loa")).changed() {
                        if let Some(l) = &self.lop {
                            l.dat_loa(if self.loa { self.hang_loa.clone() } else { None });
                        }
                    }
                });
                if self.hang_loa.is_some() {
                    ui.label(RichText::new("Loa: THẬT – thiết bị mặc định").color(CHU_XANH));
                } else {
                    ui.label(RichText::new(format!("Loa: {}", self.ly_do_loa)).color(CHU_NAU));
                }
            });
            ui.vertical(|ui| {
                nhom(ui, "Điều khiển", |ui| {
                    ui.set_width(ui.available_width() - 12.0);
                    let mut db = self.dong_bo;
                    if ui.checkbox(&mut db, "Đồng bộ tiếng-hình (tiếng làm đồng hồ chủ)").changed() {
                        self.dat_dong_bo(db);
                    }
                    ui.horizontal(|ui| {
                        ui.label("Nhận:");
                        let truoc = self.nhan;
                        ui.radio_value(&mut self.nhan, "am_thanh", "Âm thanh");
                        ui.radio_value(&mut self.nhan, "video", "Video");
                        ui.radio_value(&mut self.nhan, "ca_hai", "Cả hai");
                        if self.nhan != truoc {
                            self.ap_dang_ky();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Bộ đệm jitter (khung 20 ms):");
                        if ui.add(egui::DragValue::new(&mut self.do_sau).range(1..=25)).changed() {
                            self.ap_do_sau();
                        }
                        ui.add_space(8.0);
                        if nut(ui, "Chụp", XANH_DUONG, true, 80.0) {
                            self.chup();
                        }
                    });
                });
                nhom(ui, "Thống kê", |ui| {
                    ui.set_width(ui.available_width() - 12.0);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    let t = self.tt.as_ref();
                    let g = t.and_then(|t| t.giai_doan.last().cloned());
                    let lech = t.and_then(|t| t.lech_hien);
                    let dong: Vec<(&str, String, Color32)> = vec![
                        ("Trễ đầu-cuối (tiếng)*", t.map_or("–".into(), |t| chu_ms(t.tre_hien, false)), Color32::BLACK),
                        ("Lệch tiếng-hình hiện tại", chu_ms(lech, true), if lech.is_some_and(|l| l.abs() <= 40.0) { CHU_XANH } else { CHU_DO }),
                        (
                            "Lệch TB giai đoạn này",
                            g.map_or("–".into(), |g| format!("{} (đồng bộ {})", if g.n > 0 { chu_ms(Some(g.tong / g.n as f64), true) } else { "–".into() }, if g.dong_bo { "BẬT" } else { "TẮT" })),
                            Color32::BLACK,
                        ),
                        ("Tiếng nhận / mất", t.map_or("–".into(), |t| format!("{} / {} gói", t.goi_am, t.mat_am)), Color32::BLACK),
                        ("Khung nhận I / P", t.map_or("–".into(), |t| format!("{} / {} (hỏng {})", t.khung_i, t.khung_p, t.hong)), Color32::BLACK),
                        ("Đứng hình / PLI gửi", t.map_or("–".into(), |t| format!("{} / {}", t.dung_hinh, t.pli_gui)), Color32::BLACK),
                        ("Khung đầu sau", t.map_or("–".into(), |t| t.khung_dau_ms.map_or("–".into(), |ms| format!("{ms:.0} ms ({})", t.nguon_dau))), Color32::BLACK),
                    ];
                    for (nhan, v, mau) in dong {
                        ui.horizontal(|ui| {
                            let r = ui.label(format!("{nhan}:"));
                            ui.add_space((175.0 - r.rect.width()).max(0.0));
                            ui.label(RichText::new(v).strong().size(13.0).color(mau));
                        });
                    }
                    ui.label(RichText::new("* giả định cùng đồng hồ (cùng máy / NTP); lệch +: hình chậm hơn tiếng").size(10.5).color(Color32::from_gray(0x40)));
                });
            });
        });
        nhom(ui, "Nhật ký", |ui| {
            self.nhat_ky.ve(ui, 5, ui.available_width() - 12.0);
        });
        thanh_trang_thai(ui, &self.chu_trang_thai());
    }

    pub fn dong(&mut self) {
        if let Some(l) = &self.lop {
            l.dong();
        }
    }
}

// ---------------------------------------------------------------- Cửa sổ riêng
struct CuaSoXem(XemApp);

impl eframe::App for CuaSoXem {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().frame(egui::Frame::central_panel(&ctx.style()).fill(NEN)).show(ctx, |ui| self.0.ui(ui));
        ctx.request_repaint_after(Duration::from_millis(50));
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.0.dong();
    }
}

/// xem_gui [tên] [host] [cổng_tcp] – mặc định A 127.0.0.1 44652.
pub fn run(args: &[String]) {
    IN_RA.store(true, Ordering::SeqCst); // chạy riêng: nhật ký cũng ra dòng lệnh
    let ten = args.first().cloned().unwrap_or_else(|| "A".into());
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(44652);
    let app = XemApp::new(&ten, &host, cong);
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([780.0, 600.0]).with_title(format!("Người xem trực tiếp – MR6.5 ({ten})")),
        ..Default::default()
    };
    eframe::run_native(
        "Người xem trực tiếp",
        opt,
        Box::new(|cc| {
            cai_phong_chu(&cc.egui_ctx);
            dat_kieu(&cc.egui_ctx);
            Ok(Box::new(CuaSoXem(app)))
        }),
    )
    .unwrap_or_else(|e| eprintln!("Không mở được cửa sổ: {e}"));
    std::process::exit(0);
}
