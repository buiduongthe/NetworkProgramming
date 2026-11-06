//! Bài mở rộng MR6.5 – Lõi máy phát (camera + micro -> máy chủ), không chạm giao diện.
//!
//! Dùng chung cho phat.rs (dòng lệnh) và phat_gui.rs. Báo sự kiện bằng `bao(text)`; giao diện đọc
//! số liệu (xem trước, mức micro, thống kê, danh sách người xem) qua trang_thai() (dưới khóa).
//!
//! Đồng hồ phương tiện chung: t = 0 khi bấm Phát (đồng hồ đơn điệu, chung::bay_gio()).
//!   - Âm thanh: luồng 20 ms; gói thứ n được gửi lúc t0 + n·20 ms (bám đồng hồ, không trôi), nhãn RTP
//!     ts = n·320 (= số mẫu đã gửi, 16 kHz).
//!   - Video: luồng theo fps; nhãn = ms phương tiện lúc chụp khung. Khung 0 là I; I lặp lại mỗi `gop`
//!     khung hoặc NGAY khung kế tiếp sau khi nhận xin_khung_khoa (đếm GOP lại từ khung I đó).
//!   - Mỗi 1 s gửi {"loai":"dong_ho","ts":ms phương tiện,"gio":ms Unix} (như RTCP Sender Report –
//!     RFC 3550 mục 6.4.1): người xem ánh xạ nhãn thời gian <-> giờ thực để đo độ trễ đầu-cuối.

use crate::camera::{cam_chung, CamChung, CameraGiaLap};
use crate::chung::*;
use crate::may_chu::Bao;
use crate::thiet_bi::MicChung;
use serde_json::{json, Value};
use std::io::BufReader;
use std::net::{Shutdown, SocketAddr, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

struct TT {
    tcp: Option<TcpStream>,
    udp: Option<Arc<UdpSocket>>,
    da_ket_noi: bool,
    ma: u32,
    dang_phat: bool,
    camera: Option<CamChung>,
    mic: Option<MicChung>,
    fps: u32,
    gop: u32,
    nguoi_xem: Vec<String>,                // tên người xem (theo thông điệp nguoi_xem)
    xem_truoc: Option<(Arc<Vec<u8>>, u64)>, // (rgb, ts) khung vừa gửi
    muc_mic: f64,
    t0: f64,
    so: u32,
    seq: u32,
    n_am: u64,
    so_i_cuoi: Option<u32>,
    truoc: Option<Arc<Vec<u8>>>,
    can_i: bool,
    ly_do_i: String,
    so_i: u64,
    so_p: u64,
    byte_i: u64,
    byte_p: u64,
    goi_am: u64,
    pli_nhan: u64,
    p_min: Option<usize>,
    p_max: Option<usize>,
    dem_i: u64,
    dem_p: u64,
    dem_byte_am: u64,
    dem_byte_hinh: u64,
    kbps_am: f64,
    kbps_hinh: f64,
}

impl TT {
    fn lam_moi_thong_ke(&mut self) {
        (self.so_i, self.so_p, self.byte_i, self.byte_p, self.goi_am, self.pli_nhan) = (0, 0, 0, 0, 0, 0);
        (self.p_min, self.p_max) = (None, None);
        (self.dem_i, self.dem_p, self.dem_byte_am, self.dem_byte_hinh) = (0, 0, 0, 0);
        (self.kbps_am, self.kbps_hinh) = (0.0, 0.0);
    }
}

/// Số liệu của giây vừa qua.
pub struct Giay {
    pub i: u64,
    pub p: u64,
    pub kbps_am: f64,
    pub kbps_hinh: f64,
}

/// Ảnh chụp trạng thái máy phát.
pub struct TrangThai {
    pub dang_phat: bool,
    pub da_ket_noi: bool,
    #[allow(dead_code)]
    pub ma: u32,
    pub so_i: u64,
    pub so_p: u64,
    pub tb_i: f64,
    pub tb_p: f64,
    pub p_min: usize,
    pub p_max: usize,
    pub goi_am: u64,
    pub pli_nhan: u64,
    pub kbps_am: f64,
    pub kbps_hinh: f64,
    pub nguoi_xem: Vec<String>,
    pub muc_mic: f64,
    pub xem_truoc: Option<(Arc<Vec<u8>>, u64)>,
    pub fps: u32,
    pub gop: u32,
    pub camera: String,
    #[allow(dead_code)]
    pub mic: String,
}

pub struct MayPhat {
    pub ten: String,
    bao: Bao,
    khoa: Mutex<TT>,
    khoa_tcp: Mutex<()>,
    mat_goi: f64,
    rng: Mutex<(Rng, u64)>, // (bộ sinh số, số gói đã bỏ do giả lập mất gói)
    song: AtomicBool,
}

impl MayPhat {
    pub fn new(ten: &str, bao: Bao, mat_goi: f64) -> Arc<MayPhat> {
        let mp = Arc::new(MayPhat {
            ten: ten.to_string(),
            bao,
            khoa: Mutex::new(TT {
                tcp: None,
                udp: None,
                da_ket_noi: false,
                ma: 0,
                dang_phat: false,
                camera: None,
                mic: None,
                fps: FPS_MAC_DINH,
                gop: GOP_MAC_DINH,
                nguoi_xem: Vec::new(),
                xem_truoc: None,
                muc_mic: f64::NEG_INFINITY,
                t0: 0.0,
                so: 0,
                seq: 0,
                n_am: 0,
                so_i_cuoi: None,
                truoc: None,
                can_i: false,
                ly_do_i: String::new(),
                so_i: 0,
                so_p: 0,
                byte_i: 0,
                byte_p: 0,
                goi_am: 0,
                pli_nhan: 0,
                p_min: None,
                p_max: None,
                dem_i: 0,
                dem_p: 0,
                dem_byte_am: 0,
                dem_byte_hinh: 0,
                kbps_am: 0.0,
                kbps_hinh: 0.0,
            }),
            khoa_tcp: Mutex::new(()),
            mat_goi,
            rng: Mutex::new((Rng::new(1), 0)),
            song: AtomicBool::new(true),
        });
        let (a, b) = (Arc::clone(&mp), Arc::clone(&mp));
        thread::spawn(move || a.vong_am());
        thread::spawn(move || b.vong_hinh());
        mp
    }

    pub fn da_ket_noi(&self) -> bool {
        self.khoa.lock().unwrap().da_ket_noi
    }

    pub fn dang_phat(&self) -> bool {
        self.khoa.lock().unwrap().dang_phat
    }

    pub fn ma(&self) -> u32 {
        self.khoa.lock().unwrap().ma
    }

    pub fn bo_gia_lap(&self) -> u64 {
        self.rng.lock().unwrap().1
    }

    // ------------------------------------------------------------ kết nối
    /// Kết nối TCP, chào với vai trò "phat". Lỗi (kể cả server từ chối) -> Err(lý do).
    pub fn ket_noi(self: &Arc<Self>, host: &str, cong: u16) -> Result<u32, String> {
        let tcp = ket_noi_tcp(host, cong).map_err(|e| e.to_string())?;
        let r = (|| -> Result<(u32, UdpSocket, BufReader<TcpStream>), String> {
            let mut tep = BufReader::new(tcp.try_clone().map_err(|e| e.to_string())?);
            // đặt trên bản sao dùng để đọc (Windows: mỗi bản sao giữ thời hạn riêng)
            tep.get_ref().set_read_timeout(Some(std::time::Duration::from_secs(5))).map_err(|e| e.to_string())?;
            gui_json(&tcp, &json!({"loai": "chao", "vai_tro": "phat", "ten": self.ten})).map_err(|e| e.to_string())?;
            let tl = doc_dong(&mut tep).map_err(|e| e.to_string())?.unwrap_or_default();
            if chuoi(&tl, "loai") == "loi" {
                return Err(format!("server từ chối: {}", chuoi(&tl, "ly_do")));
            }
            if chuoi(&tl, "loai") != "chao" {
                return Err("server không trả lời 'chao'".into());
            }
            tep.get_ref().set_read_timeout(None).map_err(|e| e.to_string())?;
            let ma = tl.get("ma").and_then(Value::as_u64).ok_or("thiếu 'ma'")? as u32;
            let cong_udp = tl.get("cong_udp").and_then(Value::as_u64).ok_or("thiếu 'cong_udp'")? as u16;
            let ip = tcp.peer_addr().map_err(|e| e.to_string())?.ip();
            let tu = SocketAddr::new(if ip.is_ipv4() { [0, 0, 0, 0].into() } else { std::net::Ipv6Addr::UNSPECIFIED.into() }, 0);
            let udp = udp_dem_lon(tu).map_err(|e| e.to_string())?;
            udp.connect(SocketAddr::new(ip, cong_udp)).map_err(|e| e.to_string())?;
            Ok((ma, udp, tep))
        })();
        let (ma, udp, tep) = match r {
            Ok(x) => x,
            Err(e) => {
                let _ = tcp.shutdown(Shutdown::Both);
                return Err(e);
            }
        };
        let udp = Arc::new(udp);
        {
            let mut t = self.khoa.lock().unwrap();
            t.tcp = Some(tcp);
            t.udp = Some(Arc::clone(&udp));
            t.ma = ma;
            t.da_ket_noi = true;
        }
        let _ = udp.send(&goi_giu(ma)); // đăng ký địa chỉ UDP ngay
        let s = Arc::clone(self);
        thread::spawn(move || s.doc_tcp(tep));
        Ok(ma)
    }

    pub fn gui(&self, obj: &Value) {
        let tcp = self.khoa.lock().unwrap().tcp.as_ref().and_then(|t| t.try_clone().ok());
        if let Some(tcp) = tcp {
            let _k = self.khoa_tcp.lock().unwrap();
            let _ = gui_json(&tcp, obj);
        }
    }

    fn doc_tcp(self: Arc<Self>, mut tep: BufReader<TcpStream>) {
        while let Ok(Some(tin)) = doc_dong(&mut tep) {
            match chuoi(&tin, "loai") {
                "xin_khung_khoa" => {
                    let mut t = self.khoa.lock().unwrap();
                    t.pli_nhan += 1;
                    let tu = tin.get("tu").map_or("?".to_string(), |v| hien_gia_tri(Some(v)));
                    t.ly_do_i = format!("PLI từ {tu}");
                    t.can_i = true;
                }
                "nguoi_xem" => {
                    let ds = tin.get("ds").and_then(Value::as_array).map(|a| a.iter().map(|v| hien_gia_tri(Some(v))).collect()).unwrap_or_default();
                    self.khoa.lock().unwrap().nguoi_xem = ds;
                }
                "su_kien" => (self.bao)(&format!("Sự kiện: {}", hien_gia_tri(tin.get("noi_dung")))),
                "loi" => (self.bao)(&format!("Lỗi từ server: {}", hien_gia_tri(tin.get("ly_do")))),
                _ => {}
            }
        }
        if self.da_ket_noi() {
            (self.bao)("Mất kết nối server");
            self.ngat();
        }
    }

    pub fn ngat(&self) {
        let (tcp, _udp) = {
            let mut t = self.khoa.lock().unwrap();
            t.da_ket_noi = false;
            t.dang_phat = false;
            t.nguoi_xem.clear();
            (t.tcp.take(), t.udp.take())
        };
        if let Some(t) = tcp {
            let _ = t.shutdown(Shutdown::Both);
        }
    }

    // ------------------------------------------------------------ phát / dừng
    fn thong_tin(t: &TT) -> Value {
        json!({"loai": "thong_tin", "rong": RONG, "cao": CAO, "fps": t.fps, "gop": t.gop, "tan_so_mau": TAN_SO_MAU})
    }

    pub fn bat_dau_phat(&self, camera: CamChung, mic: MicChung, fps: u32, gop: u32) {
        let tin = {
            let mut t = self.khoa.lock().unwrap();
            t.camera = Some(camera.clone());
            t.mic = Some(mic);
            t.fps = fps.clamp(1, 30);
            t.gop = gop.clamp(1, 300);
            Self::thong_tin(&t)
        };
        // thong_tin đi TCP, gói phương tiện đi UDP trên luồng khác: gửi thong_tin trước và chờ 20 ms để máy
        // chủ kịp chuyển sang "đang phát" – nếu không, khung I #0 có thể tới trước, bị bỏ vì "chưa phát",
        // và bộ đệm GOP rỗng tới khung I kế tiếp.
        self.gui(&tin);
        ngu(0.02);
        let (fps, gop) = {
            let mut t = self.khoa.lock().unwrap();
            t.t0 = bay_gio();
            (t.so, t.seq, t.n_am) = (0, 0, 0);
            (t.so_i_cuoi, t.truoc) = (None, None);
            (t.can_i, t.ly_do_i) = (false, String::new());
            t.lam_moi_thong_ke();
            t.dang_phat = true;
            camera.lock().unwrap().dat_fps(t.fps);
            (t.fps, t.gop)
        };
        (self.bao)(&format!("Bắt đầu phát {RONG}x{CAO} {fps} fps GOP {gop}"));
    }

    pub fn dat_tham_so(&self, fps: u32, gop: u32) {
        let (phat, tin, f, g) = {
            let mut t = self.khoa.lock().unwrap();
            t.fps = fps.clamp(1, 30);
            t.gop = gop.clamp(1, 300);
            if let Some(c) = &t.camera {
                c.lock().unwrap().dat_fps(t.fps);
            }
            (t.dang_phat, Self::thong_tin(&t), t.fps, t.gop)
        };
        if phat {
            self.gui(&tin);
        }
        (self.bao)(&format!("Tham số: {f} fps, GOP {g}"));
    }

    pub fn khung_khoa_ngay(&self, ly_do: &str) {
        let mut t = self.khoa.lock().unwrap();
        t.can_i = true;
        t.ly_do_i = ly_do.to_string();
    }

    pub fn dung_phat(&self) {
        let (cam, mic) = {
            let mut t = self.khoa.lock().unwrap();
            if !t.dang_phat {
                return;
            }
            t.dang_phat = false;
            (t.camera.take(), t.mic.take())
        };
        self.gui(&json!({"loai": "dung_phat"}));
        if let Some(c) = cam {
            c.lock().unwrap().dong();
        }
        if let Some(m) = mic {
            m.lock().unwrap().dong();
        }
        (self.bao)("Dừng phát");
    }

    /// Gửi một datagram; --mat-goi giả lập mạng làm rơi gói ở chiều lên.
    fn gui_udp(&self, udp: &UdpSocket, goi: &[u8]) {
        if self.mat_goi > 0.0 {
            let mut r = self.rng.lock().unwrap();
            if r.0.random() < self.mat_goi {
                r.1 += 1;
                return;
            }
        }
        let _ = udp.send(goi);
    }

    // ------------------------------------------------------------ luồng âm thanh 20 ms
    fn vong_am(self: Arc<Self>) {
        let mut han_giu = bay_gio();
        while self.song.load(Ordering::SeqCst) {
            let bay = bay_gio();
            let (phat, mic, ma, udp, t0, n_am, seq) = {
                let t = self.khoa.lock().unwrap();
                (t.dang_phat, t.mic.clone(), t.ma, t.udp.clone(), t.t0, t.n_am, t.seq)
            };
            if let Some(u) = &udp {
                if bay >= han_giu {
                    self.gui_udp(u, &goi_giu(ma)); // giữ kết nối mỗi 1 s
                    han_giu = bay + 1.0;
                }
            }
            let (true, Some(mic), Some(udp)) = (phat, mic, udp) else {
                self.khoa.lock().unwrap().muc_mic = f64::NEG_INFINITY;
                ngu(0.02);
                continue;
            };
            let han = t0 + n_am as f64 * KHUNG_MS as f64 / 1000.0;
            if bay < han {
                ngu((han - bay).min(0.02));
                continue;
            }
            if n_am % 50 == 0 {
                // báo cáo đồng hồ mỗi 1 s
                self.gui(&json!({"loai": "dong_ho", "ts": ((bay - t0) * 1000.0) as i64, "gio": gio_unix_ms() as i64}));
            }
            let khung = mic.lock().unwrap().lay_khung();
            let muc = muc_dbfs(&khung);
            let goi = dong_goi_rtp(seq, (n_am * MAU_MOI_KHUNG as u64) as u32, ma, &pcm_sang_byte(&khung));
            self.gui_udp(&udp, &goi);
            let mut t = self.khoa.lock().unwrap();
            t.muc_mic = muc;
            t.seq = t.seq.wrapping_add(1);
            t.n_am += 1;
            t.goi_am += 1;
            t.dem_byte_am += goi.len() as u64;
        }
    }

    // ------------------------------------------------------------ luồng video theo fps
    fn vong_hinh(self: Arc<Self>) {
        let mut han: Option<f64> = None;
        while self.song.load(Ordering::SeqCst) {
            let bay = bay_gio();
            let (phat, cam, ma, udp, t0, fps) = {
                let t = self.khoa.lock().unwrap();
                (t.dang_phat, t.camera.clone(), t.ma, t.udp.clone(), t.t0, t.fps)
            };
            let (true, Some(mut cam), Some(udp)) = (phat, cam, udp) else {
                han = None;
                ngu(0.01);
                continue;
            };
            let mut h = han.filter(|&h| h >= t0).unwrap_or(t0);
            if bay < h {
                han = Some(h);
                ngu((h - bay).min(0.01));
                continue;
            }
            h += 1.0 / fps as f64;
            if h < bay - 0.2 {
                h = bay; // tụt quá xa (máy bận): bắt nhịp lại
            }
            han = Some(h);
            let loi = cam.lock().unwrap().loi();
            if !loi.is_empty() {
                // ffmpeg dừng/lỗi: lùi về giả lập
                let ngan: String = loi.chars().take(28).collect();
                (self.bao)(&format!("Camera lỗi, dùng giả lập: {ngan}"));
                cam.lock().unwrap().dong();
                cam = cam_chung(CameraGiaLap::new(&self.ten, if ma == 0 { 1 } else { ma }, fps));
                self.khoa.lock().unwrap().camera = Some(cam.clone());
            }
            let ts_ms = ((bay - t0) * 1000.0) as u64;
            let so_hien = self.khoa.lock().unwrap().so;
            let Some(anh) = cam.lock().unwrap().lay_khung(so_hien, ts_ms) else { continue };
            if anh.len() != RONG * CAO * 3 {
                continue; // camera chưa có khung
            }
            let (so, la_i, ly_do, truoc) = {
                let mut t = self.khoa.lock().unwrap();
                let la_i = t.so_i_cuoi.is_none_or(|s| t.so.wrapping_sub(s) >= t.gop) || t.can_i;
                let ly_do = if t.can_i { std::mem::take(&mut t.ly_do_i) } else { String::new() };
                t.can_i = false;
                t.ly_do_i.clear();
                (t.so, la_i, ly_do, t.truoc.clone())
            };
            let (loai, du_lieu) = match (la_i, &truoc) {
                (false, Some(tr)) => (LOAI_P, ma_hoa_p(&anh, tr)),
                _ => (LOAI_I, ma_hoa_i(&anh)),
            };
            let manh = chia_manh(loai, ma as u16, so, RONG, CAO, ts_ms, &du_lieu);
            for g in &manh {
                self.gui_udp(&udp, g);
            }
            if !ly_do.is_empty() {
                (self.bao)(&format!("{ly_do} -> gửi khung I #{so}"));
            }
            let anh = Arc::new(anh);
            let mut t = self.khoa.lock().unwrap();
            t.truoc = Some(anh.clone());
            t.so = t.so.wrapping_add(1);
            if loai == LOAI_I {
                t.so_i_cuoi = Some(so);
                t.so_i += 1;
                t.byte_i += du_lieu.len() as u64;
                t.dem_i += 1;
            } else {
                t.so_p += 1;
                t.byte_p += du_lieu.len() as u64;
                t.p_min = Some(t.p_min.map_or(du_lieu.len(), |m| m.min(du_lieu.len())));
                t.p_max = Some(t.p_max.map_or(du_lieu.len(), |m| m.max(du_lieu.len())));
                t.dem_p += 1;
            }
            t.dem_byte_hinh += manh.iter().map(|g| g.len() as u64).sum::<u64>();
            t.xem_truoc = Some((anh, ts_ms));
        }
    }

    // ------------------------------------------------------------ số liệu
    /// Gọi mỗi 1 s (dòng lệnh hoặc giao diện): trả về và xóa bộ đếm của giây vừa qua.
    pub fn thong_ke_giay(&self) -> Giay {
        let mut t = self.khoa.lock().unwrap();
        let r = Giay { i: t.dem_i, p: t.dem_p, kbps_am: t.dem_byte_am as f64 * 8.0 / 1000.0, kbps_hinh: t.dem_byte_hinh as f64 * 8.0 / 1000.0 };
        t.kbps_am = r.kbps_am;
        t.kbps_hinh = r.kbps_hinh;
        (t.dem_i, t.dem_p, t.dem_byte_am, t.dem_byte_hinh) = (0, 0, 0, 0);
        r
    }

    pub fn trang_thai(&self) -> TrangThai {
        let t = self.khoa.lock().unwrap();
        TrangThai {
            dang_phat: t.dang_phat,
            da_ket_noi: t.da_ket_noi,
            ma: t.ma,
            so_i: t.so_i,
            so_p: t.so_p,
            tb_i: if t.so_i > 0 { t.byte_i as f64 / t.so_i as f64 } else { 0.0 },
            tb_p: if t.so_p > 0 { t.byte_p as f64 / t.so_p as f64 } else { 0.0 },
            p_min: t.p_min.unwrap_or(0),
            p_max: t.p_max.unwrap_or(0),
            goi_am: t.goi_am,
            pli_nhan: t.pli_nhan,
            kbps_am: t.kbps_am,
            kbps_hinh: t.kbps_hinh,
            nguoi_xem: t.nguoi_xem.clone(),
            muc_mic: t.muc_mic,
            xem_truoc: t.xem_truoc.clone(),
            fps: t.fps,
            gop: t.gop,
            camera: t.camera.as_ref().map(|c| c.lock().unwrap().mo_ta()).unwrap_or_default(),
            mic: t.mic.as_ref().map(|m| m.lock().unwrap().mo_ta()).unwrap_or_default(),
        }
    }

    pub fn dong(&self) {
        self.dung_phat();
        self.song.store(false, Ordering::SeqCst);
        self.ngat();
    }
}

/// Các dòng tổng kết của máy phát (dùng chung cho dòng lệnh).
pub fn dong_tong_ket(tt: &TrangThai, giay: f64) -> Vec<String> {
    vec![
        format!("===== TỔNG KẾT sau {giay:.1} s ====="),
        format!("khung I {}, P {}; TB I {:.0} B, P {:.0} B", tt.so_i, tt.so_p, tt.tb_i, tt.tb_p),
        format!("P nhỏ nhất {} B, lớn nhất {} B (khung chớp)", tt.p_min, tt.p_max),
        format!("I/thô {:.1} %, P TB/I TB {:.1} %", tt.tb_i / (RONG * CAO * 3) as f64 * 100.0, if tt.tb_i > 0.0 { tt.tb_p / tt.tb_i * 100.0 } else { 0.0 }),
        format!("tiếng {} gói; PLI nhận {}", tt.goi_am, tt.pli_nhan),
    ]
}
