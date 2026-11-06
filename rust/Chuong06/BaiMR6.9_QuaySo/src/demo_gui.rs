//! Bài mở rộng MR6.9 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).
//!
//! Một tiến trình, một cửa sổ egui chứa bốn "cửa sổ con": bảng điều khiển máy chủ và ba vòng quay: An,
//! Bình và "Bình (máy 2)" – cùng một người mở trên hai thiết bị. Kịch bản gọi đúng các hàm mà nút gọi,
//! theo từng bước (chờ một số giây hoặc chờ tới khi điều kiện đúng – vòng vẽ của egui vẫn chạy trong lúc
//! chờ), in `[demo] …`:
//!   1–2. chế độ khoa, 1 món quà, p = 1: cả ba cùng bấm QUAY -> đúng một người trúng, thiết bị thứ hai
//!        của Bình bị từ chối "đã quay";
//!   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> cả ba "trúng" một món quà -> SAI PHẠM
//!        (thừa quà, quay 2 lần); chụp ket_qua_gui_tranh_chap.png;
//!   4.   So sánh 4 chế độ (10 000 người, p = 1/20, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
//!   5–6. 10 quà, p = 0.8: An, Bình quay; Bình (máy 2) quay -> "đã quay"; bắn đồng loạt 500 người ->
//!        danh sách người trúng trực tiếp; chụp 4 cửa sổ ghép thành ket_qua_gui.png.
//!
//! Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" mr6_9_quay_so demo_gui [host] [cổng]

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::{CONG_GUI, TEN_QUA};
use crate::server_gui::{tao_cua_so, tieu_de, CuaSoServerQuaySo};
use crate::vong_quay_gui::VongQuay;
use eframe::egui;
use serde_json::Value;

fn log(text: &str) {
    in_dong("demo", text);
}

const TEN_HIEN: [&str; 3] = ["An", "Bình", "Bình (máy 2)"];

struct DemoApp {
    sv: CuaSoServerQuaySo,
    ds: Vec<CuaSoNguoiDung<VongQuay>>,
    kb: KichBan<DemoApp>,
    chup: MayChup,
    khung: [egui::Rect; 4], // vị trí 4 cửa sổ con (để cắt ảnh)
    ppp: f32,
    thu_muc: String,
}

impl DemoApp {
    fn bam_quay(&mut self, i: usize) {
        let cs = &mut self.ds[i];
        if cs.ud.nut_bat {
            cs.ud.quay(&mut cs.nd);
        }
    }

    fn cung_bam(&mut self) {
        for i in 0..3 {
            self.bam_quay(i);
        }
    }

    fn dat_lai(&mut self, so_qua: &str, p: &str) {
        self.sv.tn.dat_tham_so(so_qua, p);
        self.sv.bam_dat_lai();
    }

    fn in_cua_so(&self) {
        for (i, cs) in self.ds.iter().enumerate() {
            log(&format!("   {}: {}", TEN_HIEN[i], cs.ud.thong_bao.0));
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

    fn quay_xong(&self) -> bool {
        self.ds.iter().all(|cs| cs.ud.pha.is_none())
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
                log("   An, Bình, Bình (máy 2): Kết nối");
                for cs in &mut self.ds {
                    cs.bam_ket_noi();
                }
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.is_some()));
            }
            3 => {
                log("2. Đặt lại: 1 món quà, p = 1 (lượt đầu chắc trúng)");
                self.dat_lai("1", "1");
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_i64(t, "so_qua")) == Some(1)));
            }
            4 => self.kb.cho(0.5),
            5 => {
                log("   An, Bình, Bình (máy 2) CÙNG bấm QUAY");
                self.cung_bam();
                self.kb.cho(0.3);
            }
            6 => self.kb.cho_khi(|d| d.quay_xong()),
            7 => {
                self.in_cua_so();
                self.in_server();
                log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; Đặt lại");
                self.sv.o_tre = "20".into();
                self.sv.bam_tre();
                self.dat_lai("1", "1");
                self.sv.chon_che_do("khong_khoa");
                self.kb.cho_khi(|d| d.sv.lop.kho.che_do() == "khong_khoa");
            }
            8 => self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_str(t, "che_do")) == Some("khong_khoa") && cs.ud.nut_bat)),
            9 => self.kb.cho(0.5),
            10 => {
                log("   An, Bình, Bình (máy 2) CÙNG bấm QUAY");
                self.cung_bam();
                self.kb.cho(0.3);
            }
            11 => self.kb.cho_khi(|d| d.quay_xong()),
            12 => self.kb.cho(0.5),
            13 => {
                self.in_cua_so();
                self.in_server();
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            14 => {
                let anh = ghep_anh(&[vec![self.chup.cat(self.khung[0], self.ppp), self.chup.cat(self.khung[3], self.ppp)]]);
                ghi_va_bao(&self.duong("ket_qua_gui_tranh_chap.png"), &anh, log);
                log("4. Máy chủ: khoa, trễ 0–2 ms; 10 quà, p = 0.05 (1/20);");
                log("   So sánh 4 chế độ, 10000 người");
                self.sv.o_tre = "2".into();
                self.sv.bam_tre();
                self.dat_lai("10", "0.05");
                self.sv.chon_che_do("khoa");
                self.kb.cho_khi(|d| d.sv.lop.kho.che_do() == "khoa");
            }
            15 => {
                self.sv.o_n = "10000".into();
                self.kb.cho(0.3);
            }
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
                log("5. Đặt lại: 10 quà, p = 0.8; An quay, rồi Bình quay");
                self.dat_lai("10", "0.8");
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_f64(t, "p")) == Some(0.8) && cs.ud.nut_bat));
            }
            20 => self.kb.cho(0.5),
            21 => {
                self.bam_quay(0);
                self.kb.cho(0.4);
            }
            22 => {
                self.bam_quay(1);
                self.kb.cho(0.3);
            }
            23 => self.kb.cho_khi(|d| d.ds[0].ud.pha.is_none() && d.ds[1].ud.pha.is_none()),
            24 => {
                log("   Bình (máy 2) bấm QUAY");
                self.bam_quay(2);
                self.kb.cho(0.3);
            }
            25 => self.kb.cho_khi(|d| d.ds[2].ud.pha.is_none()),
            26 => {
                self.in_cua_so();
                log("6. Bắn đồng loạt 500 người (chế độ khoa)");
                self.sv.o_n = "500".into();
                self.sv.bam_ban();
                self.kb.cho(0.3);
            }
            27 => self.kb.cho_khi(|d| !d.sv.dang_mo_phong),
            28 => self.kb.cho(0.8),
            29 => {
                let hang = self.sv.hang_so_sanh.clone();
                self.sv.xong_mo_phong(hang); // bảng: hiện lại kết quả so sánh 4 chế độ
                self.in_server();
                log("   Người trúng gần nhất (cửa sổ An):");
                let tt = self.ds[0].nd.tt.clone().unwrap_or_default();
                for x in tt.get("nguoi_trung").and_then(Value::as_array).into_iter().flatten().take(4) {
                    let q = x.get("qua").and_then(Value::as_u64).unwrap_or(0) as usize;
                    log(&format!("     {} {:<6} {}", so_us(x.get("gio")), x.get("nguoi").map(py_str).unwrap_or_default(), TEN_QUA.get(q).copied().unwrap_or("?")));
                }
                log("7. Chụp 4 cửa sổ");
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            30 => {
                let c = |i: usize| self.chup.cat(self.khung[i], self.ppp);
                let anh = ghep_anh(&[vec![c(0), c(1)], vec![c(2), c(3)]]);
                ghi_va_bao(&self.duong("ket_qua_gui.png"), &anh, log);
                log("8. Máy chủ: STOP SERVER");
                self.sv.bam_stop();
                self.kb.cho_khi(|d| d.sv.start_bat());
            }
            31 => self.kb.cho(0.5),
            32 => {
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

const BUOC_CUOI: usize = 32;

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        vua_man_hinh(ctx, 1800.0, 1500.0);
        self.ppp = ctx.pixels_per_point();
        self.chup.nhan(ctx);
        // ---- vẽ 4 cửa sổ con (vị trí tính theo kích thước thật của khung lần vẽ trước)
        let k = self.khung;
        let duoi = k[0].max.y.max(k[1].max.y) + 12.0;
        let vi_tri = [egui::pos2(4.0, 4.0), egui::pos2(k[0].max.x + 12.0, 4.0), egui::pos2(4.0, duoi), egui::pos2(k[2].max.x + 12.0, duoi)];
        egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::from_gray(0xd0))).show(ctx, |_| {});
        let ten: Vec<String> = std::iter::once(tieu_de()).chain(TEN_HIEN.iter().map(|t| format!("Vòng quay may mắn – {t}"))).collect();
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
        let nhip = if self.sv.dang_mo_phong { 300 } else { 30 };
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
        ds: ["An", "Bình", "Bình"].iter().map(|t| CuaSoNguoiDung::moi(t, &host, cong)).collect(),
        kb,
        chup: MayChup::default(),
        khung: [egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1060.0, 700.0)); 4],
        ppp: 1.0,
        thu_muc,
    };
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1800.0, 1500.0]).with_title("Demo giao diện MR6.9 – máy chủ và 3 vòng quay"),
        ..Default::default()
    };
    eframe::run_native(
        "Demo MR6.9",
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
