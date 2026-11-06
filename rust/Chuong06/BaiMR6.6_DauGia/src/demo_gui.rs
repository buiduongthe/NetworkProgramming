//! Bài mở rộng MR6.6 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).
//!
//! Một tiến trình, một cửa sổ egui chứa bốn "cửa sổ con": bảng điều khiển máy chủ và ba phòng đấu giá
//! An, Bình, Chi. Kịch bản gọi đúng các hàm mà nút gọi, theo từng bước (chờ một số giây hoặc chờ tới
//! khi điều kiện đúng – vòng vẽ của egui vẫn chạy trong lúc chờ), in `[demo] …`:
//!   1–3. chế độ khoa: Bình và Chi cùng bấm "+1 bước" -> một người bị từ chối "giá đã thay đổi";
//!   4.   chế độ khong_khoa: ba người cùng bấm "+1 bước" -> cả ba cùng "được chấp nhận" -> SAI PHẠM;
//!        chụp ket_qua_gui_tranh_chap.png;
//!   5.   So sánh 4 chế độ (mô phỏng 500 người × 5 lần, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
//!   6–7. phiên 8 s, gia hạn 4 s: trả giá phút cuối -> gia hạn hai lần -> kết thúc, công bố người thắng;
//!        chụp 4 cửa sổ ghép thành ket_qua_gui.png.
//!
//! Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" mr6_6_dau_gia demo_gui [host] [cổng]

use crate::chung::*;
use crate::dau_gia_gui::PhongDauGia;
use crate::gui_chung::*;
use crate::may_chu::{so_nguyen, CONG_GUI};
use crate::server_gui::{tao_cua_so, tieu_de, CuaSoServerDauGia};
use eframe::egui;
use serde_json::Value;

fn log(text: &str) {
    in_dong("demo", text);
}

struct DemoApp {
    sv: CuaSoServerDauGia,
    ds: Vec<CuaSoNguoiDung<PhongDauGia>>,
    kb: KichBan<DemoApp>,
    chup: MayChup,
    khung: [egui::Rect; 4], // vị trí 4 cửa sổ con (để cắt ảnh)
    ppp: f32,
    thu_muc: String,
}

impl DemoApp {
    fn an(&mut self) -> &mut CuaSoNguoiDung<PhongDauGia> {
        &mut self.ds[0]
    }

    fn bam_buoc(&mut self, i: usize, n: i64) {
        let cs = &mut self.ds[i];
        cs.ud.tang(&mut cs.nd, n);
    }

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
                log("1. Máy chủ: START SERVER (chế độ khoa, phiên 120 s)");
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
                log("2. An bấm '+1 bước'");
                self.bam_buoc(0, 1);
                self.kb.cho(0.8);
            }
            5 => {
                log("3. Bình và Chi CÙNG bấm '+1 bước' (cùng giá)");
                self.bam_buoc(1, 1);
                self.bam_buoc(2, 1);
                self.kb.cho(0.8);
            }
            6 => {
                self.in_cua_so();
                self.in_server();
                log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)");
                self.sv.o_tre = "20".into();
                self.sv.bam_tre();
                self.sv.chon_che_do("khong_khoa");
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_str(t, "che_do")) == Some("khong_khoa")));
            }
            7 => self.kb.cho(0.5),
            8 => {
                log("   An, Bình, Chi CÙNG bấm '+1 bước'");
                for i in 0..3 {
                    self.bam_buoc(i, 1);
                }
                self.kb.cho(0.8);
            }
            9 => {
                self.in_cua_so();
                self.in_server();
                log("   Lịch sử trả giá (mới nhất trước):");
                for x in self.sv.lop.trang_thai().get("lich_su").and_then(Value::as_array).into_iter().flatten().take(4) {
                    log(&format!("     {} {:<5} {}", so_us(x.get("gio")), x.get("nguoi").map(py_str).unwrap_or_default(), tien(so_nguyen(x.get("gia")))));
                }
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            10 => {
                let anh = ghep_anh(&[vec![self.chup.cat(self.khung[0], self.ppp), self.chup.cat(self.khung[2], self.ppp)]]);
                ghi_va_bao(&self.duong("ket_qua_gui_tranh_chap.png"), &anh, log);
                log("5. Máy chủ: khoa, trễ 0–2 ms; So sánh 4 chế độ");
                self.sv.o_tre = "2".into();
                self.sv.bam_tre();
                self.sv.chon_che_do("khoa");
                self.sv.o_n = "500".into();
                self.kb.cho_khi(|d| d.sv.lop.kho.che_do() == "khoa");
            }
            11 => self.kb.cho(0.3),
            12 => {
                self.sv.bam_so_sanh();
                self.kb.cho(0.3);
            }
            13 => self.kb.cho_khi(|d| !d.sv.dang_mo_phong),
            14 => {
                for d in bang_so_sanh(&self.sv.hang_so_sanh) {
                    log(&d);
                }
                for cau in self.sv.ket_luan.split(". ") {
                    log(&format!("   {}", cau.trim_end_matches('.')));
                }
                self.kb.cho(0.5);
            }
            15 => {
                log("6. Đặt lại: phiên 8 s, gia hạn 4 s; An '+1 bước'");
                self.sv.tn.dat_tham_so("thoi_gian_s", "8");
                self.sv.tn.dat_tham_so("gia_han_s", "4");
                self.sv.bam_dat_lai();
                self.kb.cho(1.0);
            }
            16 => {
                self.bam_buoc(0, 1);
                self.kb.cho(4.2);
            }
            17 => {
                log(&format!("   Bình '+2 bước' khi còn {}", self.ds[0].ud.dong_ho.0));
                self.bam_buoc(1, 2);
                self.kb.cho(0.5);
            }
            18 => {
                let an = &self.ds[0].ud;
                log(&format!("   -> đồng hồ An: {} {}", an.dong_ho.0, an.gia_han));
                self.kb.cho_khi(|d| d.ds[0].ud.het_han.is_some_and(|h| h - bay_gio() < 0.8));
            }
            19 => {
                log(&format!("7. An '+1 bước' khi còn {}", self.an().ud.dong_ho.0));
                self.bam_buoc(0, 1);
                self.kb.cho(0.5);
            }
            20 => {
                let binh = &self.ds[1].ud;
                log(&format!("   -> đồng hồ Bình: {} {}", binh.dong_ho.0, binh.gia_han));
                self.kb.cho_khi(|d| d.ds[0].nd.tt.as_ref().is_some_and(|t| dung(t, "ket_thuc")));
            }
            21 => self.kb.cho(0.8),
            22 => {
                self.in_cua_so();
                log(&format!("   máy chủ: {}", so_us(self.sv.lop.thong_ke_day_du().get("tom_tat"))));
                log("8. Chụp 4 cửa sổ");
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            23 => {
                let c = |i: usize| self.chup.cat(self.khung[i], self.ppp);
                let anh = ghep_anh(&[vec![c(0), c(1)], vec![c(2), c(3)]]);
                ghi_va_bao(&self.duong("ket_qua_gui.png"), &anh, log);
                log("9. Máy chủ: STOP SERVER");
                self.sv.bam_stop();
                self.kb.cho_khi(|d| d.sv.start_bat());
            }
            24 => self.kb.cho(0.5),
            25 => {
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
                        ui.set_width(690.0);
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
            Some(true) if self.kb.buoc <= 25 => self.buoc(ctx),
            None => {
                log("Kịch bản chờ quá lâu – dừng");
                self.kb.buoc = 25;
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
    };
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1800.0, 1400.0]).with_title("Demo giao diện MR6.6 – máy chủ và 3 phòng đấu giá"),
        ..Default::default()
    };
    eframe::run_native(
        "Demo MR6.6",
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
