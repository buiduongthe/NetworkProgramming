//! Bài mở rộng MR6.7 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).
//!
//! Một tiến trình, một cửa sổ egui chứa bốn "cửa sổ con": bảng điều khiển máy chủ và ba ứng dụng gọi xe
//! An, Bình, Chi. Kịch bản gọi đúng các hàm mà nút gọi, theo từng bước (chờ một số giây hoặc chờ tới khi
//! điều kiện đúng – vòng vẽ của egui vẫn chạy trong lúc chờ), in `[demo] …`:
//!   1–3. chế độ khoa, 1 xe: An, Bình, Chi CÙNG bấm "ĐẶT XE" -> một người được xe, hai người "Hết chỗ";
//!        người được xe hủy chuyến;
//!   4.   chế độ khong_khoa, trễ 0–20 ms: cả ba CÙNG bấm -> cả ba "Đặt thành công – tài xế Hùng", rồi hai
//!        người thấy "Tài xế Hùng đã nhận khách khác!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
//!   5.   đặt lại 3 xe, So sánh 4 chế độ (mô phỏng 1000 người, 200 kết nối) -> bảng, khong_khoa tô đỏ;
//!   6.   đặt lại (3 xe): An, Bình, Chi cùng đặt -> mỗi người một xe, xe chạy tới điểm đón;
//!        chụp 4 cửa sổ ghép thành ket_qua_gui.png.
//!
//! Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" mr6_7_dat_xe demo_gui [host] [cổng]

use crate::chung::*;
use crate::dat_xe_gui::{Trang, UngDungGoiXe};
use crate::gui_chung::*;
use crate::may_chu::CONG_GUI;
use crate::server_gui::{tao_cua_so, tieu_de, CuaSoServerDatXe};
use eframe::egui;
use serde_json::Value;

const BUOC_CUOI: usize = 28;

fn log(text: &str) {
    in_dong("demo", text);
}

struct DemoApp {
    sv: CuaSoServerDatXe,
    ds: Vec<CuaSoNguoiDung<UngDungGoiXe>>,
    kb: KichBan<DemoApp>,
    chup: MayChup,
    khung: [egui::Rect; 4], // vị trí 4 cửa sổ con (để cắt ảnh)
    ppp: f32,
    thu_muc: String,
    duoc: usize,
    nan_nhan: usize,
}

impl DemoApp {
    fn in_cua_so(&self) {
        for cs in &self.ds {
            log(&format!("   {}: {}", cs.nd.ten, cs.ud.nhan_tt.0));
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

    /// Ba lần bấm liền nhau trên luồng giao diện.
    fn cung_bam(&mut self) {
        for cs in &mut self.ds {
            cs.ud.dat_xe(&mut cs.nd);
        }
    }

    fn duong(&self, ten: &str) -> String {
        format!("{}/{ten}", self.thu_muc)
    }

    fn het_tim(d: &DemoApp) -> bool {
        d.ds.iter().all(|cs| cs.ud.trang != Trang::DangTim)
    }

    /// Một bước của kịch bản; mỗi bước tự đặt điều kiện chờ cho bước sau.
    fn buoc(&mut self, ctx: &egui::Context) {
        let n = self.kb.buoc;
        self.kb.buoc += 1;
        match n {
            0 => self.kb.cho(0.5),
            1 => {
                log("1. Máy chủ: START SERVER (chế độ khoa, 1 xe)");
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
                log("2. An, Bình, Chi CÙNG bấm 'ĐẶT XE'");
                self.cung_bam();
                self.kb.cho_khi(Self::het_tim);
            }
            5 => self.kb.cho(0.5),
            6 => {
                self.in_cua_so();
                self.in_server();
                self.duoc = self.ds.iter().position(|cs| cs.ud.trang == Trang::DaDat).unwrap_or(0);
                self.kb.cho(1.0);
            }
            7 => {
                let cs = &mut self.ds[self.duoc];
                log(&format!("3. {} bấm 'Hủy chuyến'", cs.nd.ten));
                cs.ud.huy(&mut cs.nd);
                self.kb.cho_khi(|d| d.ds[d.duoc].ud.trang == Trang::Xong);
            }
            8 => self.kb.cho(0.5),
            9 => {
                let cs = &self.ds[self.duoc];
                log(&format!("   {}: {}", cs.nd.ten, cs.ud.nhan_tt.0));
                log(&format!("   máy chủ: {}", self.tom_tat()));
                log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)");
                self.sv.o_tre = "20".into();
                self.sv.bam_tre();
                self.sv.chon_che_do("khong_khoa");
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_str(t, "che_do")) == Some("khong_khoa")));
            }
            10 => self.kb.cho(0.5),
            11 => {
                log("   An, Bình, Chi CÙNG bấm 'ĐẶT XE'");
                self.cung_bam();
                self.kb.cho_khi(Self::het_tim);
            }
            12 => {
                for cs in &self.ds {
                    log(&format!("   {} nhận: {}", cs.nd.ten, cs.ud.nhan_tt.0));
                }
                self.kb.cho(0.6);
            }
            13 => {
                log("   …sau cap_nhat của máy chủ:");
                self.in_cua_so();
                self.in_server();
                let tt = self.sv.lop.trang_thai();
                let khach = tt.get("xe").and_then(Value::as_array).and_then(|a| a.first()).map(|x| so_us(x.get("khach"))).unwrap_or_default();
                log(&format!("   xe 1 chở: {khach}"));
                self.nan_nhan = self.ds.iter().position(|cs| cs.ud.trang == Trang::MatXe).unwrap_or(1);
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            14 => {
                let anh = ghep_anh(&[vec![self.chup.cat(self.khung[0], self.ppp), self.chup.cat(self.khung[1 + self.nan_nhan], self.ppp)]]);
                ghi_va_bao(&self.duong("ket_qua_gui_tranh_chap.png"), &anh, log);
                log("5. Máy chủ: khoa, trễ 0–2 ms, đặt lại 3 xe");
                log("   So sánh 4 chế độ (1000 người)");
                self.sv.o_tre = "2".into();
                self.sv.bam_tre();
                self.sv.chon_che_do("khoa");
                self.sv.tn.dat_so_xe("3");
                self.sv.bam_dat_lai();
                self.sv.o_n = "1000".into();
                self.kb.cho_khi(|d| d.sv.lop.kho.che_do() == "khoa");
            }
            15 => self.kb.cho(0.3),
            16 => {
                self.sv.bam_so_sanh();
                self.kb.cho(0.3);
            }
            17 => self.kb.cho_khi(|d| !d.sv.dang_mo_phong),
            18 => {
                for d in bang_so_sanh(&self.sv.hang_so_sanh) {
                    log(&d);
                }
                for cau in self.sv.ket_luan.split(". ") {
                    log(&format!("   {}", cau.trim_end_matches('.')));
                }
                self.kb.cho(0.5);
            }
            19 => {
                log("6. Đặt lại (3 xe); An, Bình, Chi CÙNG bấm 'ĐẶT XE'");
                self.sv.bam_dat_lai();
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_i64(t, "trong")) == Some(3)));
            }
            20 => self.kb.cho(0.8),
            21 => {
                self.cung_bam();
                self.kb.cho_khi(Self::het_tim);
            }
            22 => self.kb.cho(1.6),
            23 => {
                self.in_cua_so();
                for cs in &self.ds {
                    if let Some(c) = &cs.ud.chuyen {
                        let tx = so_us(c.get("tai_xe"));
                        log(&format!("   {}: {} {}, đến sau {} phút", cs.nd.ten, tx.split_whitespace().next().unwrap_or(""), so_us(c.get("bien_so")), so_us(c.get("phut"))));
                    }
                }
                log(&format!("   máy chủ: {}", self.tom_tat()));
                log("7. Chụp 4 cửa sổ");
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            24 => {
                let c = |i: usize| self.chup.cat(self.khung[i], self.ppp);
                let anh = ghep_anh(&[vec![c(0), c(1)], vec![c(2), c(3)]]);
                ghi_va_bao(&self.duong("ket_qua_gui.png"), &anh, log);
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.ud.hoat_hinh.is_none() || cs.ud.da_toi()));
            }
            25 => self.kb.cho(0.3),
            26 => {
                self.in_cua_so();
                log("8. Máy chủ: STOP SERVER");
                self.sv.bam_stop();
                self.kb.cho_khi(|d| d.sv.start_bat());
            }
            27 => self.kb.cho(0.5),
            28 => {
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
                        ui.set_width(700.0);
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
        let nhip = if self.sv.dang_mo_phong { 300 } else { 40 };
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
        duoc: 0,
        nan_nhan: 1,
    };
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1800.0, 1400.0]).with_title("Demo giao diện MR6.7 – máy chủ và 3 ứng dụng gọi xe"),
        ..Default::default()
    };
    eframe::run_native(
        "Demo MR6.7",
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
