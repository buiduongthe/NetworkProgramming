//! Bài mở rộng MR6.13 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).
//!
//! Một tiến trình, một cửa sổ egui chứa bốn "cửa sổ con": bảng điều khiển máy chủ và ba trang bán vé An,
//! Bình, Chi. Kịch bản gọi đúng các hàm mà nút gọi, theo từng bước (chờ một số giây hoặc chờ tới khi điều
//! kiện đúng – vòng vẽ của egui vẫn chạy trong lúc chờ), in `[demo] …`:
//!   1–3. chế độ khoa, phòng chờ bật (50 người/s, 2000 vé): Chi vào hàng đầu tiên -> tới lượt ngay, mua 2 vé;
//!        mô phỏng 300 người ùa vào; An, Bình vào sau -> trang phòng chờ "Còn khoảng … người phía trước";
//!        chụp ket_qua_gui_phong_cho.png; tới lượt An -> mua 3 vé;
//!   4.   chế độ khong_khoa, trễ 0–20 ms: Chi rời, đăng nhập lại tên An (TAB THỨ HAI của An); hai tab cùng vào
//!        hàng, cùng được lượt, cùng bấm THANH TOÁN -> cả hai đơn được nhận -> SAI PHẠM "sai lượt";
//!        chụp ket_qua_gui_tranh_chap.png;
//!   5.   So sánh 4 chế độ (1000 người, 200 kết nối), rồi So sánh bật/tắt phòng chờ;
//!   6.   phòng chờ 20 người/s: Bình tới lượt (đồng hồ đếm ngược), Chi mua 1 vé, 400 người ùa vào, An xếp sau;
//!        chụp 4 cửa sổ ghép thành ket_qua_gui.png.
//!
//! Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" mr6_13_phong_cho demo_gui [host] [cổng]

use crate::chung::*;
use crate::gui_chung::*;
use crate::may_chu::{so_con, CONG_GUI};
use crate::nguoi_ao::bang_phong_cho;
use crate::phong_cho_gui::{Trang, TrangBanVe};
use crate::server_gui::{tao_cua_so, tieu_de, ve_server, CuaSoServerPhongCho};
use eframe::egui;
use serde_json::Value;
use std::sync::atomic::Ordering;

fn log(text: &str) {
    in_dong("demo", text);
}

struct DemoApp {
    sv: CuaSoServerPhongCho,
    ds: Vec<CuaSoNguoiDung<TrangBanVe>>,
    kb: KichBan<DemoApp>,
    chup: MayChup,
    khung: [egui::Rect; 4], // vị trí 4 cửa sổ con (để cắt ảnh)
    ppp: f32,
    thu_muc: String,
    dem: u64,
}

const AN: usize = 0;
const BINH: usize = 1;
const CHI: usize = 2;

impl DemoApp {
    fn vao(&mut self, i: usize) {
        let cs = &mut self.ds[i];
        cs.ud.vao_hang(&mut cs.nd);
    }

    fn mua(&mut self, i: usize, n: i64) {
        let cs = &mut self.ds[i];
        cs.ud.mua(&mut cs.nd, n);
    }

    fn trang(&self, i: usize) -> Trang {
        self.ds[i].ud.trang
    }

    fn tre(&mut self, ms: &str) {
        self.sv.o_tre = ms.into();
        self.sv.bam_tre();
    }

    /// Bấm "Bắn đồng loạt" với n người trên k kết nối.
    fn ban(&mut self, n: usize, k: usize) {
        self.sv.o_n = n.to_string();
        self.sv.o_k = k.to_string();
        self.sv.bam_ban();
    }

    fn dat_lai(&mut self, ts: &[(&str, &str)]) {
        for (k, v) in ts {
            self.sv.tn.dat_tham_so(k, v);
        }
        self.sv.bam_dat_lai();
        for cs in &mut self.ds {
            cs.ud.ve_su_kien(&cs.nd);
        }
    }

    fn so_cap(&self) -> i64 {
        lay_i64(&self.sv.lop.trang_thai(), "so_cap").unwrap_or(0)
    }

    fn tt(&self, i: usize) -> Obj {
        self.ds[i].nd.tt.clone().unwrap_or_default()
    }

    fn doi_ten(&mut self, i: usize, ten: &str) {
        let cs = &mut self.ds[i];
        cs.bam_roi();
        cs.nd.o_ten = ten.into();
        cs.bam_ket_noi();
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
                log("1. Máy chủ: START SERVER (khoa, phòng chờ bật)");
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
            3 => self.kb.cho(0.8),
            4 => {
                log("2. Đặt lại: 2000 vé, cấp lượt 50 người/s, thùng 50");
                self.dat_lai(&[("tong_ve", "2000"), ("toc_do", "50"), ("dung_luong", "50"), ("phong_cho", "1")]);
                self.kb.cho_khi(|d| so_con(&d.tt(CHI), "su_kien", "tong_ve") == 2000);
            }
            5 => self.kb.cho(0.4),
            6 => {
                log("   Chi: VÀO HÀNG CHỜ (người đầu tiên)");
                self.vao(CHI);
                self.kb.cho_khi(|d| d.trang(CHI) == Trang::Mua);
            }
            7 => {
                log(&format!("   Chi: {}", self.ds[CHI].ud.bang_luot.0));
                log("   Mô phỏng: 300 người ùa vào hàng (Bắn đồng loạt)");
                self.ban(300, 100);
                self.kb.cho_khi(|d| d.so_cap() >= 250);
            }
            8 => {
                log("   An, Bình: VÀO HÀNG CHỜ");
                self.vao(AN);
                self.vao(BINH);
                self.kb.cho_khi(|d| d.trang(AN) == Trang::Cho && d.trang(BINH) == Trang::Cho);
            }
            9 => self.kb.cho(0.5),
            10 => {
                log("   Chi: chọn 2 vé, THANH TOÁN");
                self.mua(CHI, 2);
                self.kb.cho(1.0);
            }
            11 => {
                log(&format!("   Chi: {}", self.ds[CHI].ud.kq.0));
                log(&format!("   An: {}", self.ds[AN].ud.chu_so()));
                log(&format!("   An: {}", self.ds[AN].ud.chu_truoc(&self.ds[AN].nd)));
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            12 => {
                let c = |i: usize| self.chup.cat(self.khung[i], self.ppp);
                let anh = ghep_anh(&[vec![c(0)], vec![c(1), c(3)]]);
                ghi_va_bao(&self.duong("ket_qua_gui_phong_cho.png"), &anh, log);
                self.kb.cho_khi(|d| d.trang(AN) == Trang::Mua);
            }
            13 => self.kb.cho(0.5),
            14 => {
                log(&format!("3. An tới lượt: đồng hồ {}; chọn 3 vé, THANH TOÁN", self.ds[AN].ud.dong_ho.0));
                self.mua(AN, 3);
                self.kb.cho_khi(|d| d.ds[AN].ud.so_don == 1);
            }
            15 => {
                log(&format!("   An: {}", self.ds[AN].ud.kq.0));
                self.kb.cho_khi(|d| !d.sv.dang_mo_phong);
            }
            16 => {
                self.in_server();
                log("4. Máy chủ: khong_khoa, trễ 0–20 ms; Đặt lại (500 vé)");
                self.tre("20");
                self.sv.chon_che_do("khong_khoa");
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_str(t, "che_do")) == Some("khong_khoa")));
            }
            17 => {
                self.dat_lai(&[("tong_ve", "500")]);
                self.kb.cho_khi(|d| {
                    let t = d.tt(AN);
                    so_con(&t, "su_kien", "tong_ve") == 500 && lay_i64(&t, "da_cap_luot") == Some(0)
                });
            }
            18 => {
                log("   Chi: Rời, đăng nhập lại tên 'An' (tab thứ hai của An)");
                self.doi_ten(CHI, "An");
                self.kb.cho_khi(|d| d.ds[CHI].nd.kn.is_some() && d.ds[CHI].nd.tt.is_some());
            }
            19 => self.kb.cho(0.5),
            20 => {
                log("   Hai tab của An cùng VÀO HÀNG CHỜ");
                self.vao(AN);
                self.vao(CHI);
                self.kb.cho_khi(|d| d.trang(AN) == Trang::Mua && d.trang(CHI) == Trang::Mua);
            }
            21 => {
                log("   Hai tab cùng bấm THANH TOÁN 2 vé (một lượt)");
                self.dem = self.ds[AN].ud.so_kq + self.ds[CHI].ud.so_kq;
                self.mua(AN, 2);
                self.mua(CHI, 2);
                self.kb.cho_khi(|d| d.ds[AN].ud.so_kq + d.ds[CHI].ud.so_kq == d.dem + 2);
            }
            22 => self.kb.cho(0.5),
            23 => {
                log(&format!("   tab 1: {}", self.ds[AN].ud.kq.0));
                log(&format!("   tab 2: {}", self.ds[CHI].ud.kq.0));
                self.in_server();
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            24 => {
                let anh = ghep_anh(&[vec![self.chup.cat(self.khung[0], self.ppp), self.chup.cat(self.khung[3], self.ppp)]]);
                ghi_va_bao(&self.duong("ket_qua_gui_tranh_chap.png"), &anh, log);
                log("5. khoa, trễ 0–2 ms; Chi vào lại; So sánh 4 chế độ");
                self.doi_ten(CHI, "Chi");
                self.tre("2");
                self.sv.chon_che_do("khoa");
                self.kb.cho_khi(|d| d.sv.lop.kho.che_do() == "khoa" && d.ds[CHI].nd.kn.is_some());
            }
            25 => {
                self.sv.o_n = "1000".into();
                self.sv.o_k = "200".into();
                self.kb.cho(0.3);
            }
            26 => {
                self.sv.bam_so_sanh();
                self.kb.cho(0.3);
            }
            27 => self.kb.cho_khi(|d| !d.sv.dang_mo_phong),
            28 => {
                for d in bang_so_sanh(&self.sv.hang_so_sanh) {
                    log(&d);
                }
                for cau in self.sv.ket_luan.split(". ") {
                    log(&format!("   {}", cau.trim_end_matches('.')));
                }
                log("   So sánh bật/tắt phòng chờ (khoa, 1000 người)");
                self.sv.tn.so_sanh_pc();
                self.kb.cho(0.3);
            }
            29 => self.kb.cho_khi(|d| !d.sv.tn.dang_pc.load(Ordering::SeqCst) && !d.sv.dang_mo_phong),
            30 => {
                for d in bang_phong_cho(&self.sv.tn.hang_pc) {
                    log(&d);
                }
                log("6. Đặt lại: 2000 vé, 20 người/s, thùng 2");
                self.dat_lai(&[("tong_ve", "2000"), ("toc_do", "20"), ("dung_luong", "2"), ("phong_cho", "1")]);
                self.kb.cho_khi(|d| {
                    let t = d.tt(AN);
                    so_con(&t, "su_kien", "tong_ve") == 2000 && lay_f64(&t, "toc_do") == Some(20.0)
                });
            }
            31 => self.kb.cho(0.4),
            32 => {
                log("   Bình, Chi vào hàng; Chi mua 1 vé");
                self.vao(BINH);
                self.vao(CHI);
                self.kb.cho_khi(|d| d.trang(BINH) == Trang::Mua && d.trang(CHI) == Trang::Mua);
            }
            33 => {
                self.mua(CHI, 1);
                log("   400 người ùa vào; An vào hàng sau họ");
                self.ban(400, 100);
                self.kb.cho_khi(|d| d.so_cap() >= 350);
            }
            34 => {
                self.vao(AN);
                self.kb.cho_khi(|d| d.trang(AN) == Trang::Cho);
            }
            35 => self.kb.cho(2.0),
            36 => {
                log(&format!("   An: {}", self.ds[AN].ud.chu_truoc(&self.ds[AN].nd)));
                log(&format!("   Bình: đồng hồ {}", self.ds[BINH].ud.dong_ho.0));
                log(&format!("   Chi: {}", self.ds[CHI].ud.kq.0));
                log("7. Chụp 4 cửa sổ");
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            37 => {
                let c = |i: usize| self.chup.cat(self.khung[i], self.ppp);
                let anh = ghep_anh(&[vec![c(0), c(1)], vec![c(2), c(3)]]);
                ghi_va_bao(&self.duong("ket_qua_gui.png"), &anh, log);
                log("8. Máy chủ: STOP SERVER");
                self.sv.bam_stop();
                self.kb.cho_khi(|d| d.sv.start_bat());
            }
            38 => self.kb.cho(0.5),
            39 => {
                log(&format!("   Bình: {}", self.ds[BINH].nd.thanh_tt));
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

const BUOC_CUOI: usize = 39;

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
                        ve_server(&mut self.sv, ui);
                    } else {
                        ui.set_width(560.0);
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
        viewport: egui::ViewportBuilder::default().with_inner_size([1720.0, 1500.0]).with_title("Demo giao diện MR6.13 – máy chủ và 3 trang bán vé"),
        ..Default::default()
    };
    eframe::run_native(
        "Demo MR6.13",
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
