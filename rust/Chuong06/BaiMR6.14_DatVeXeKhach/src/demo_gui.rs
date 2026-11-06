//! Bài mở rộng MR6.14 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).
//!
//! Một tiến trình, một cửa sổ egui chứa bốn "cửa sổ con": bảng điều khiển máy chủ và ba ứng dụng đặt vé
//! xe khách An, Bình, Chi. Kịch bản chọn đoạn, bấm chỗ, bấm nút như người dùng (gọi đúng các hàm mà nút
//! gọi; chờ một số giây hoặc chờ tới khi điều kiện đúng – vòng vẽ của egui vẫn chạy trong lúc chờ), in
//! `[demo] …`:
//!   1–3. xe giường nằm, chế độ khoa: An và Bình chọn D01 Sài Gòn → Đà Lạt, Chi chọn D01 Đà Lạt → Nha
//!        Trang, cả ba CÙNG bấm "GIỮ CHỖ" -> một trong An/Bình giữ được, Chi CŨNG giữ được (đoạn không
//!        chồng nhau), người còn lại bị từ chối; hai người giữ được thanh toán -> một giường bán hai vé;
//!   4.   chế độ khong_khoa, trễ 0–20 ms: cả ba chọn D02 Sài Gòn → Nha Trang, CÙNG bấm -> cả ba "Đã giữ",
//!        rồi hai người thấy "Chỗ D02 đã bị … lấy mất!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
//!   5.   So sánh 4 chế độ (mô phỏng 500 người cùng giữ ghế nóng D01, đoạn ngẫu nhiên, 200 kết nối) -> bảng;
//!   6.   đặt lại: xe ghế ngồi 50 chỗ, giữ chỗ 6 s: An giữ 05, 06 Sài Gòn → Bảo Lộc, không thanh toán;
//!        Bình bấm 06 cho Sài Gòn → Đà Lạt: đang giữ; hết hạn -> Bình giữ và mua 05, 06 Sài Gòn → Đà Lạt;
//!        Chi mua 05 Đà Lạt → Nha Trang (cùng ghế, đoạn sau); An giữ 10, 11 (đồng hồ chạy); chụp 4 cửa
//!        sổ ghép thành ket_qua_gui.png.
//!
//! Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" mr6_14_dat_ve_xe_khach demo_gui [host] [cổng]

use crate::chung::*;
use crate::dat_ve_xe::chu_o;
use crate::dat_ve_xe_gui::UngDungDatVeXe;
use crate::gui_chung::*;
use crate::may_chu::CONG_GUI;
use crate::server_gui::{tao_cua_so, tieu_de, CuaSoServerDatVeXe};
use eframe::egui;
use serde_json::Value;

const BUOC_CUOI: usize = 33;

fn log(text: &str) {
    in_dong("demo", text);
}

struct DemoApp {
    sv: CuaSoServerDatVeXe,
    ds: Vec<CuaSoNguoiDung<UngDungDatVeXe>>,
    kb: KichBan<DemoApp>,
    chup: MayChup,
    khung: [egui::Rect; 4], // vị trí 4 cửa sổ con (để cắt ảnh)
    ppp: f32,
    thu_muc: String,
    duoc: Vec<usize>,
    truoc: Vec<u32>,
}

impl DemoApp {
    fn in_cua_so(&self) {
        for cs in &self.ds {
            log(&format!("   {}: {}", cs.nd.ten, cs.ud.thong_bao.0));
        }
    }

    fn in_server(&self) {
        let tk = self.sv.lop.thong_ke_day_du();
        let s = |k: &str| so_us(tk.get(k));
        log(&format!("   máy chủ: {} yc, tc {}, từ chối {}, sai phạm {}", s("so_yc"), s("thanh_cong"), s("tu_choi"), s("sai_pham_tong")));
        for v in tk.get("vi_du_sai_pham").and_then(Value::as_array).into_iter().flatten().take(2) {
            log(&format!("     ví dụ: {}", py_str(v)));
        }
    }

    fn tom_tat(&self) -> String {
        so_us(self.sv.lop.thong_ke_day_du().get("tom_tat"))
    }

    fn doan(&mut self, i: usize, tu: usize, den: usize) {
        let cs = &mut self.ds[i];
        cs.ud.dat_doan(&cs.nd, tu, den);
    }

    fn bam(&mut self, i: usize, g: &str) {
        let cs = &mut self.ds[i];
        cs.ud.bam(&cs.nd, g);
    }

    fn giu(&mut self, i: usize) {
        let cs = &mut self.ds[i];
        cs.ud.giu_cho(&mut cs.nd);
    }

    fn thanh_toan(&mut self, i: usize) {
        let cs = &mut self.ds[i];
        cs.ud.thanh_toan(&mut cs.nd);
    }

    fn het_dang_giu(d: &DemoApp) -> bool {
        d.ds.iter().all(|cs| !cs.ud.thong_bao.0.contains("Đang giữ chỗ"))
    }

    fn duong(&self, ten: &str) -> String {
        format!("{}/{ten}", self.thu_muc)
    }

    /// Một bước của kịch bản; mỗi bước tự đặt điều kiện chờ cho bước sau.
    fn buoc(&mut self, ctx: &egui::Context) {
        let n = self.kb.buoc;
        self.kb.buoc += 1;
        match n {
            0 => self.kb.cho(0.5),
            1 => {
                log("1. Máy chủ: START SERVER (chế độ khoa, xe giường nằm, giữ chỗ 30 s)");
                self.sv.bam_start();
                self.kb.cho(0.3);
            }
            2 => {
                log("   An, Bình, Chi: Kết nối");
                for cs in &mut self.ds {
                    cs.bam_ket_noi();
                }
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.is_some()));
            }
            3 => self.kb.cho(1.0),
            4 => {
                log("2. An, Bình: D01 Sài Gòn → Đà Lạt; Chi: D01 Đà Lạt → Nha Trang; CÙNG bấm 'GIỮ CHỖ'");
                self.doan(0, 0, 2);
                self.doan(1, 0, 2);
                self.doan(2, 2, 3);
                for i in 0..3 {
                    self.bam(i, "D01");
                }
                for i in 0..3 {
                    self.giu(i);
                }
                self.kb.cho_khi(Self::het_dang_giu);
            }
            5 => self.kb.cho(0.4),
            6 => {
                self.in_cua_so();
                self.in_server();
                self.duoc = (0..3).filter(|&i| !self.ds[i].ud.giu.is_empty()).collect();
                let ten: Vec<&str> = self.duoc.iter().map(|&i| self.ds[i].nd.ten.as_str()).collect();
                log(&format!("3. {} bấm 'THANH TOÁN'", ten.join(" và ")));
                for i in self.duoc.clone() {
                    self.thanh_toan(i);
                }
                self.kb.cho_khi(|d| d.duoc.iter().all(|&i| !d.ds[i].ud.ve.is_empty()));
            }
            7 => self.kb.cho(0.4),
            8 => {
                for &i in &self.duoc {
                    let cs = &self.ds[i];
                    log(&format!("   {}: {}", cs.nd.ten, cs.ud.thong_bao.0));
                }
                log(&format!("   máy chủ: {}", self.tom_tat()));
                log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)");
                self.sv.o_tre = "20".into();
                self.sv.bam_tre();
                self.sv.chon_che_do("khong_khoa");
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_str(t, "che_do")) == Some("khong_khoa")));
            }
            9 => self.kb.cho(0.5),
            10 => {
                log("   Cả ba chọn D02 Sài Gòn → Nha Trang rồi CÙNG bấm 'GIỮ CHỖ'");
                self.truoc = self.ds.iter().map(|cs| cs.ud.so_giu).collect();
                for i in 0..3 {
                    self.doan(i, 0, 3);
                    self.bam(i, "D02");
                }
                for i in 0..3 {
                    self.giu(i);
                }
                self.kb.cho_khi(Self::het_dang_giu);
            }
            11 => {
                for (cs, t) in self.ds.iter().zip(&self.truoc) {
                    log(&format!("   {}: máy chủ trả lời {}", cs.nd.ten, if cs.ud.so_giu > *t { "ĐÃ GIỮ D02" } else { "từ chối" }));
                }
                self.kb.cho(0.6);
            }
            12 => {
                log("   …sau cap_nhat của máy chủ:");
                self.in_cua_so();
                self.in_server();
                let tt = self.sv.lop.trang_thai();
                let chu: Vec<&str> = (0..3).map(|k| chu_o(&tt, &format!("D02#{k}")).unwrap_or("None")).collect();
                log(&format!("   D02 thuộc: {}", chu.join(", ")));
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            13 => {
                let nan_nhan = self.ds.iter().position(|cs| cs.ud.thong_bao.0.contains("lấy mất")).unwrap_or(1);
                let anh = ghep_anh(&[vec![self.chup.cat(self.khung[0], self.ppp), self.chup.cat(self.khung[1 + nan_nhan], self.ppp)]]);
                ghi_va_bao(&self.duong("ket_qua_gui_tranh_chap.png"), &anh, log);
                log("5. khoa, trễ 0–2 ms; So sánh 4 chế độ (ghế nóng D01, đoạn ngẫu nhiên)");
                self.sv.o_tre = "2".into();
                self.sv.bam_tre();
                self.sv.chon_che_do("khoa");
                self.kb.cho_khi(|d| d.sv.lop.kho.che_do() == "khoa");
            }
            14 => self.kb.cho(0.3),
            15 => {
                self.sv.bam_so_sanh();
                self.kb.cho(0.3);
            }
            16 => self.kb.cho_khi(|d| !d.sv.dang_mo_phong),
            17 => {
                for d in bang_so_sanh(&self.sv.hang_so_sanh) {
                    log(&d);
                }
                for cau in self.sv.ket_luan.split(". ") {
                    log(&format!("   {}", cau.trim_end_matches('.')));
                }
                self.kb.cho(0.5);
            }
            18 => {
                log("6. Đặt lại: xe ghế ngồi 50 chỗ, giữ chỗ 6 s; An giữ 05, 06 Sài Gòn → Bảo Lộc (không thanh toán)");
                self.sv.tn.dat_tham_so("ttl_s", "6");
                self.sv.tn.dat_tham_so("loai_xe", "ghe");
                self.sv.bam_dat_lai();
                self.kb.cho_khi(|d| {
                    d.ds.iter().all(|cs| {
                        cs.nd.tt.as_ref().is_some_and(|t| {
                            lay_f64(t, "ttl_s") == Some(6.0) && lay_str(t, "loai_xe") == Some("ghe") && t.get("dem").and_then(|x| x.get("ban")).and_then(Value::as_i64) == Some(0)
                        })
                    })
                });
            }
            19 => self.kb.cho(0.8),
            20 => {
                self.doan(0, 0, 1);
                self.bam(0, "05");
                self.bam(0, "06");
                self.giu(0);
                self.kb.cho_khi(|d| !d.ds[0].ud.giu.is_empty());
            }
            21 => self.kb.cho(0.5),
            22 => {
                self.doan(1, 0, 2);
                self.bam(1, "06");
                log(&format!("   Bình bấm 06 (Sài Gòn → Đà Lạt): {}", self.ds[1].ud.thong_bao.0));
                self.kb.cho_khi(|d| d.ds[0].ud.giu.is_empty());
            }
            23 => {
                log(&format!("   hết hạn – An: {}", self.ds[0].ud.thong_bao.0));
                self.kb.cho(0.3);
            }
            24 => {
                log("   Bình giữ 05, 06 Sài Gòn → Đà Lạt rồi thanh toán; Chi mua 05 Đà Lạt → Nha Trang");
                self.bam(1, "05");
                self.bam(1, "06");
                self.doan(2, 2, 3);
                self.bam(2, "05");
                self.giu(1);
                self.giu(2);
                self.kb.cho_khi(|d| !d.ds[1].ud.giu.is_empty() && !d.ds[2].ud.giu.is_empty());
            }
            25 => self.kb.cho(0.4),
            26 => {
                self.thanh_toan(1);
                self.thanh_toan(2);
                self.kb.cho_khi(|d| !d.ds[1].ud.ve.is_empty() && !d.ds[2].ud.ve.is_empty());
            }
            27 => self.kb.cho(0.4),
            28 => {
                log("   An giữ 10, 11 Sài Gòn → Nha Trang; Chi chọn 21");
                self.doan(0, 0, 3);
                self.bam(0, "10");
                self.bam(0, "11");
                self.giu(0);
                self.doan(2, 0, 3);
                self.bam(2, "21");
                self.kb.cho_khi(|d| !d.ds[0].ud.giu.is_empty());
            }
            29 => self.kb.cho(1.5),
            30 => {
                self.in_cua_so();
                log(&format!("   An: đồng hồ giữ chỗ {}", self.ds[0].ud.dong_ho.0));
                log(&format!("   máy chủ: {}", self.tom_tat()));
                self.in_server();
                log("7. Chụp 4 cửa sổ");
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            31 => {
                let c = |i: usize| self.chup.cat(self.khung[i], self.ppp);
                let anh = ghep_anh(&[vec![c(0), c(1)], vec![c(2), c(3)]]);
                ghi_va_bao(&self.duong("ket_qua_gui.png"), &anh, log);
                log("8. Máy chủ: STOP SERVER");
                self.sv.bam_stop();
                self.kb.cho_khi(|d| d.sv.start_bat());
            }
            32 => self.kb.cho(0.5),
            33 => {
                log(&format!("   Bình: {}", self.ds[1].nd.thanh_tt));
                for cs in &mut self.ds {
                    cs.dong();
                }
                self.sv.dong();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                self.kb.cho(1e9);
            }
            _ => {}
        }
    }
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        vua_man_hinh(ctx, 1900.0, 1600.0);
        self.ppp = ctx.pixels_per_point();
        self.chup.nhan(ctx);
        // ---- vẽ 4 cửa sổ con (vị trí tính theo kích thước thật của khung lần vẽ trước)
        let k = self.khung;
        let duoi = k[0].max.y.max(k[1].max.y) + 12.0;
        let vi_tri = [egui::pos2(4.0, 4.0), egui::pos2(k[0].max.x + 12.0, 4.0), egui::pos2(4.0, duoi), egui::pos2(k[2].max.x + 12.0, duoi)];
        egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::from_gray(0xd0))).show(ctx, |_| {});
        let ten: Vec<String> = std::iter::once(tieu_de()).chain(self.ds.iter().map(|c| c.tieu_de())).collect();
        for i in 0..4 {
            let r = egui::Window::new(&ten[i])
                .id(egui::Id::new(("cs", i)))
                .current_pos(vi_tri[i])
                .resizable(false)
                .collapsible(false)
                .movable(false)
                .constrain(false)
                .frame(egui::Frame::window(&ctx.style()).fill(NEN).rounding(0.0))
                .show(ctx, |ui| {
                    if i == 0 {
                        ui.set_width(1040.0);
                        self.sv.ui(ui);
                    } else {
                        ui.set_width(780.0);
                        self.ds[i - 1].ui(ui);
                    }
                });
            if let Some(r) = r {
                self.khung[i] = r.response.rect;
            }
        }
        // ---- kịch bản
        let mut kb = std::mem::take(&mut self.kb);
        let toi = kb.toi_luot(self);
        self.kb = kb;
        match toi {
            Some(true) if self.kb.buoc <= BUOC_CUOI => self.buoc(ctx),
            None => {
                log("Kịch bản chờ quá lâu – dừng");
                self.kb.buoc = BUOC_CUOI;
                self.buoc(ctx);
            }
            _ => {}
        }
        // đang mô phỏng tải: vẽ thưa lại để máy chủ, bộ mô phỏng (cùng tiến trình) có CPU
        let nhip = if self.sv.dang_mo_phong { 300 } else { 50 };
        ctx.request_repaint_after(std::time::Duration::from_millis(nhip));
    }
}

/// demo_gui [host] [cổng] [thư mục ảnh]
pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(CONG_GUI);
    let thu_muc = args.get(2).cloned().unwrap_or_else(|| ".".into());
    let app = DemoApp {
        sv: tao_cua_so(&host, cong),
        ds: ["An", "Bình", "Chi"].iter().map(|t| CuaSoNguoiDung::moi(t, &host, cong)).collect(),
        kb: KichBan::default(),
        chup: MayChup::default(),
        khung: [egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1060.0, 700.0)); 4],
        ppp: 1.0,
        thu_muc,
        duoc: Vec::new(),
        truoc: Vec::new(),
    };
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1900.0, 1600.0]).with_title("Demo giao diện MR6.14 – máy chủ và 3 ứng dụng đặt vé xe khách"),
        ..Default::default()
    };
    eframe::run_native(
        "Demo MR6.14",
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
