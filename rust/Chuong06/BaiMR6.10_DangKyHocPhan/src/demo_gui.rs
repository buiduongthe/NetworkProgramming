//! Bài mở rộng MR6.10 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).
//!
//! Một tiến trình, một cửa sổ egui chứa bốn "cửa sổ con": bảng điều khiển máy chủ và ba cổng đăng ký của
//! An, Bình, Chi. Kịch bản gọi đúng các hàm mà nút gọi, theo từng bước (chờ một số giây hoặc chờ tới khi
//! điều kiện đúng – vòng vẽ của egui vẫn chạy trong lúc chờ), in `[demo] …`:
//!   1–2. chế độ khoa, sĩ số 1: cả ba cùng bấm "Đăng ký" lớp 123013-01 -> một người được, hai người
//!        "Lớp đã đủ";
//!   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> cả ba "thành công" -> SAI PHẠM vượt sĩ số;
//!        danh sách lớp chỉ còn một tên, hai cửa sổ kia báo "đăng ký đã bị MẤT";
//!        chụp ket_qua_gui_tranh_chap.png;
//!   4.   So sánh 4 chế độ (2000 sinh viên, 3 lớp × 50, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
//!   5–6. đợt mới (3 lớp × 50): An đăng ký 2 lớp, Bình 1 lớp; 500 sinh viên ảo đăng ký -> lớp đủ;
//!        Chi đăng ký lớp đủ -> "Lớp đã đủ"; An hủy một lớp -> Chi đăng ký được chỗ vừa nhả;
//!        chụp 4 cửa sổ ghép thành ket_qua_gui.png.
//!
//! Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" mr6_10_dang_ky_hoc_phan demo_gui [host] [cổng]

use crate::chung::*;
use crate::dang_ky_gui::CongDangKy;
use crate::gui_chung::*;
use crate::may_chu::CONG_GUI;
use crate::server_gui::{tao_cua_so, tieu_de, CuaSoServerDangKy};
use eframe::egui;
use serde_json::Value;

const LOP1: &str = "123013-01";
const LOP2: &str = "123005-01";

fn log(text: &str) {
    in_dong("demo", text);
}

struct DemoApp {
    sv: CuaSoServerDangKy,
    ds: Vec<CuaSoNguoiDung<CongDangKy>>,
    kb: KichBan<DemoApp>,
    chup: MayChup,
    khung: [egui::Rect; 4], // vị trí 4 cửa sổ con (để cắt ảnh)
    ppp: f32,
    thu_muc: String,
}

impl DemoApp {
    fn dang_ky(&mut self, i: usize, ma: &str) {
        let cs = &mut self.ds[i];
        cs.ud.bam(&mut cs.nd, ma);
    }

    fn dat_lai(&mut self, so_lop: i64, si_so: i64) {
        self.sv.tn.dat_tham_so(so_lop, si_so);
        self.sv.bam_dat_lai();
    }

    fn xong(&self) -> bool {
        self.ds.iter().all(|cs| cs.ud.dang_cho.is_empty())
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

    fn in_lop(&self) {
        for x in self.sv.lop.trang_thai().get("lop").and_then(Value::as_array).into_iter().flatten() {
            let sv: Vec<String> = x.get("sv").and_then(Value::as_array).into_iter().flatten().take(3).map(py_str).collect();
            log(format!("   {}: {}/{} {}", so_us(x.get("ma")), so_us(x.get("da_dk")), so_us(x.get("si_so")), sv.join(" ")).trim_end());
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
                log("2. Đặt lại: 3 lớp, sĩ số 1 (còn đúng 1 chỗ mỗi lớp)");
                self.dat_lai(3, 1);
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_i64(t, "si_so")) == Some(1)));
            }
            4 => self.kb.cho(0.5),
            5 => {
                log(&format!("   An, Bình, Chi CÙNG bấm Đăng ký {LOP1}"));
                for i in 0..3 {
                    self.dang_ky(i, LOP1);
                }
                self.kb.cho_khi(|d| d.xong());
            }
            6 => self.kb.cho(0.8),
            7 => {
                self.in_cua_so(&[0, 1, 2]);
                self.in_server();
                log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; Đặt lại");
                self.sv.o_tre = "20".into();
                self.sv.bam_tre();
                self.dat_lai(3, 1);
                self.sv.chon_che_do("khong_khoa");
                self.kb.cho_khi(|d| d.sv.lop.kho.che_do() == "khong_khoa");
            }
            8 => self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.nd.tt.as_ref().and_then(|t| lay_str(t, "che_do")) == Some("khong_khoa") && cs.ud.cua_toi.is_empty())),
            9 => self.kb.cho(0.5),
            10 => {
                log(&format!("   An, Bình, Chi CÙNG bấm Đăng ký {LOP1}"));
                for i in 0..3 {
                    self.dang_ky(i, LOP1);
                }
                self.kb.cho_khi(|d| d.xong());
            }
            11 => {
                log("   ngay khi có trả lời:");
                self.in_cua_so(&[0, 1, 2]);
                self.kb.cho(1.2);
            }
            12 => {
                log("   1 giây sau (đối chiếu danh sách lớp):");
                self.in_cua_so(&[0, 1, 2]);
                self.in_server();
                self.in_lop();
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            13 => {
                let mat = self.ds.iter().position(|cs| cs.ud.thong_bao.0.contains("MẤT")).unwrap_or(1);
                let anh = ghep_anh(&[vec![self.chup.cat(self.khung[0], self.ppp), self.chup.cat(self.khung[mat + 1], self.ppp)]]);
                let ra = self.duong("ket_qua_gui_tranh_chap.png");
                match ghi_png(&ra, &anh) {
                    Ok(()) => log(&format!("   Đã ghi ket_qua_gui_tranh_chap.png ({} KB, cửa sổ {})", std::fs::metadata(&ra).map_or(0, |m| m.len() / 1024), self.ds[mat].nd.ten)),
                    Err(e) => log(&format!("   Không ghi được ket_qua_gui_tranh_chap.png: {e}")),
                }
                log("4. Máy chủ: khoa, trễ 0–2 ms, 3 lớp × 50; So sánh");
                self.sv.o_tre = "2".into();
                self.sv.bam_tre();
                self.dat_lai(3, 50);
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
                log("5. Đặt lại; An đăng ký 2 lớp, Bình 1 lớp");
                self.dat_lai(3, 50);
                self.kb.cho_khi(|d| d.ds.iter().all(|cs| cs.ud.cua_toi.is_empty() && cs.ud.chu_nut(&cs.nd, LOP1) == "Đăng ký"));
            }
            19 => self.kb.cho(0.5),
            20 => {
                self.dang_ky(0, LOP1);
                self.dang_ky(0, LOP2);
                self.dang_ky(1, LOP1);
                self.kb.cho_khi(|d| d.xong());
            }
            21 => self.kb.cho(0.5),
            22 => {
                log("   Bắn đồng loạt 500 sinh viên ảo (mỗi người 1 lớp)");
                self.sv.o_n = "500".into();
                self.sv.bam_ban();
                self.kb.cho(0.3);
            }
            23 => self.kb.cho_khi(|d| !d.sv.dang_mo_phong),
            24 => self.kb.cho(0.8),
            25 => {
                let hang = self.sv.hang_so_sanh.clone();
                self.sv.xong_mo_phong(hang); // bảng: hiện lại kết quả so sánh 4 chế độ
                self.in_lop();
                let chi = &self.ds[2];
                log(&format!("   Chi bấm '{}' ở {LOP2}", chi.ud.chu_nut(&chi.nd, LOP2)));
                self.dang_ky(2, LOP2);
                self.kb.cho_khi(|d| d.xong());
            }
            26 => self.kb.cho(0.5),
            27 => {
                self.in_cua_so(&[2]);
                log(&format!("6. An bấm Hủy {LOP2}, rồi Chi bấm Đăng ký {LOP2}"));
                self.dang_ky(0, LOP2);
                self.kb.cho_khi(|d| d.xong());
            }
            28 => self.kb.cho_khi(|d| d.ds[2].ud.chu_nut(&d.ds[2].nd, LOP2) == "Đăng ký"),
            29 => {
                self.dang_ky(2, LOP2);
                self.kb.cho_khi(|d| d.xong());
            }
            30 => self.kb.cho(0.6),
            31 => {
                self.in_cua_so(&[0, 1, 2]);
                self.in_server();
                log("7. Chụp 4 cửa sổ");
                self.chup.yeu_cau(ctx);
                self.kb.cho_khi(|d| d.chup.anh.is_some());
            }
            32 => {
                let c = |i: usize| self.chup.cat(self.khung[i], self.ppp);
                let anh = ghep_anh(&[vec![c(0), c(1)], vec![c(2), c(3)]]);
                ghi_va_bao(&self.duong("ket_qua_gui.png"), &anh, log);
                log("8. Máy chủ: STOP SERVER");
                self.sv.bam_stop();
                self.kb.cho_khi(|d| d.sv.start_bat());
            }
            33 => self.kb.cho(0.5),
            34 => {
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

const BUOC_CUOI: usize = 34;

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
                        ui.set_width(650.0);
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
        khung: [egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1060.0, 700.0)); 4],
        ppp: 1.0,
        thu_muc,
    };
    let opt = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1800.0, 1400.0]).with_title("Demo giao diện MR6.10 – máy chủ và 3 cổng đăng ký"),
        ..Default::default()
    };
    eframe::run_native(
        "Demo MR6.10",
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
