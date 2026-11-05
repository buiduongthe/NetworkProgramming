//! Bài mở rộng MR6.5 – Lõi người xem phát trực tiếp, không chạm giao diện.
//!
//! Dùng chung cho xem.rs (dòng lệnh) và xem_gui.rs. Bốn luồng, chung một khóa `khoa`:
//!   1. luồng đọc TCP: thong_tin, dong_ho (báo cáo đồng hồ), su_kien, ket_thuc, bi_ngat;
//!   2. luồng nhận UDP: RTP -> bộ đệm jitter; mảnh video -> GhepKhung -> GiaiMaVideo -> hàng hiển thị;
//!   3. luồng phát 20 ms = ĐỒNG HỒ CHỦ: lấy một khung âm thanh, đưa ra loa, cập nhật
//!         dong_ho_am = ts/16 (ms phương tiện) và giờ thực lúc phát; ghi lịch sử ~3 s để đo lệch;
//!   4. luồng hiển thị 5 ms: kiểm tra mất tham chiếu (xin khung khóa), chọn khung để hiện.
//!
//! Đồng bộ tiếng-hình (lip-sync) – âm thanh làm đồng hồ chủ (như trình phát thật: tai nhạy với tiếng
//! bị ngắt hơn mắt nhạy với một khung hình hiện sớm/muộn):
//!   - BẬT: chỉ hiện khung mới nhất có nhãn ≤ đồng hồ phát hiện tại
//!         dong_ho_am + (bây giờ − lúc phát khung âm thanh đó); khung quá cũ bị bỏ.
//!   - TẮT (hoặc chỉ nhận video): hiện ngay khi giải mã xong -> hình đi TRƯỚC tiếng đúng bằng thời gian
//!     âm thanh nằm trong bộ đệm jitter (độ sâu × 20 ms).
//! Lệch tiếng-hình của một khung = giờ thực lúc hiện − giờ thực lúc âm thanh cùng nhãn được phát
//! (nội suy trên lưới 20 ms của lịch sử phát); dương = hình chậm hơn tiếng, âm = hình đi trước.
//! Độ trễ đầu-cuối (âm thanh) = giờ Unix lúc phát − (gio + (ts_ms − ts)) theo báo cáo đồng hồ mới nhất
//! – chỉ đúng khi máy phát và người xem cùng đồng hồ (cùng máy hoặc đồng bộ NTP).

use crate::chung::*;
use crate::may_chu::Bao;
use crate::thiet_bi::{day_ra_loa, HangMau};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::BufReader;
use std::net::{Shutdown, SocketAddr, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MS_MOI_MAU: f64 = 1000.0 / TAN_SO_MAU as f64;

/// Khung đang hiện: (rộng, cao, rgb, nhãn hình ms, số khung, đồng hồ tiếng lúc hiện).
#[derive(Clone)]
pub struct Hien {
    pub w: usize,
    pub h: usize,
    pub anh: Arc<Vec<u8>>,
    pub ts: u32,
    pub so: u32,
    pub am: Option<f64>,
}

/// Lệch tiếng-hình của một giai đoạn bật/tắt đồng bộ.
#[derive(Clone)]
pub struct GiaiDoan {
    pub dong_bo: bool,
    pub tong: f64,
    pub n: u64,
}

pub struct TT {
    tcp: Option<TcpStream>,
    udp: Option<Arc<UdpSocket>>,
    phien: u64,
    pub da_ket_noi: bool,
    pub ma: u32,
    pub do_sau: usize,
    pub dong_bo: bool,
    pub am_thanh: bool,
    pub video: bool,
    rng: Rng,
    bo_gia_lap: u64,
    pub loa: Option<HangMau>, // hàng đợi phát của loa thật hoặc None (không phát)
    thong_tin: Option<Value>,
    muc_am: f64,
    giai_doan: Vec<GiaiDoan>,
    // thống kê
    luc_chao: f64,
    luc_moc: f64, // mốc đo khung đầu: lúc vào, hoặc lúc máy phát bắt đầu phát nếu khi vào chưa có ai phát
    cho_phat: bool,
    goi_am: u64,
    byte_nhan: u64,
    khung_i: u64,
    khung_p: u64,
    khung_hien: u64,
    dung_hinh: u64,
    pli_gui: u64,
    bo_cu: u64,
    mat_cu: u64, // số đếm của các phiên phát trước
    hong_cu: u64,
    bo_p_cu: u64,
    tong_tre: f64,
    n_tre: u64,
    tre_hien: Option<f64>,
    lech_hien: Option<f64>,
    khung_dau_ms: Option<f64>,
    nguon_dau: &'static str,
    i_dau: Option<(Option<f64>, f64)>,
    cho_hinh_lai: Option<f64>,
    luc_pli: f64,
    dang_dung: bool,
    w_am: u64,
    w_hien: u64,
    w_byte: u64,
    w_tre: Vec<f64>,
    w_lech: Vec<f64>,
    // phiên phát
    bo_dem: BoDemJitter,
    ghep: GhepKhung,
    giai_ma: GiaiMaVideo,
    hang: VecDeque<KhungGiai>,
    lich_su: VecDeque<(f64, f64)>, // (ts ms, giờ đơn điệu) ~3 s gần nhất
    dong_ho_am: f64,
    luc_phat_am: Option<f64>,
    luc_nhan_am: Option<f64>,
    ts_am_moi: Option<u32>,
    ts_hinh_moi: Option<u32>,
    dong_ho_bao: Option<(f64, f64)>, // (ts, gio) báo cáo đồng hồ mới nhất
    pub hien: Option<Hien>,
    pub so_hien: u64,
}

impl TT {
    fn lam_moi_thong_ke(&mut self) {
        self.luc_chao = bay_gio();
        self.luc_moc = self.luc_chao;
        self.cho_phat = false;
        (self.goi_am, self.byte_nhan, self.khung_i, self.khung_p, self.khung_hien) = (0, 0, 0, 0, 0);
        (self.dung_hinh, self.pli_gui, self.bo_cu) = (0, 0, 0);
        (self.mat_cu, self.hong_cu, self.bo_p_cu) = (0, 0, 0);
        self.tong_tre = 0.0;
        self.n_tre = 0;
        (self.tre_hien, self.lech_hien) = (None, None);
        (self.khung_dau_ms, self.nguon_dau, self.i_dau) = (None, "", None);
        self.cho_hinh_lai = None;
        self.luc_pli = -1e9;
        self.dang_dung = false;
        (self.w_am, self.w_hien, self.w_byte) = (0, 0, 0);
        self.w_tre.clear();
        self.w_lech.clear();
    }

    /// Trạng thái của một phiên phát (làm mới khi máy phát dừng phát).
    fn lam_moi_phien(&mut self) {
        self.mat_cu += self.bo_dem.mat();
        self.hong_cu += self.ghep.khung_hong;
        self.bo_p_cu += self.giai_ma.bo_p;
        self.bo_dem = BoDemJitter::new(self.do_sau);
        self.ghep = GhepKhung::new();
        self.giai_ma = GiaiMaVideo::new();
        self.hang.clear();
        self.lich_su.clear();
        self.dong_ho_am = 0.0;
        (self.luc_phat_am, self.luc_nhan_am) = (None, None);
        (self.ts_am_moi, self.ts_hinh_moi) = (None, None);
        self.dong_ho_bao = None;
        self.hien = None;
        self.so_hien = 0;
    }

    /// Ước lượng ms phương tiện hiện tại (để biết khung I đầu tiên "cũ" bao nhiêu).
    fn uoc_luong_ts(&self) -> Option<f64> {
        if let Some((ts, gio)) = self.dong_ho_bao {
            return Some(ts + (gio_unix_ms() - gio)); // giả định cùng đồng hồ
        }
        [self.ts_hinh_moi.map(|x| x as f64), self.ts_am_moi.map(|x| x as f64 * MS_MOI_MAU)].into_iter().flatten().filter(|&x| x != 0.0).reduce(f64::max)
    }

    /// Khung vừa đủ mảnh -> giải mã (có thể giải thêm các khung đang chờ) -> hàng hiển thị.
    fn nhan_khung(&mut self, k: KhungDu, bay: f64, bao: &Bao) {
        for kh in self.giai_ma.them(k, bay) {
            self.ts_hinh_moi = Some(self.ts_hinh_moi.unwrap_or(0).max(kh.ts));
            if kh.loai == LOAI_I {
                self.khung_i += 1;
                if self.i_dau.is_none() {
                    let uoc = self.uoc_luong_ts();
                    self.i_dau = Some((uoc.map(|u| u - kh.ts as f64), (bay - self.luc_moc) * 1000.0));
                }
                if self.dang_dung {
                    self.dang_dung = false;
                    let ms = self.giai_ma.luc_mat.map_or(0.0, |l| (bay - l) * 1000.0);
                    bao(&format!("Hồi phục bằng khung I #{} sau {ms:.0} ms", kh.so));
                }
            } else {
                self.khung_p += 1;
            }
            self.hang.push_back(kh);
            if self.hang.len() > HANG_HIEN_THI {
                self.hang.pop_front();
                self.bo_cu += 1;
            }
        }
    }

    /// Đồng hồ phát âm thanh hiện tại (ms phương tiện) hoặc None nếu tiếng không chạy.
    fn dong_ho_hien_tai(&self, bay: f64) -> Option<f64> {
        let l = self.luc_phat_am?;
        if bay - l > 0.2 {
            return None;
        }
        Some(self.dong_ho_am + (bay - l).min(0.04) * 1000.0)
    }

    fn chon_khung(&mut self, bay: f64) -> Option<KhungGiai> {
        if self.hang.is_empty() {
            return None;
        }
        if !self.video {
            self.hang.clear();
            return None;
        }
        // Tiếng "đang chạy": vừa nhận gói tiếng trong 0,5 s, hoặc mới vào xem chưa tới 0,5 s (chờ tiếng
        // tới để khung đầu cũng được đồng bộ). Không có tiếng -> hiện ngay, không chờ mãi.
        let tieng_chay = match self.luc_nhan_am {
            Some(l) => self.am_thanh && bay - l < 0.5,
            None => self.am_thanh && bay - self.luc_chao < 0.5,
        };
        if self.dong_bo && tieng_chay {
            let am = self.dong_ho_hien_tai(bay)?; // chờ đồng hồ tiếng (bộ đệm jitter đang nạp)
            let mut chon = None;
            while self.hang.front().is_some_and(|k| k.ts as f64 <= am) {
                if chon.is_some() {
                    self.bo_cu += 1; // khung quá cũ: bỏ
                }
                chon = self.hang.pop_front();
            }
            return chon;
        }
        let chon = self.hang.pop_back(); // không đồng bộ: hiện ngay khung mới nhất
        self.bo_cu += self.hang.len() as u64;
        self.hang.clear();
        chon
    }

    /// Giờ (đơn điệu) mà âm thanh có nhãn ts_ms được/sẽ được phát – nội suy theo lịch sử phát.
    fn gio_am(&self, ts_ms: f64) -> Option<f64> {
        self.lich_su.iter().rev().find(|(t_am, _)| *t_am <= ts_ms).map(|(t_am, gio)| gio + (ts_ms - t_am) / 1000.0)
    }

    fn hien_khung(&mut self, k: KhungGiai, bay: f64, bao: &Bao) {
        let am = self.dong_ho_hien_tai(bay);
        self.hien = Some(Hien { w: k.w as usize, h: k.h as usize, anh: k.anh.clone(), ts: k.ts, so: k.so, am });
        self.so_hien += 1;
        self.khung_hien += 1;
        self.w_hien += 1;
        if self.khung_dau_ms.is_none() {
            let ms = (bay - self.luc_moc) * 1000.0;
            self.khung_dau_ms = Some(ms);
            // Nguồn khung đầu: khung I đã "cũ" (chụp trước lúc vào > 1,5 chu kỳ khung) -> từ bộ đệm GOP;
            // I mới nhưng tới muộn -> phải chờ khung I; I mới tới ngay -> vừa đúng lúc có khung I.
            let (cu, den) = self.i_dau.unwrap_or((None, f64::NAN));
            let fps = self.thong_tin.as_ref().and_then(|t| t.get("fps")).and_then(Value::as_f64).unwrap_or(10.0);
            let nguong = 1500.0 / fps;
            let mut them = String::new();
            if cu.is_some_and(|c| c > nguong) {
                self.nguon_dau = "bộ đệm GOP";
                them = format!(" (I cũ {:.1} s)", cu.unwrap() / 1000.0);
            } else if den > nguong {
                self.nguon_dau = "chờ khung I";
            } else {
                self.nguon_dau = "khung I trực tiếp";
            }
            let moc = if self.luc_moc > self.luc_chao { " (từ lúc phát)" } else { "" };
            bao(&format!("Khung đầu sau {ms:.0} ms{moc}: {}{them}", self.nguon_dau));
        }
        if let Some(c) = self.cho_hinh_lai.take() {
            bao(&format!("Hình trở lại sau {:.0} ms", (bay - c) * 1000.0));
        }
        let tieng_chay = self.am_thanh && self.luc_phat_am.is_some_and(|l| bay - l < 0.5);
        let gio_am = if tieng_chay { self.gio_am(k.ts as f64) } else { None };
        if let Some(g) = gio_am {
            let lech = (bay - g) * 1000.0;
            self.lech_hien = Some(lech);
            let gd = self.giai_doan.last_mut().unwrap();
            gd.tong += lech;
            gd.n += 1;
            self.w_lech.push(lech);
        } else {
            self.lech_hien = None;
        }
    }
}

/// Ảnh chụp khung đang hiện: (w, h, rgb, nhãn hình ms, đồng hồ tiếng LÚC HIỆN ms, lệch ms, số khung).
pub struct Chup {
    pub w: usize,
    pub h: usize,
    pub anh: Arc<Vec<u8>>,
    pub ts: u32,
    pub am: Option<f64>,
    pub lech: Option<f64>,
    pub so: u32,
}

/// Số liệu của giây vừa qua.
pub struct Giay {
    pub am: u64,
    pub mat: u64,
    pub hien: u64,
    pub tre: Option<f64>,
    pub lech: Option<f64>,
    #[allow(dead_code)]
    pub kbps: f64,
}

/// Ảnh chụp trạng thái người xem.
pub struct TrangThai {
    pub da_ket_noi: bool,
    pub goi_am: u64,
    pub mat_am: u64,
    pub tre_tb: Option<f64>,
    pub tre_hien: Option<f64>,
    pub khung_i: u64,
    pub khung_p: u64,
    pub khung_hien: u64,
    pub hong: u64,
    pub bo_p: u64,
    pub bo_cu: u64,
    pub dung_hinh: u64,
    pub pli_gui: u64,
    pub khung_dau_ms: Option<f64>,
    pub nguon_dau: &'static str,
    pub lech_hien: Option<f64>,
    pub giai_doan: Vec<GiaiDoan>,
    pub kbps: f64,
    pub dong_bo: bool,
    pub am_thanh: bool,
    pub video: bool,
    pub do_sau: usize,
    pub muc_am: f64,
    pub bo_gia_lap: u64,
}

pub struct NguoiXem {
    pub ten: String,
    bao: Bao,
    pub khoa: Mutex<TT>,
    khoa_tcp: Mutex<()>,
    mat_goi: f64,
    song: AtomicBool,
}

impl NguoiXem {
    pub fn new(ten: &str, bao: Bao, do_sau: usize, dong_bo: bool, am_thanh: bool, video: bool, mat_goi: f64, loa: Option<HangMau>) -> Arc<NguoiXem> {
        let mut tt = TT {
            tcp: None,
            udp: None,
            phien: 0,
            da_ket_noi: false,
            ma: 0,
            do_sau,
            dong_bo,
            am_thanh,
            video,
            rng: Rng::new(0),
            bo_gia_lap: 0,
            loa,
            thong_tin: None,
            muc_am: f64::NEG_INFINITY,
            giai_doan: vec![GiaiDoan { dong_bo, tong: 0.0, n: 0 }],
            luc_chao: 0.0,
            luc_moc: 0.0,
            cho_phat: false,
            goi_am: 0,
            byte_nhan: 0,
            khung_i: 0,
            khung_p: 0,
            khung_hien: 0,
            dung_hinh: 0,
            pli_gui: 0,
            bo_cu: 0,
            mat_cu: 0,
            hong_cu: 0,
            bo_p_cu: 0,
            tong_tre: 0.0,
            n_tre: 0,
            tre_hien: None,
            lech_hien: None,
            khung_dau_ms: None,
            nguon_dau: "",
            i_dau: None,
            cho_hinh_lai: None,
            luc_pli: -1e9,
            dang_dung: false,
            w_am: 0,
            w_hien: 0,
            w_byte: 0,
            w_tre: Vec::new(),
            w_lech: Vec::new(),
            bo_dem: BoDemJitter::new(do_sau),
            ghep: GhepKhung::new(),
            giai_ma: GiaiMaVideo::new(),
            hang: VecDeque::new(),
            lich_su: VecDeque::new(),
            dong_ho_am: 0.0,
            luc_phat_am: None,
            luc_nhan_am: None,
            ts_am_moi: None,
            ts_hinh_moi: None,
            dong_ho_bao: None,
            hien: None,
            so_hien: 0,
        };
        tt.lam_moi_thong_ke();
        let nx = Arc::new(NguoiXem { ten: ten.to_string(), bao, khoa: Mutex::new(tt), khoa_tcp: Mutex::new(()), mat_goi, song: AtomicBool::new(true) });
        let (a, b) = (Arc::clone(&nx), Arc::clone(&nx));
        thread::spawn(move || a.vong_am());
        thread::spawn(move || b.vong_hien());
        nx
    }

    pub fn da_ket_noi(&self) -> bool {
        self.khoa.lock().unwrap().da_ket_noi
    }

    // ------------------------------------------------------------ kết nối
    pub fn ket_noi(self: &Arc<Self>, host: &str, cong: u16) -> Result<u32, String> {
        let tcp = ket_noi_tcp(host, cong).map_err(|e| e.to_string())?;
        let r = (|| -> Result<(u32, bool, UdpSocket, BufReader<TcpStream>), String> {
            let mut tep = BufReader::new(tcp.try_clone().map_err(|e| e.to_string())?);
            tcp.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
            gui_json(&tcp, &json!({"loai": "chao", "vai_tro": "xem", "ten": self.ten})).map_err(|e| e.to_string())?;
            let tl = doc_dong(&mut tep).map_err(|e| e.to_string())?.unwrap_or_default();
            if chuoi(&tl, "loai") == "loi" {
                return Err(format!("server từ chối: {}", chuoi(&tl, "ly_do")));
            }
            if chuoi(&tl, "loai") != "chao" {
                return Err("server không trả lời 'chao'".into());
            }
            tcp.set_read_timeout(None).map_err(|e| e.to_string())?;
            let ma = tl.get("ma").and_then(Value::as_u64).ok_or("thiếu 'ma'")? as u32;
            let cong_udp = tl.get("cong_udp").and_then(Value::as_u64).ok_or("thiếu 'cong_udp'")? as u16;
            let phat = tl.get("phat").is_some_and(la_dung);
            let ip = tcp.peer_addr().map_err(|e| e.to_string())?.ip();
            let tu = SocketAddr::new(if ip.is_ipv4() { [0, 0, 0, 0].into() } else { std::net::Ipv6Addr::UNSPECIFIED.into() }, 0);
            let udp = udp_dem_lon(tu).map_err(|e| e.to_string())?;
            udp.connect(SocketAddr::new(ip, cong_udp)).map_err(|e| e.to_string())?; // chỉ nhận datagram từ server
            udp.set_read_timeout(Some(Duration::from_millis(200))).map_err(|e| e.to_string())?;
            Ok((ma, phat, udp, tep))
        })();
        let (ma, phat, udp, tep) = match r {
            Ok(x) => x,
            Err(e) => {
                let _ = tcp.shutdown(Shutdown::Both);
                return Err(e);
            }
        };
        let udp = Arc::new(udp);
        let (phien, am, vid) = {
            let mut t = self.khoa.lock().unwrap();
            t.tcp = Some(tcp);
            t.udp = Some(Arc::clone(&udp));
            t.ma = ma;
            t.phien += 1;
            t.rng = Rng::new(ma as u64);
            t.lam_moi_phien();
            t.lam_moi_thong_ke();
            t.cho_phat = !phat;
            t.da_ket_noi = true;
            (t.phien, t.am_thanh, t.video)
        };
        if !(am && vid) {
            self.gui(&json!({"loai": "dang_ky", "am_thanh": am, "video": vid}));
        }
        (self.bao)(&format!("Vào xem: mã {ma}, {}", if phat { "máy phát đang phát" } else { "chưa có ai phát" }));
        // Luồng TCP đọc trước các dòng thong_tin, dong_ho máy chủ gửi kèm lời chào (đã nằm sẵn trong bộ đệm
        // nhận), rồi mới đăng ký UDP: bộ đệm GOP chỉ được gửi sau khi có địa chỉ UDP, nên khi khung I đầu tới
        // người xem đã biết đồng hồ phương tiện -> phân loại nguồn khung đầu (bộ đệm GOP / chờ I) đúng.
        let (tx, rx) = std::sync::mpsc::channel();
        let (a, b) = (Arc::clone(self), Arc::clone(self));
        thread::spawn(move || a.doc_tcp(tep, Some(tx)));
        if phat {
            let _ = rx.recv_timeout(Duration::from_millis(50));
        }
        let _ = udp.send(&goi_giu(ma)); // đăng ký địa chỉ UDP
        thread::spawn(move || b.nhan_udp(udp, phien));
        Ok(ma)
    }

    pub fn gui(&self, obj: &Value) {
        let tcp = self.khoa.lock().unwrap().tcp.as_ref().and_then(|t| t.try_clone().ok());
        if let Some(tcp) = tcp {
            let _k = self.khoa_tcp.lock().unwrap();
            let _ = gui_json(&tcp, obj);
        }
    }

    pub fn ngat(&self) {
        let tcp = {
            let mut t = self.khoa.lock().unwrap();
            t.da_ket_noi = false;
            t.phien += 1; // luồng nhận UDP của phiên cũ tự thoát
            t.udp = None;
            t.tcp.take()
        };
        if let Some(t) = tcp {
            let _ = t.shutdown(Shutdown::Both);
        }
    }

    fn doc_tcp(self: Arc<Self>, mut tep: BufReader<TcpStream>, mut san_sang: Option<std::sync::mpsc::Sender<()>>) {
        while let Ok(Some(tin)) = doc_dong(&mut tep) {
            match chuoi(&tin, "loai") {
                "thong_tin" => {
                    {
                        let mut t = self.khoa.lock().unwrap();
                        t.thong_tin = Some(Value::Object(tin.clone()));
                        if t.cho_phat {
                            // vào trước khi phát: đo từ lúc bắt đầu phát
                            t.cho_phat = false;
                            if t.khung_dau_ms.is_none() {
                                t.luc_moc = bay_gio();
                            }
                        }
                    }
                    (self.bao)(&format!(
                        "Luồng {}x{}, {} fps, GOP {}, {} Hz",
                        hien_gia_tri(tin.get("rong")),
                        hien_gia_tri(tin.get("cao")),
                        hien_gia_tri(tin.get("fps")),
                        hien_gia_tri(tin.get("gop")),
                        hien_gia_tri(tin.get("tan_so_mau"))
                    ));
                }
                "dong_ho" => {
                    let (Some(ts), Some(gio)) = (tin.get("ts").and_then(Value::as_f64), tin.get("gio").and_then(Value::as_f64)) else { break };
                    self.khoa.lock().unwrap().dong_ho_bao = Some((ts, gio));
                }
                "su_kien" => (self.bao)(&format!("Sự kiện: {}", hien_gia_tri(tin.get("noi_dung")))),
                "ket_thuc" => {
                    {
                        let mut t = self.khoa.lock().unwrap();
                        t.thong_tin = None;
                        t.cho_phat = true;
                        t.lam_moi_phien();
                    }
                    (self.bao)("Máy phát kết thúc phát");
                }
                "bi_ngat" => (self.bao)(&format!("Bị ngắt: {}", hien_gia_tri(tin.get("ly_do")))),
                _ => {}
            }
            if tep.buffer().is_empty() {
                san_sang.take().map(|s| s.send(())); // đã xử lý hết các dòng có sẵn
            }
        }
        if self.da_ket_noi() {
            (self.bao)("Mất kết nối server");
            self.ngat();
        }
    }

    // ------------------------------------------------------------ luồng nhận UDP
    fn nhan_udp(self: Arc<Self>, udp: Arc<UdpSocket>, phien: u64) {
        let mut buf = vec![0u8; 2048];
        loop {
            let n = match udp.recv(&mut buf) {
                Ok(n) => Some(n),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::ConnectionReset) => None,
                Err(_) => return,
            };
            let mut t = self.khoa.lock().unwrap();
            if t.phien != phien || !self.song.load(Ordering::SeqCst) {
                return; // đã ngắt / kết nối lại: socket cũ không dùng nữa
            }
            let Some(n) = n else { continue };
            let goi = &buf[..n];
            if n == 0 {
                continue;
            }
            if self.mat_goi > 0.0 && t.rng.random() < self.mat_goi {
                t.bo_gia_lap += 1; // "mạng" làm rơi gói ở chiều xuống
                continue;
            }
            let bay = bay_gio();
            t.byte_nhan += n as u64;
            t.w_byte += n as u64;
            if goi[0] == DAU_RTP {
                if let Some(r) = tach_rtp(goi) {
                    if r.pt == PT_AM_THANH && r.payload.len() == BYTE_MOI_KHUNG && t.am_thanh {
                        t.bo_dem.nhan_goi(r.seq, r.ts, r.payload);
                        t.luc_nhan_am = Some(bay);
                        t.ts_am_moi = Some(r.ts);
                        t.goi_am += 1;
                        t.w_am += 1;
                    }
                }
            } else if goi[0] == PHIEN_BAN && t.video {
                let Some(m) = tach_header(goi) else { continue };
                if m.loai != LOAI_I && m.loai != LOAI_P {
                    continue;
                }
                if let Some(k) = t.ghep.them(&m, bay) {
                    t.nhan_khung(k, bay, &self.bao);
                }
            }
        }
    }

    // ------------------------------------------------------------ luồng phát 20 ms (đồng hồ chủ)
    fn vong_am(self: Arc<Self>) {
        let mut ke_tiep = bay_gio();
        let mut n: u64 = 0;
        let im_lang = vec![0i16; MAU_MOI_KHUNG];
        while self.song.load(Ordering::SeqCst) {
            n += 1;
            let (udp, ma, loa, mau) = {
                let mut t = self.khoa.lock().unwrap();
                let r = if t.da_ket_noi && t.am_thanh { t.bo_dem.lay() } else { None };
                let bay = bay_gio();
                let mut mau = None;
                if let Some((ts, p)) = r {
                    let ts_ms = ts as f64 * MS_MOI_MAU;
                    t.dong_ho_am = ts_ms;
                    t.luc_phat_am = Some(bay);
                    t.lich_su.push_back((ts_ms, bay));
                    while t.lich_su.len() > 150 {
                        t.lich_su.pop_front();
                    }
                    match p {
                        None => mau = Some(im_lang.clone()), // mất gói: phát im lặng
                        Some(p) => {
                            mau = Some(byte_sang_pcm(&p));
                            if let Some((ts_r, gio_r)) = t.dong_ho_bao {
                                // trễ đầu-cuối (cùng đồng hồ)
                                let tre = gio_unix_ms() - (gio_r + (ts_ms - ts_r));
                                t.tre_hien = Some(tre);
                                t.tong_tre += tre;
                                t.n_tre += 1;
                                t.w_tre.push(tre);
                            }
                        }
                    }
                }
                t.muc_am = mau.as_ref().map_or(f64::NEG_INFINITY, |m| muc_dbfs(m));
                (t.udp.clone(), t.ma, t.loa.clone(), mau)
            };
            if let Some(loa) = loa {
                day_ra_loa(&loa, mau.as_deref().unwrap_or(&im_lang));
            }
            if let Some(u) = udp {
                if n % 50 == 0 {
                    let _ = u.send(&goi_giu(ma)); // giữ kết nối mỗi 1 s
                }
            }
            ke_tiep += KHUNG_MS as f64 / 1000.0;
            let con = ke_tiep - bay_gio();
            if con < -0.1 {
                ke_tiep = bay_gio(); // tụt quá xa: bắt nhịp lại
            } else if con > 0.0 {
                ngu(con);
            }
        }
    }

    // ------------------------------------------------------------ luồng hiển thị 5 ms
    fn vong_hien(self: Arc<Self>) {
        while self.song.load(Ordering::SeqCst) {
            ngu(0.005);
            let (mut xin, mut gui_pli) = (None, false);
            {
                let mut t = self.khoa.lock().unwrap();
                if !t.da_ket_noi {
                    continue;
                }
                let bay = bay_gio();
                let thieu = if !t.giai_ma.doi.is_empty() { t.giai_ma.tham_chieu.as_ref().map(|tc| tc.0 + 1) } else { None };
                let duoc_xin = bay - t.luc_pli >= PLI_XEM_S; // giới hạn 1 lần / 1 s
                if t.video && t.giai_ma.kiem_tra(bay) {
                    t.dung_hinh += 1;
                    t.dang_dung = true;
                    let so = thieu.map_or("None".into(), |s| s.to_string());
                    xin = Some(format!("Thiếu khung #{so} -> đứng hình{}", if duoc_xin { ", xin khung khóa" } else { " (xin sau giới hạn 1 s)" }));
                    gui_pli = duoc_xin;
                } else if t.video && t.giai_ma.mat_tham_chieu && duoc_xin {
                    xin = Some("Chưa có khung I: xin khung khóa".into()); // bị giới hạn lúc trước / I bị mất
                    gui_pli = true;
                }
                if gui_pli {
                    t.luc_pli = bay;
                    t.pli_gui += 1;
                }
                t.ghep.don_dep(bay, None);
                if let Some(k) = t.chon_khung(bay) {
                    t.hien_khung(k, bay, &self.bao);
                }
            }
            if let Some(x) = xin {
                (self.bao)(&x);
                if gui_pli {
                    self.gui(&json!({"loai": "xin_khung_khoa"}));
                }
            }
        }
    }

    // ------------------------------------------------------------ điều khiển
    pub fn dat_dong_bo(&self, bat: bool) {
        {
            let mut t = self.khoa.lock().unwrap();
            if bat != t.dong_bo {
                t.dong_bo = bat;
                t.giai_doan.push(GiaiDoan { dong_bo: bat, tong: 0.0, n: 0 });
            }
        }
        (self.bao)(&format!("Đồng bộ tiếng-hình: {}", if bat { "BẬT" } else { "TẮT" }));
    }

    pub fn dang_ky(&self, am_thanh: bool, video: bool) {
        {
            let mut t = self.khoa.lock().unwrap();
            if !video && t.video {
                t.ghep.bo_dang_do();
                t.hang.clear();
            }
            if video && !t.video {
                t.cho_hinh_lai = Some(bay_gio());
            }
            if !am_thanh && t.am_thanh {
                t.bo_dem.xoa();
                t.lich_su.clear();
                (t.luc_phat_am, t.luc_nhan_am) = (None, None);
            }
            if am_thanh && !t.am_thanh {
                t.mat_cu += t.bo_dem.mat();
                t.bo_dem = BoDemJitter::new(t.do_sau);
            }
            t.am_thanh = am_thanh;
            t.video = video;
        }
        self.gui(&json!({"loai": "dang_ky", "am_thanh": am_thanh, "video": video}));
        let ten = match (am_thanh, video) {
            (true, true) => "âm thanh+video",
            (true, false) => "âm thanh",
            _ => "video",
        };
        (self.bao)(&format!("Đăng ký nhận: {ten}"));
    }

    pub fn dat_do_sau(&self, n: usize) {
        let mut t = self.khoa.lock().unwrap();
        t.do_sau = n.clamp(1, 25);
        let d = t.do_sau;
        t.bo_dem.dat_do_sau(d);
    }

    pub fn dat_loa(&self, loa: Option<HangMau>) {
        self.khoa.lock().unwrap().loa = loa;
    }

    pub fn chup(&self) -> Option<Chup> {
        let t = self.khoa.lock().unwrap();
        let h = t.hien.as_ref()?;
        Some(Chup { w: h.w, h: h.h, anh: h.anh.clone(), ts: h.ts, am: h.am, lech: t.lech_hien, so: h.so })
    }

    // ------------------------------------------------------------ số liệu
    /// Gọi mỗi 1 s: số liệu của giây vừa qua (rồi xóa bộ đếm cửa sổ).
    pub fn thong_ke_giay(&self) -> Giay {
        let mut t = self.khoa.lock().unwrap();
        let tb = |v: &Vec<f64>| if v.is_empty() { None } else { Some(v.iter().sum::<f64>() / v.len() as f64) };
        let r = Giay { am: t.w_am, mat: t.mat_cu + t.bo_dem.mat(), hien: t.w_hien, tre: tb(&t.w_tre), lech: tb(&t.w_lech), kbps: t.w_byte as f64 * 8.0 / 1000.0 };
        (t.w_am, t.w_hien, t.w_byte) = (0, 0, 0);
        t.w_tre.clear();
        t.w_lech.clear();
        r
    }

    pub fn trang_thai(&self) -> TrangThai {
        let t = self.khoa.lock().unwrap();
        let giay = (bay_gio() - t.luc_chao).max(0.001);
        TrangThai {
            da_ket_noi: t.da_ket_noi,
            goi_am: t.goi_am,
            mat_am: t.mat_cu + t.bo_dem.mat(),
            tre_tb: if t.n_tre > 0 { Some(t.tong_tre / t.n_tre as f64) } else { None },
            tre_hien: t.tre_hien,
            khung_i: t.khung_i,
            khung_p: t.khung_p,
            khung_hien: t.khung_hien,
            hong: t.hong_cu + t.ghep.khung_hong,
            bo_p: t.bo_p_cu + t.giai_ma.bo_p,
            bo_cu: t.bo_cu,
            dung_hinh: t.dung_hinh,
            pli_gui: t.pli_gui,
            khung_dau_ms: t.khung_dau_ms,
            nguon_dau: t.nguon_dau,
            lech_hien: t.lech_hien,
            giai_doan: t.giai_doan.clone(),
            kbps: t.byte_nhan as f64 * 8.0 / 1000.0 / giay,
            dong_bo: t.dong_bo,
            am_thanh: t.am_thanh,
            video: t.video,
            do_sau: t.do_sau,
            muc_am: t.muc_am,
            bo_gia_lap: t.bo_gia_lap,
        }
    }

    pub fn dong(&self) {
        self.song.store(false, Ordering::SeqCst);
        self.ngat();
        self.khoa.lock().unwrap().loa = None;
    }
}

/// Các dòng TỔNG KẾT của người xem (dùng chung cho dòng lệnh).
pub fn dong_tong_ket(tt: &TrangThai, tre_vao: f64) -> Vec<String> {
    let dau = match tt.khung_dau_ms {
        Some(ms) => format!("khung đầu sau {ms:.0} ms"),
        None => "chưa có khung nào".into(),
    };
    let mut ds = vec![
        "===== TỔNG KẾT =====".to_string(),
        if tre_vao > 0.0 { format!("vào muộn {tre_vao:.1} s; {dau}") } else { format!("vào ngay; {dau}") },
        format!("  nguồn khung đầu: {}", if tt.nguon_dau.is_empty() { "–" } else { tt.nguon_dau }),
        format!("tiếng: nhận {} gói, mất {}, trễ TB {} ms", tt.goi_am, tt.mat_am, chu_ms(tt.tre_tb, false)),
        "  (trễ đầu-cuối: giả định cùng đồng hồ)".to_string(),
        format!("hình: nhận {} khung (I {}, P {}), hỏng {}", tt.khung_i + tt.khung_p, tt.khung_i, tt.khung_p, tt.hong),
        format!("  P bỏ chờ I {}; hiện {}, bỏ cũ {}", tt.bo_p, tt.khung_hien, tt.bo_cu),
        format!("đứng hình {}, xin khung khóa {}", tt.dung_hinh, tt.pli_gui),
        "lệch tiếng-hình TB (+ = hình chậm hơn tiếng):".to_string(),
    ];
    for (i, g) in tt.giai_doan.iter().enumerate() {
        let bt = if g.dong_bo { "BẬT" } else { "TẮT" };
        if g.n > 0 {
            ds.push(format!("  {}. đồng bộ {bt}: {:+.0} ms (n={})", i + 1, g.tong / g.n as f64, g.n));
        } else {
            ds.push(format!("  {}. đồng bộ {bt}: – (không có khung)", i + 1));
        }
    }
    ds.push(format!("nhận TB {:.0} kbit/s", tt.kbps));
    ds
}

/// Ảnh chụp có chú thích: khung đang hiện phóng ×2 + dải chữ nhãn hình, đồng hồ tiếng, lệch.
/// Trả về (rộng, cao, rgb).
pub fn anh_chup(c: &Chup) -> (usize, usize, Vec<u8>) {
    let (w, h) = (c.w, c.h);
    let (ww, hh) = (2 * w, 2 * h + 62);
    let mut ra = phong_to(&c.anh, w, h, 2);
    ra.extend(std::iter::repeat_n([20u8, 20, 20], ww * 62).flatten());
    ve_chu(&mut ra, ww, hh, 6, 2 * h + 6, &format!("HINH  T={:.3} #{}", c.ts as f64 / 1000.0, c.so), 2, [255, 255, 255]);
    let tieng = match c.am {
        None => "TIENG -".to_string(),
        Some(a) => format!("TIENG T={:.3}", a / 1000.0),
    };
    ve_chu(&mut ra, ww, hh, 6, 2 * h + 24, &tieng, 2, [255, 255, 255]);
    let (chu, mau) = match c.lech {
        None => ("LECH -".to_string(), [200, 200, 200]),
        Some(l) => {
            let them = if l < -40.0 {
                " HINH SOM"
            } else if l > 40.0 {
                " HINH TRE"
            } else {
                ""
            };
            (format!("LECH {l:+.0} MS{them}"), if l.abs() <= 40.0 { [120, 255, 120] } else { [255, 120, 120] })
        }
    };
    ve_chu(&mut ra, ww, hh, 6, 2 * h + 42, &chu, 2, mau);
    to_mau(&mut ra, ww, 0, 2 * h, ww, 2, [255, 255, 0]);
    (ww, hh, ra)
}
