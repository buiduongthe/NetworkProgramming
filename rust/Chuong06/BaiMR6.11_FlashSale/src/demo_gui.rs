//! Bài mở rộng MR6.11 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).
//!
//! Một tiến trình, một cửa sổ egui chứa bốn "cửa sổ con": bảng điều khiển máy chủ và ba trang flash sale
//! của An, Bình, Chi. Kịch bản gọi đúng các hàm mà nút gọi, theo từng bước (chờ một số giây hoặc chờ tới
//! khi điều kiện đúng – vòng vẽ của egui vẫn chạy trong lúc chờ), in `[demo] …`:
//!   1–2. chế độ khoa, kho 3 sản phẩm, mở bán sau 3 s: nút MUA NGAY khóa trong lúc đếm ngược; vừa mở,
//!        An mua 2, Bình 1, Chi 2 cùng lúc -> tổng bán đúng 3, người đến sau bị từ chối;
//!   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> cả ba "đặt hàng thành công" 5 sản phẩm từ
//!        kho 3 -> SAI PHẠM (tồn âm, lệch sổ); chụp ket_qua_gui_tranh_chap.png;
//!   4.   So sánh 4 chế độ (10 000 người, kho 100, 200 kết nối, trễ 0–1 ms) -> bảng, dòng khong_khoa đỏ;
//!   5–6. đợt mới: kho 100, mở sau 4 s (chụp trang của Chi lúc đếm ngược); mở bán: An mua 2, 50 người
//!        ảo cùng mua, Bình mua 2 -> thanh "SẮP CHÁY HÀNG"; chụp, ghép thành ket_qua_gui.png.
//!
//! Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" mr6_11_flash_sale demo_gui [host] [cổng]

use crate::chung::*;
use crate::flash_sale_gui::TrangFlashSale;
use crate::gui_chung::*;
use crate::may_chu::CONG_GUI;
use crate::server_gui::{tao_cua_so, tieu_de, CuaSoServerFlashSale};
use eframe::egui;
use serde_json::Value;

fn log(text: &str) {
    in_dong("demo", text);
}

struct DemoApp {
    sv: CuaSoServerFlashSale,
    ds: Vec<CuaSoNguoiDung<TrangFlashSale>>,
    kb: KichBan<DemoApp>,
    chup: MayChup,
    anh_chi: Option<Anh>, // trang của Chi lúc đếm ngược (chụp sớm)
    khung: [egui::Rect; 4], // vị trí 4 cửa sổ con (để cắt ảnh)
    ppp: f32,
    thu_muc: String,
}

impl DemoApp {
    fn mua(&mut self, i: usize, sl: i64) {
        let cs = &mut self.ds[i];
        cs.ud.mua(&mut cs.nd, Some(sl));
    }

    fn dat_lai(&mut self, ton_dau: i64, mo_sau_s: f64) {
        self.sv.tn.dat_tham_so(ton_dau, mo_sau_s);
        self.sv.bam_dat_lai();
    }

    fn dat_tre(&mut self, ms: &str) {
        self.sv.o_tre = ms.into();
        self.sv.bam_tre();
    }

    fn mo_het(&self) -> bool {
        self.ds.iter().all(|cs| cs.ud.nut_mua(&cs.nd).1)
    }

    fn xong(&self) -> bool {
        self.ds.iter().all(|cs| !cs.ud.dang_cho)
    }

    fn in_cua_so(&self, chi: &[usize]) {
        for &i in chi {
            let cs = &self.ds[i];
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
        log(&format!("   {}", s("tom_tat")));
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
                log("1. Máy chủ: START SERVER (chế độ khoa)");
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
            3 => {
                log("2. Đặt lại: kho 3 sản phẩm, mở bán sau 3 s");
                self.dat_lai(3, 3.0);
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_i64(t, "ton_dau")) == Some(3)));
            }
            4 => self.kb.cho(0.5),
            5 => {
                let an = &self.ds[0];
                let (chu, bat, _) = an.ud.nut_mua(&an.nd);
                log(&format!("   An thấy: {}, nút '{chu}' ({})", an.ud.dong_ho(&an.nd).1, if bat { "normal" } else { "disabled" }));
                self.kb.cho_khi(|d| d.mo_het());
            }
            6 => {
                log("   Mở bán: An (2), Bình (1), Chi (2) CÙNG bấm MUA NGAY");
                for (i, sl) in [(0, 2), (1, 1), (2, 2)] {
                    self.mua(i, sl);
                }
                self.kb.cho_khi(|d| d.xong());
            }
            7 => self.kb.cho(0.5),
            8 => {
                self.in_cua_so(&[0, 1, 2]);
                self.in_server();
                log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; đặt lại");
                self.dat_tre("20");
                self.dat_lai(3, 2.0);
                self.sv.chon_che_do("khong_khoa");
                self.kb.cho_khi(|d| d.sv.lop.kho.che_do() == "khong_khoa");
            }
            9 => self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_str(t, "che_do")) == Some("khong_khoa") && cs.ud.nut_mua(&cs.nd).1)),
            10 => {
                log("   Mở bán: An (2), Bình (1), Chi (2) CÙNG bấm MUA NGAY");
                for (i, sl) in [(0, 2), (1, 1), (2, 2)] {
                    self.mua(i, sl);
                }
                self.kb.cho_khi(|d| d.xong());
            }
            11 => self.kb.cho(0.6),
            12 => {
                self.in_cua_so(&[0, 1, 2]);
                self.in_server();
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            13 => {
                let anh = ghep_anh(&[vec![self.chup.cat(self.khung[0], self.ppp), self.chup.cat(self.khung[3], self.ppp)]]);
                ghi_va_bao(&self.duong("ket_qua_gui_tranh_chap.png"), &anh, log);
                log("4. Máy chủ: khoa, trễ 0–1 ms, kho 100; So sánh 4 chế độ");
                self.dat_tre("1");
                self.dat_lai(100, 30.0);
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
                log("5. Đặt lại: kho 100, mở bán sau 4 s");
                self.dat_lai(100, 4.0);
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().is_some_and(|t| lay_i64(t, "ton") == Some(100) && !dung(t, "mo_ban"))));
            }
            19 => self.kb.cho(1.5),
            20 => {
                let chi = &self.ds[2];
                let (nhan, dh, _) = chi.ud.dong_ho(&chi.nd);
                log(&format!("   Chi thấy: {nhan} {dh}, nút '{}'", chi.ud.nut_mua(&chi.nd).0));
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            21 => {
                self.anh_chi = Some(self.chup.cat(self.khung[3], self.ppp));
                self.kb.cho_khi(|d| d.ds[0].ud.nut_mua(&d.ds[0].nd).1);
            }
            22 => {
                log("6. Mở bán: An mua 2; 50 người ảo cùng mua; Bình mua 2");
                self.mua(0, 2);
                self.kb.cho_khi(|d| !d.ds[0].ud.dang_cho);
            }
            23 => {
                self.sv.o_n = "50".into();
                self.sv.bam_ban();
                self.kb.cho(0.3);
            }
            24 => self.kb.cho_khi(|d| !d.sv.dang_mo_phong),
            25 => {
                let hang = self.sv.hang_so_sanh.clone();
                self.sv.xong_mo_phong(hang); // bảng: hiện lại kết quả so sánh 4 chế độ
                self.mua(1, 2);
                self.kb.cho_khi(|d| !d.ds[1].ud.dang_cho);
            }
            26 => self.kb.cho(0.6),
            27 => {
                self.in_cua_so(&[0, 1]);
                self.in_server();
                log("7. Chụp cửa sổ (Chi: lúc đếm ngược)");
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            28 => {
                let c = |i: usize| self.chup.cat(self.khung[i], self.ppp);
                let chi = self.anh_chi.take().unwrap_or_else(|| c(3));
                let anh = ghep_anh(&[vec![c(0), c(1)], vec![c(2), chi]]);
                ghi_va_bao(&self.duong("ket_qua_gui.png"), &anh, log);
                log("8. Máy chủ: STOP SERVER");
                self.sv.bam_stop();
                self.kb.cho_khi(|d| d.sv.start_bat());
            }
            29 => self.kb.cho(0.5),
            30 => {
                log(&format!("   An: {}", self.ds[0].nd.thanh_tt));
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

const BUOC_CUOI: usize = 30;

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        vua_man_hinh(ctx, 1800.0, 1450.0);
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
                        ui.set_width(660.0);
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
    let mut kb = KichBan::default();
    kb.het_gio = 120.0;
    let app = DemoApp {
        sv: tao_cua_so(&host, cong),
        ds: ["An", "Bình", "Chi"].iter().map(|t| CuaSoNguoiDung::moi(t, &host, cong)).collect(),
        kb,
        chup: MayChup::default(),
        anh_chi: None,
        khung: [egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1060.0, 700.0)); 4],
        ppp: 1.0,
        thu_muc,
    };
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1800.0, 1450.0]).with_title("Demo giao diện MR6.11 – máy chủ và 3 trang flash sale"),
        ..Default::default()
    };
    eframe::run_native(
        "Demo MR6.11",
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
