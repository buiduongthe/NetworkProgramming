//! Bài mở rộng MR6.12 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).
//!
//! Một tiến trình, một cửa sổ egui chứa bốn "cửa sổ con": bảng điều khiển máy chủ và ba ứng dụng ngân
//! hàng An (TK01), Bình (TK02), Chi (TK03). Kịch bản gọi đúng các hàm mà nút gọi, theo từng bước (chờ một
//! số giây hoặc chờ tới khi điều kiện đúng – vòng vẽ của egui vẫn chạy trong lúc chờ), in `[demo] …`:
//!   1–2. chế độ khoa (khóa theo thứ tự mã): An→Bình và Bình→An CÙNG LÚC -> cả hai thành công;
//!   3.   bật "khóa ngây thơ" + Đặt lại, trễ 0–50 ms: An→Bình và Bình→An cùng lúc -> KHÓA CHẾT, sau 2 s
//!        một giao dịch bị hủy ("khóa chết: …"), giao dịch kia đi tiếp; chụp ket_qua_gui_khoa_chet.png;
//!   4.   tắt "khóa ngây thơ" + Đặt lại: lặp lại -> cả hai thành công ngay;
//!   5.   chế độ khong_khoa, trễ 0–20 ms: An→Bình, Bình→Chi, Chi→An cùng lúc -> TỔNG TIỀN LỆCH (đỏ);
//!        chụp ket_qua_gui_tranh_chap.png;
//!   6.   So sánh 4 chế độ (mô phỏng 500 người × 5 lần, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
//!   7.   Đặt lại, An chuyển cho Chi; chụp 4 cửa sổ ghép thành ket_qua_gui.png.
//!
//! Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" mr6_12_chuyen_khoan demo_gui [host] [cổng]

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::CONG_GUI;
use crate::ngan_hang_gui::UngDungNganHang;
use crate::server_gui::{tao_cua_so, tieu_de, CuaSoServerNganHang};
use eframe::egui;
use serde_json::Value;

fn log(text: &str) {
    in_dong("demo", text);
}

struct DemoApp {
    sv: CuaSoServerNganHang,
    ds: Vec<CuaSoNguoiDung<UngDungNganHang>>,
    kb: KichBan<DemoApp>,
    chup: MayChup,
    khung: [egui::Rect; 4], // vị trí 4 cửa sổ con (để cắt ảnh)
    ppp: f32,
    thu_muc: String,
    dem: u64, // số kết quả đã nhận trước lượt chuyển đang chờ
}

impl DemoApp {
    /// Chọn người nhận, điền số tiền, bấm "Chuyển tiền" – như người dùng.
    fn chuyen(&mut self, i: usize, den: &str, so_tien: i64) {
        let cs = &mut self.ds[i];
        cs.ud.chon_nguoi_nhan(&cs.nd, den);
        cs.ud.o_tien = tien(so_tien).trim_end_matches(" đ").to_string();
        cs.ud.bam_chuyen(&mut cs.nd);
    }

    /// An→Bình 500.000 đ và Bình→An 300.000 đ trong cùng một nhịp giao diện (cùng lúc).
    fn cheo(&mut self) {
        self.dem = self.ds[0].ud.so_kq + self.ds[1].ud.so_kq;
        self.chuyen(0, "TK02", 500_000);
        self.chuyen(1, "TK01", 300_000);
        self.kb.cho_khi(|d| d.ds[0].ud.so_kq + d.ds[1].ud.so_kq == d.dem + 2);
    }

    fn tre(&mut self, ms: &str) {
        self.sv.o_tre = ms.into();
        self.sv.bam_tre();
    }

    fn in_cua_so(&self, chi: &[usize]) {
        for &i in chi {
            let cs = &self.ds[i];
            log(&format!("   {}: {}", cs.nd.ten, cs.ud.thong_bao.0.split('\n').next().unwrap_or("")));
        }
    }

    fn in_server(&self) {
        let tk = self.sv.lop.thong_ke_day_du();
        let s = |k: &str| so_us(tk.get(k));
        log(&format!("   máy chủ: {} yc, tc {}, từ chối {}, sai phạm {}", s("so_yc"), s("thanh_cong"), s("tu_choi"), s("sai_pham_tong")));
        log(&format!("   {}", s("tom_tat")));
        for v in tk.get("vi_du_sai_pham").and_then(Value::as_array).into_iter().flatten().take(2) {
            log(&format!("     ví dụ: {}", py_str(v)));
        }
    }

    fn so_gd_an(&self) -> Option<i64> {
        self.ds[0].nd.tt.as_ref().and_then(|t| lay_i64(t, "so_gd"))
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
                log("1. Máy chủ: START SERVER (khoa, khóa theo thứ tự mã)");
                self.sv.bam_start();
                self.kb.cho(0.3);
            }
            2 => {
                log("   An, Bình, Chi: Kết nối (TK01, TK02, TK03)");
                for cs in &mut self.ds {
                    cs.bam_ket_noi();
                }
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.is_some()));
            }
            3 => self.kb.cho(1.0),
            4 => {
                log("2. An→Bình 500.000 đ và Bình→An 300.000 đ CÙNG LÚC");
                self.cheo();
            }
            5 => self.kb.cho(0.4),
            6 => {
                self.in_cua_so(&[0, 1]);
                log("3. Bật 'khóa ngây thơ', Đặt lại; trễ 0–50 ms");
                self.sv.tn.dat_khoa_ngay_tho(true);
                self.sv.bam_dat_lai();
                self.tre("50");
                self.kb.cho_khi(|d| d.ds[0].nd.tt.as_ref().is_some_and(|t| dung(t, "khoa_ngay_tho")) && d.so_gd_an() == Some(0));
            }
            7 => self.kb.cho(0.5),
            8 => {
                log("   An→Bình và Bình→An cùng lúc…");
                self.cheo();
            }
            9 => self.kb.cho(0.4),
            10 => {
                self.in_cua_so(&[0, 1]);
                log(&format!("   máy chủ: khóa chết phát hiện {}", so_us(self.sv.lop.trang_thai().get("khoa_chet"))));
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            11 => {
                let c = |i: usize| self.chup.cat(self.khung[i], self.ppp);
                let anh = ghep_anh(&[vec![c(0)], vec![c(1), c(2)]]);
                ghi_va_bao(&self.duong("ket_qua_gui_khoa_chet.png"), &anh, log);
                log("4. Tắt 'khóa ngây thơ', Đặt lại; lặp lại chuyển chéo");
                self.sv.tn.dat_khoa_ngay_tho(false);
                self.sv.bam_dat_lai();
                self.kb.cho_khi(|d| d.ds[0].nd.tt.as_ref().is_some_and(|t| !dung(t, "khoa_ngay_tho")) && d.so_gd_an() == Some(0));
            }
            12 => self.kb.cho(0.5),
            13 => self.cheo(),
            14 => self.kb.cho(0.4),
            15 => {
                self.in_cua_so(&[0, 1]);
                log("5. Máy chủ: khong_khoa, trễ 0–20 ms; Đặt lại");
                self.tre("20");
                self.sv.chon_che_do("khong_khoa");
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_str(t, "che_do")) == Some("khong_khoa")));
            }
            16 => {
                self.sv.bam_dat_lai();
                self.kb.cho_khi(|d| d.so_gd_an() == Some(0));
            }
            17 => self.kb.cho(0.5),
            18 => {
                log("   An→Bình, Bình→Chi, Chi→An 1.000.000 đ cùng lúc");
                self.dem = self.ds.iter().map(|cs| cs.ud.so_kq).sum();
                for (i, den) in ["TK02", "TK03", "TK01"].iter().enumerate() {
                    self.chuyen(i, den, 1_000_000);
                }
                self.kb.cho_khi(|d| d.ds.iter().map(|cs| cs.ud.so_kq).sum::<u64>() == d.dem + 3);
            }
            19 => self.kb.cho(0.6),
            20 => {
                self.in_cua_so(&[0, 1, 2]);
                self.in_server();
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            21 => {
                let anh = ghep_anh(&[vec![self.chup.cat(self.khung[0], self.ppp), self.chup.cat(self.khung[3], self.ppp)]]);
                ghi_va_bao(&self.duong("ket_qua_gui_tranh_chap.png"), &anh, log);
                log("6. Máy chủ: khoa, trễ 0–2 ms; So sánh 4 chế độ");
                self.tre("2");
                self.sv.chon_che_do("khoa");
                self.kb.cho_khi(|d| d.sv.lop.kho.che_do() == "khoa");
            }
            22 => self.kb.cho(0.3),
            23 => {
                self.sv.bam_so_sanh();
                self.kb.cho(0.3);
            }
            24 => self.kb.cho_khi(|d| !d.sv.dang_mo_phong),
            25 => {
                for d in bang_so_sanh(&self.sv.hang_so_sanh) {
                    log(&d);
                }
                for cau in self.sv.ket_luan.split(". ") {
                    log(&format!("   {}", cau.trim_end_matches('.')));
                }
                self.kb.cho(0.5);
            }
            26 => {
                log("7. Đặt lại; An chuyển 200.000 đ cho Chi");
                self.sv.bam_dat_lai();
                self.kb.cho_khi(|d| d.so_gd_an() == Some(0));
            }
            27 => self.kb.cho(0.5),
            28 => {
                self.dem = self.ds[0].ud.so_kq;
                self.chuyen(0, "TK03", 200_000);
                self.kb.cho_khi(|d| d.ds[0].ud.so_kq == d.dem + 1);
            }
            29 => self.kb.cho(0.8),
            30 => {
                self.in_cua_so(&[0]);
                log(&format!("   Chi: {}", self.ds[2].ud.nhan_den.0));
                log(&format!("   máy chủ: {}", so_us(self.sv.lop.thong_ke_day_du().get("tom_tat"))));
                log("8. Chụp 4 cửa sổ");
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            31 => {
                let c = |i: usize| self.chup.cat(self.khung[i], self.ppp);
                let anh = ghep_anh(&[vec![c(0), c(1)], vec![c(2), c(3)]]);
                ghi_va_bao(&self.duong("ket_qua_gui.png"), &anh, log);
                log("9. Máy chủ: STOP SERVER");
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

const BUOC_CUOI: usize = 33;

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
    let app = DemoApp {
        sv: tao_cua_so(&host, cong),
        ds: ["An", "Bình", "Chi"].iter().map(|t| CuaSoNguoiDung::moi(t, &host, cong)).collect(),
        kb: KichBan::default(),
        chup: MayChup::default(),
        khung: [egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1060.0, 700.0)); 4],
        ppp: 1.0,
        thu_muc,
        dem: 0,
    };
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1800.0, 1500.0]).with_title("Demo giao diện MR6.12 – máy chủ và 3 ứng dụng ngân hàng"),
        ..Default::default()
    };
    eframe::run_native(
        "Demo MR6.12",
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
