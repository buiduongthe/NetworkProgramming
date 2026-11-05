//! Bài mở rộng MR6.5 – Máy phát trực tiếp có giao diện (egui).
//!
//! Lõi MayPhat (may_phat.rs) giữ kết nối, luồng âm thanh 20 ms và luồng video; cửa sổ chọn nguồn
//! (camera thật qua ffmpeg / tệp video / mẫu thử testsrc / giả lập clapper; micro thật qua cpal /
//! tiếng của tệp video / giả lập), fps, GOP, bấm Phát / Dừng phát / Khung khóa ngay; cứ 100 ms đọc
//! trạng thái để vẽ xem trước, thanh mức micro, thống kê và danh sách người xem. Thiết bị thật chỉ
//! được mở khi bấm PHÁT. Nút "Chọn tệp…" mở hộp thoại chọn tệp vẽ bằng egui (duyệt thư mục, lọc đuôi
//! tệp video) – không cần thư viện hộp thoại riêng của hệ điều hành.
//!
//! Chạy:  phat_gui [tên] [host] [cổng_tcp]

use crate::camera::{co_ffmpeg, liet_ke_camera, mo_camera, GIA_LAP, MAU_THU};
use crate::chung::{bay_gio, CAO, FPS_MAC_DINH, GOP_MAC_DINH, RONG};
use crate::gui_chung::*;
use crate::may_phat::MayPhat;
use crate::tep_video::{mo_tep, tao_tep_mau, TEP};
use crate::thiet_bi::{ds_micro, mic_chung, mo_mic_that, MicGiaLap};
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Mục trong danh sách micro.
#[derive(Clone, PartialEq)]
pub enum MaMic {
    That(String),
    Tep,
    GiaLap,
}

const DUOI_VIDEO: [&str; 8] = ["mp4", "mkv", "avi", "mov", "webm", "flv", "ts", "m4v"];

/// Hộp thoại chọn tệp vẽ bằng egui: thư mục hiện tại, các thư mục con và tệp video (hoặc mọi tệp).
struct HopThoaiTep {
    mo: bool,
    thu_muc: PathBuf,
    nhap: String,
    moi_tep: bool,
    chon: Option<PathBuf>,
}

impl HopThoaiTep {
    fn ve(&mut self, ctx: &egui::Context) -> Option<String> {
        if !self.mo {
            return None;
        }
        let mut ket_qua = None;
        let mut di_toi: Option<PathBuf> = None;
        egui::Window::new("Chọn tệp video").collapsible(false).resizable(false).default_width(520.0).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Thư mục:");
                let r = ui.add(egui::TextEdit::singleline(&mut self.nhap).desired_width(330.0));
                if ui.button("Đi").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    di_toi = Some(PathBuf::from(self.nhap.trim()));
                }
                if ui.button("Lên").clicked() {
                    if let Some(p) = self.thu_muc.parent() {
                        di_toi = Some(p.to_path_buf());
                    }
                }
            });
            ui.checkbox(&mut self.moi_tep, "Mọi tệp (*.*) – bỏ chọn: chỉ tệp video (mp4 mkv avi mov webm flv ts m4v)");
            let mut muc: Vec<(bool, String, PathBuf)> = std::fs::read_dir(&self.thu_muc)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| (e.path().is_dir(), e.file_name().to_string_lossy().into_owned(), e.path()))
                        .filter(|(la_tm, ten, p)| {
                            !ten.starts_with('.') && (*la_tm || self.moi_tep || p.extension().is_some_and(|d| DUOI_VIDEO.contains(&d.to_string_lossy().to_lowercase().as_str())))
                        })
                        .collect()
                })
                .unwrap_or_default();
            muc.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.to_lowercase().cmp(&b.1.to_lowercase())));
            egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, Color32::from_gray(0x90))).show(ui, |ui| {
                ui.set_width(500.0);
                egui::ScrollArea::vertical().max_height(260.0).auto_shrink([false, false]).show(ui, |ui| {
                    for (la_tm, ten, p) in muc {
                        let chu = if la_tm { format!("[thư mục] {ten}") } else { ten };
                        let r = ui.selectable_label(self.chon.as_ref() == Some(&p), chu);
                        if r.double_clicked() {
                            if la_tm {
                                di_toi = Some(p.clone());
                            } else {
                                ket_qua = Some(p.to_string_lossy().into_owned());
                            }
                        } else if r.clicked() {
                            self.chon = Some(p);
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                let la_tep = self.chon.as_ref().is_some_and(|p| p.is_file());
                if ui.add_enabled(la_tep, egui::Button::new("Mở")).clicked() {
                    ket_qua = self.chon.as_ref().map(|p| p.to_string_lossy().into_owned());
                }
                if ui.button("Hủy").clicked() {
                    self.mo = false;
                }
            });
        });
        if let Some(p) = di_toi.filter(|p| p.is_dir()) {
            self.thu_muc = p.canonicalize().unwrap_or(p);
            self.nhap = self.thu_muc.to_string_lossy().into_owned();
            self.chon = None;
        }
        if ket_qua.is_some() {
            self.mo = false;
        }
        ket_qua
    }
}

pub struct PhatApp {
    pub ten: String,
    pub ip: String,
    pub cong: String,
    pub lop: Option<Arc<MayPhat>>,
    pub ds_cam: Vec<(String, String)>,
    pub chon_cam: usize,
    pub ds_mic: Vec<(MaMic, String)>,
    pub chon_mic: usize,
    pub fps: u32,
    pub gop: u32,
    pub o_tep: String,
    ffmpeg: bool,
    pub nhat_ky: NhatKy,
    dinh_ky: ThongKeDinhKy,
    thanh_mic: ThanhMuc,
    tex: Option<egui::TextureHandle>,
    ts_da_ve: Option<u64>,
    pub nhan_tt: String,
    pub chi_bao: (String, Color32),
    ket_noi_kq: Arc<Mutex<Option<Result<u32, String>>>>,
    pub dang_ket_noi: bool,
    pub da_ket_noi_gui: bool, // giao diện đang ở trạng thái "đã kết nối"
    tao_mau_kq: Arc<Mutex<Option<Result<String, String>>>>,
    pub dang_tao_mau: bool,
    luong_mic: Option<cpal::Stream>, // luồng cpal của micro thật (giữ để còn thu)
    moc_giay: f64,
    hop_thoai: HopThoaiTep,
}

impl PhatApp {
    pub fn new(ten: &str, host: &str, cong: u16) -> PhatApp {
        let ffmpeg = co_ffmpeg();
        let mut ds_cam = liet_ke_camera();
        if ffmpeg {
            ds_cam.push((TEP.into(), "Tệp video (chọn tệp bên dưới)".into()));
            ds_cam.push((MAU_THU.into(), "Mẫu thử ffmpeg (testsrc)".into()));
        }
        ds_cam.push((GIA_LAP.into(), "Camera giả lập (bảng clapper)".into()));
        let mut ds_mic: Vec<(MaMic, String)> = ds_micro().into_iter().map(|t| (MaMic::That(t.clone()), t)).collect();
        if ffmpeg {
            ds_mic.push((MaMic::Tep, "Tiếng của tệp video".into()));
        }
        ds_mic.push((MaMic::GiaLap, "Micro giả lập (440 Hz + bíp mỗi giây)".into()));
        let thu_muc = std::env::current_dir().unwrap_or_default();
        PhatApp {
            ten: ten.into(),
            ip: host.into(),
            cong: cong.to_string(),
            lop: None,
            chon_cam: ds_cam.len() - 1,
            ds_cam,
            chon_mic: ds_mic.len() - 1,
            ds_mic,
            fps: FPS_MAC_DINH,
            gop: GOP_MAC_DINH,
            o_tep: String::new(),
            ffmpeg,
            nhat_ky: NhatKy::new(&format!("phat {ten}")),
            dinh_ky: ThongKeDinhKy::new(&format!("phat {ten}")),
            thanh_mic: ThanhMuc::new(),
            tex: None,
            ts_da_ve: None,
            nhan_tt: "chưa kết nối".into(),
            chi_bao: (format!("Camera: tắt | Micro: tắt{}", if ffmpeg { "" } else { " (không có ffmpeg)" }), Color32::from_gray(0x40)),
            ket_noi_kq: Arc::new(Mutex::new(None)),
            dang_ket_noi: false,
            da_ket_noi_gui: false,
            tao_mau_kq: Arc::new(Mutex::new(None)),
            dang_tao_mau: false,
            luong_mic: None,
            moc_giay: bay_gio(),
            hop_thoai: HopThoaiTep { mo: false, nhap: thu_muc.to_string_lossy().into_owned(), thu_muc, moi_tep: false, chon: None },
        }
    }

    pub fn dang_phat(&self) -> bool {
        self.lop.as_ref().is_some_and(|l| l.dang_phat())
    }

    // ------------------------------------------------------------ kết nối
    pub fn bam_ket_noi(&mut self) {
        if let Some(l) = self.lop.as_ref().filter(|l| l.da_ket_noi()) {
            l.dong();
            self.luong_mic = None;
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
        let lop = MayPhat::new(self.ten.trim(), self.nhat_ky.bao(), 0.0);
        self.lop = Some(Arc::clone(&lop));
        self.dang_ket_noi = true;
        let (kq, host) = (Arc::clone(&self.ket_noi_kq), self.ip.trim().to_string());
        std::thread::spawn(move || {
            let r = lop.ket_noi(&host, cong); // connect có thể chờ tới 5 s
            *kq.lock().unwrap() = Some(r);
        });
    }

    fn da_ngat(&mut self, text: &str) {
        self.da_ket_noi_gui = false;
        self.dang_ket_noi = false;
        self.nhan_tt = text.into();
    }

    // ------------------------------------------------------------ phát
    pub fn bam_phat(&mut self) {
        let Some(lop) = self.lop.clone().filter(|l| l.da_ket_noi()) else { return };
        let (ma_cam, ten_cam) = self.ds_cam[self.chon_cam].clone();
        let (fps, gop) = (self.fps.clamp(1, 30), self.gop.clamp(1, 300));
        let ma_mic = self.ds_mic[self.chon_mic].0.clone();
        let (mut cam, mut mic) = (None, None);
        if ma_cam == TEP {
            // tệp video: hình (và tiếng nếu chọn)
            match mo_tep(self.o_tep.trim(), fps, ma_mic == MaMic::Tep) {
                Ok((c, m, ten_tep)) => {
                    self.nhat_ky.ghi(&format!("Phát tệp {ten_tep}{}", if m.is_some() { " (cả tiếng)" } else { "" }));
                    cam = Some(c);
                    mic = m;
                }
                Err(e) => self.nhat_ky.ghi(&format!("Không mở được tệp video ({e}); dùng camera giả lập")),
            }
        }
        let ma = lop.ma();
        let cam = cam.unwrap_or_else(|| mo_camera(if ma_cam == TEP { GIA_LAP } else { &ma_cam }, &ten_cam, &lop.ten, if ma == 0 { 1 } else { ma }, fps));
        self.luong_mic = None;
        let mic = match (mic, &ma_mic) {
            (Some(m), _) => m,
            (None, MaMic::That(ten)) => match mo_mic_that(ten) {
                Ok((luong, m)) => {
                    self.luong_mic = Some(luong);
                    m
                }
                Err(e) => {
                    self.nhat_ky.ghi(&format!("không mở được micro '{ten}' ({e}); chuyển sang giả lập"));
                    mic_chung(MicGiaLap::new())
                }
            },
            (None, MaMic::Tep) => {
                self.nhat_ky.ghi("Tiếng của tệp chỉ dùng được khi camera là tệp video; dùng micro giả lập");
                mic_chung(MicGiaLap::new())
            }
            (None, MaMic::GiaLap) => mic_chung(MicGiaLap::new()),
        };
        let (mo_cam, cam_that) = {
            let c = cam.lock().unwrap();
            (c.mo_ta(), c.that())
        };
        let (mo_mic, mic_that) = {
            let m = mic.lock().unwrap();
            (m.mo_ta(), m.that())
        };
        lop.bat_dau_phat(cam, mic, fps, gop);
        self.nhat_ky.ghi(&format!("Camera: {mo_cam}"));
        self.nhat_ky.ghi(&format!("Micro: {mo_mic}"));
        self.chi_bao = (format!("Camera: {mo_cam} | Micro: {mo_mic}"), if cam_that || mic_that { CHU_XANH } else { CHU_NAU });
    }

    pub fn bam_dung(&mut self) {
        if let Some(l) = &self.lop {
            l.dung_phat();
        }
        self.luong_mic = None;
        self.chi_bao = ("Camera: tắt | Micro: tắt".into(), Color32::from_gray(0x40));
    }

    pub fn bam_khung_khoa(&mut self) {
        if let Some(l) = &self.lop {
            l.khung_khoa_ngay("nút Khung khóa ngay");
        }
    }

    fn ap_tham_so(&mut self) {
        if let Some(l) = self.lop.as_ref().filter(|l| l.dang_phat()) {
            l.dat_tham_so(self.fps.clamp(1, 30), self.gop.clamp(1, 300));
        }
    }

    /// Chọn camera theo mã (kịch bản demo dùng).
    pub fn chon_camera(&mut self, ma: &str) -> bool {
        match self.ds_cam.iter().position(|(m, _)| m == ma) {
            Some(i) => {
                self.chon_cam = i;
                self.doi_camera();
                true
            }
            None => false,
        }
    }

    /// Chọn camera "Tệp video" -> micro tự chuyển sang tiếng của tệp (đổi lại được); bỏ "Tệp video"
    /// mà micro còn là tiếng của tệp -> micro về thiết bị đầu tiên trong danh sách (micro thật nếu có).
    fn doi_camera(&mut self) {
        if self.ds_cam[self.chon_cam].0 == TEP {
            if let Some(i) = self.ds_mic.iter().position(|(m, _)| *m == MaMic::Tep) {
                self.chon_mic = i;
            }
        } else if self.ds_mic[self.chon_mic].0 == MaMic::Tep {
            self.chon_mic = 0;
        }
    }

    pub fn dat_tep(&mut self, duong: &str) {
        self.o_tep = duong.to_string();
        if let Some(i) = self.ds_cam.iter().position(|(m, _)| m == TEP) {
            self.chon_cam = i;
        }
        self.doi_camera();
    }

    /// Tạo tệp mẫu 10 s (chớp trắng + bíp mỗi giây) trong out/ của thư mục hiện tại, rồi chọn nó.
    fn tao_mau(&mut self) {
        let duong = std::env::current_dir().unwrap_or_default().join("out").join("tep_mau.mp4");
        let _ = std::fs::create_dir_all(duong.parent().unwrap());
        self.dang_tao_mau = true;
        let kq = Arc::clone(&self.tao_mau_kq);
        std::thread::spawn(move || {
            let r = tao_tep_mau(&duong.to_string_lossy(), 10).map_err(|e| e.to_string());
            *kq.lock().unwrap() = Some(r);
        });
    }

    pub fn ten_cam(&self) -> &str {
        &self.ds_cam[self.chon_cam].1
    }

    pub fn ten_mic(&self) -> &str {
        &self.ds_mic[self.chon_mic].1
    }

    // ------------------------------------------------------------ vẽ
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // kết quả của các việc chạy nền
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
        let kq = self.tao_mau_kq.lock().unwrap().take();
        if let Some(r) = kq {
            self.dang_tao_mau = false;
            match r {
                Ok(d) => {
                    self.nhat_ky.ghi(&format!("Đã tạo tệp mẫu {d}"));
                    self.dat_tep(&d);
                }
                Err(e) => self.nhat_ky.ghi(&format!("Không tạo được tệp mẫu: {e}")),
            }
        }
        if let Some(d) = self.hop_thoai.ve(ui.ctx()) {
            self.dat_tep(&d);
        }
        if bay_gio() - self.moc_giay >= 1.0 {
            self.moc_giay = bay_gio();
            if let Some(l) = &self.lop {
                l.thong_ke_giay(); // cập nhật kbit/s của giây vừa qua
            }
        }
        let tt = self.lop.as_ref().map(|l| l.trang_thai());
        if let Some(t) = &tt {
            if self.da_ket_noi_gui && !t.da_ket_noi {
                self.da_ngat("mất kết nối server");
            }
            if t.dang_phat {
                self.dinh_ky.in_neu_den_han(|| {
                    format!("I {} P {} | hình {:.0} tiếng {:.0} kbit/s | mic {:.0} dBFS | PLI {} | {} người xem", t.so_i, t.so_p, t.kbps_hinh, t.kbps_am, t.muc_mic, t.pli_nhan, t.nguoi_xem.len())
                });
            }
            match &t.xem_truoc {
                Some((rgb, ts)) if t.dang_phat => {
                    if self.ts_da_ve != Some(*ts) {
                        dat_texture(ui.ctx(), &mut self.tex, &format!("phat_{}", self.ten), RONG, CAO, rgb);
                        self.ts_da_ve = Some(*ts);
                    }
                }
                _ if !t.dang_phat => {
                    self.tex = None;
                    self.ts_da_ve = None;
                }
                _ => {}
            }
        }
        let ket_noi = self.da_ket_noi_gui;
        let phat = tt.as_ref().is_some_and(|t| t.dang_phat);
        // ---- hàng trên: kết nối, nguồn và tham số
        ui.horizontal_top(|ui| {
            nhom(ui, "Kết nối server", |ui| {
                ui.set_min_height(160.0);
                egui::Grid::new(("p_kn", self.ten.clone())).num_columns(2).show(ui, |ui| {
                    ui.label("Server IP:");
                    o_nhap(ui, &mut self.ip, 110.0, !ket_noi && !self.dang_ket_noi);
                    ui.end_row();
                    ui.label("Port:");
                    o_nhap(ui, &mut self.cong, 60.0, !ket_noi && !self.dang_ket_noi);
                    ui.end_row();
                    ui.label("Tên:");
                    o_nhap(ui, &mut self.ten, 80.0, !ket_noi && !self.dang_ket_noi);
                    ui.end_row();
                });
                let (chu, mau) = if self.dang_ket_noi {
                    ("Đang kết nối…", XANH_LA)
                } else if ket_noi {
                    ("Ngắt kết nối", DO)
                } else {
                    ("Kết nối", XANH_LA)
                };
                if nut(ui, chu, mau, !self.dang_ket_noi, 150.0) {
                    self.bam_ket_noi();
                }
            });
            nhom(ui, "Nguồn và tham số", |ui| {
                ui.set_min_height(160.0);
                let doi_duoc = !phat;
                egui::Grid::new(("p_nguon", self.ten.clone())).num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
                    ui.label("Camera:");
                    let truoc = self.chon_cam;
                    ui.add_enabled_ui(doi_duoc, |ui| {
                        egui::ComboBox::from_id_salt(("cam", self.ten.clone())).width(300.0).selected_text(self.ds_cam[self.chon_cam].1.clone()).show_ui(ui, |ui| {
                            for (i, (_, t)) in self.ds_cam.iter().enumerate() {
                                ui.selectable_value(&mut self.chon_cam, i, t);
                            }
                        });
                    });
                    if self.chon_cam != truoc {
                        self.doi_camera();
                    }
                    ui.end_row();
                    ui.label("Micro:");
                    ui.add_enabled_ui(doi_duoc, |ui| {
                        egui::ComboBox::from_id_salt(("mic", self.ten.clone())).width(300.0).selected_text(self.ds_mic[self.chon_mic].1.clone()).show_ui(ui, |ui| {
                            for (i, (_, t)) in self.ds_mic.iter().enumerate() {
                                ui.selectable_value(&mut self.chon_mic, i, t);
                            }
                        });
                    });
                    ui.end_row();
                    ui.label("FPS:");
                    ui.horizontal(|ui| {
                        let a = ui.add(egui::DragValue::new(&mut self.fps).range(1..=30)).changed();
                        ui.add_space(120.0);
                        ui.label("GOP (khung):");
                        let b = ui.add(egui::DragValue::new(&mut self.gop).range(1..=300)).changed();
                        if a || b {
                            self.ap_tham_so();
                        }
                    });
                    ui.end_row();
                    ui.label("Tệp video:");
                    ui.horizontal(|ui| {
                        o_nhap(ui, &mut self.o_tep, 220.0, self.ffmpeg);
                        if nut(ui, "Chọn tệp…", XAM, self.ffmpeg, 80.0) {
                            self.hop_thoai.mo = true;
                        }
                        let chu = if self.dang_tao_mau { "Đang tạo…" } else { "Tạo tệp mẫu" };
                        if nut(ui, chu, XAM, self.ffmpeg && !self.dang_tao_mau, 90.0) {
                            self.tao_mau();
                        }
                    });
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if nut(ui, "PHÁT", XANH_LA, ket_noi && !phat, 100.0) {
                        self.bam_phat();
                    }
                    if nut(ui, "DỪNG PHÁT", DO, ket_noi && phat, 100.0) {
                        self.bam_dung();
                    }
                    if nut(ui, "Khung khóa ngay", XANH_DUONG, ket_noi && phat, 130.0) {
                        self.bam_khung_khoa();
                    }
                });
                ui.label(RichText::new(&self.chi_bao.0).color(self.chi_bao.1));
            });
        });
        // ---- hàng giữa: xem trước, thống kê
        ui.horizontal_top(|ui| {
            nhom(ui, "Xem trước (khung vừa gửi)", |ui| {
                o_hinh(ui, self.tex.as_ref(), 2.0 * RONG as f32, 2.0 * CAO as f32, "");
                ui.horizontal(|ui| {
                    ui.label("Mức micro:");
                    self.thanh_mic.ve(ui, tt.as_ref().map_or(f64::NEG_INFINITY, |t| t.muc_mic), 230.0);
                });
            });
            nhom(ui, "Thống kê", |ui| {
                ui.set_width(ui.available_width() - 12.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                let gt = |s: String| RichText::new(s).strong().size(13.0);
                let (ip, tb, pmin, kb, pli, ds) = match &tt {
                    Some(t) => (
                        format!("{} / {}", t.so_i, t.so_p),
                        format!("{:.0} B / {:.0} B", t.tb_i, t.tb_p),
                        format!("{} B / {} B", t.p_min, t.p_max),
                        format!("{:.0} / {:.0}", t.kbps_am, t.kbps_hinh),
                        t.pli_nhan.to_string(),
                        t.nguoi_xem.clone(),
                    ),
                    None => ("–".into(), "–".into(), "–".into(), "–".into(), "–".into(), Vec::new()),
                };
                for (nhan, v) in [("Khung I / P đã gửi", ip), ("Kích thước TB I / P", tb), ("P nhỏ nhất / lớn nhất", pmin), ("kbit/s tiếng / hình", kb), ("PLI (xin khung khóa) nhận", pli)] {
                    ui.label(format!("{nhan}:"));
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(gt(v));
                    });
                }
                ui.add_space(4.0);
                ui.label(format!("Người xem ({}):", ds.len()));
                egui::Frame::none().fill(Color32::WHITE).stroke(egui::Stroke::new(1.0_f32, Color32::from_gray(0x90))).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_height(66.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for t in &ds {
                        ui.label(RichText::new(t).color(Color32::BLACK));
                    }
                });
            });
        });
        nhom(ui, "Nhật ký", |ui| {
            self.nhat_ky.ve(ui, 5, ui.available_width() - 12.0);
        });
        let trang = match &tt {
            Some(t) if t.dang_phat => format!("ĐANG PHÁT {} fps, GOP {} – {} người xem", t.fps, t.gop, t.nguoi_xem.len()),
            _ => self.nhan_tt.clone(),
        };
        thanh_trang_thai(ui, &format!("Trạng thái: {trang}"));
    }

    pub fn dong(&mut self) {
        if let Some(l) = &self.lop {
            l.dong();
        }
        self.luong_mic = None;
    }
}

// ---------------------------------------------------------------- Cửa sổ riêng
struct CuaSoPhat(PhatApp);

impl eframe::App for CuaSoPhat {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().frame(egui::Frame::central_panel(&ctx.style()).fill(NEN)).show(ctx, |ui| self.0.ui(ui));
        ctx.request_repaint_after(Duration::from_millis(100));
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.0.dong();
    }
}

/// phat_gui [tên] [host] [cổng_tcp] – mặc định P 127.0.0.1 44652.
pub fn run(args: &[String]) {
    IN_RA.store(true, Ordering::SeqCst); // chạy riêng: nhật ký cũng ra dòng lệnh
    let ten = args.first().cloned().unwrap_or_else(|| "P".into());
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(44652);
    let app = PhatApp::new(&ten, &host, cong);
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([850.0, 720.0]).with_title(format!("Máy phát trực tiếp – MR6.5 ({ten})")),
        ..Default::default()
    };
    eframe::run_native(
        "Máy phát trực tiếp",
        opt,
        Box::new(|cc| {
            cai_phong_chu(&cc.egui_ctx);
            dat_kieu(&cc.egui_ctx);
            Ok(Box::new(CuaSoPhat(app)))
        }),
    )
    .unwrap_or_else(|e| eprintln!("Không mở được cửa sổ: {e}"));
    std::process::exit(0);
}
