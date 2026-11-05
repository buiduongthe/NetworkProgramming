//! Bài mở rộng MR6.5 – Lõi máy chủ trung chuyển phát trực tiếp 1→n (kiểu SFU).
//!
//! Dùng chung cho server_phat.rs (dòng lệnh) và server_gui.rs (giao diện); không chạm giao diện, báo
//! sự kiện bằng hàm `bao(text)`. Start/Stop được nhiều lần; mọi socket có thời gian chờ để dừng gọn.
//!
//! Kiến trúc SFU (Selective Forwarding Unit – RFC 7667 mục 3.7): máy chủ KHÔNG giải nén, KHÔNG trộn;
//! mỗi datagram của máy phát được NHÂN BẢN nguyên byte tới từng người xem (fan-out):
//!     băng thông ra ≈ n × băng thông vào; CPU chỉ tốn cho send_to() – khác MCU ở MR6.3.
//! Máy chủ chỉ đọc header (byte đầu, loại khung, số khung) để: đếm gói/byte, lọc theo đăng ký (âm
//! thanh/video) và giữ BỘ ĐỆM GOP: khung I mới nhất + mọi khung P sau nó (như GOP cache của SRS,
//! nginx-rtmp). Người vào muộn nhận ngay cả nhóm -> giải được hình tức thì, không phải chờ I kế tiếp.
//!
//! Luồng: một luồng nhận kết nối TCP, mỗi client một luồng đọc TCP, một luồng nhận UDP chuyển tiếp
//! ngay (mọi gói phương tiện đi ra từ CHỈ luồng này nên thứ tự gói được giữ). Trạng thái chung
//! (danh sách client, bộ đệm GOP, thống kê) nằm trong một Mutex.

use crate::chung::*;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::io::BufReader;
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub type Bao = Arc<dyn Fn(&str) + Send + Sync>;

/// Một kết nối: máy phát (phat = true) hoặc người xem.
pub struct Khach {
    pub ma: u32,
    pub ten: String,
    pub phat: bool,
    conn: TcpStream,
    ip: IpAddr,
    pub udp: Option<SocketAddr>, // địa chỉ UDP đã đăng ký
    pub am_thanh: bool,          // đăng ký nhận (người xem)
    pub video: bool,
    luc_chao: f64,
    luc_moc: f64, // mốc đo khung I đầu: lúc vào, hoặc lúc bắt đầu phát nếu muộn hơn
    can_bo_dem: bool, // chờ luồng UDP gửi bộ đệm GOP cho riêng người này
    pub goi_ra: u64,
    pub byte_ra: u64,
    dem_byte: u64, // cửa sổ 1 giây
    pub kbps: f64,
    pub khung_dau_ms: Option<f64>, // từ lúc chào tới mảnh khung I đầu tiên gửi cho người này
    pub nguon_dau: &'static str,   // "bộ đệm" hoặc "trực tiếp"
    pub pli: u64,
}

impl Khach {
    fn gui(&self, obj: &Value) {
        let _ = gui_json(&self.conn, obj);
    }

    pub fn nhan(&self) -> &'static str {
        match (self.am_thanh, self.video) {
            (true, true) => "A+V",
            (true, false) => "A",
            (false, true) => "V",
            _ => "-",
        }
    }

    fn tom_tat(&self) -> TomTat {
        TomTat {
            ma: self.ma,
            ten: self.ten.clone(),
            udp: self.udp.map(|a| a.to_string()).unwrap_or_else(|| "–".into()),
            nhan: self.nhan(),
            goi_ra: self.goi_ra,
            byte_ra: self.byte_ra,
            kbps: self.kbps,
            khung_dau_ms: self.khung_dau_ms,
            nguon_dau: self.nguon_dau,
            pli: self.pli,
            giay: bay_gio() - self.luc_chao,
        }
    }
}

/// Bản sao gọn một người xem (bảng của dòng lệnh và giao diện).
#[derive(Clone)]
pub struct TomTat {
    pub ma: u32,
    pub ten: String,
    pub udp: String,
    pub nhan: &'static str,
    pub goi_ra: u64,
    pub byte_ra: u64,
    pub kbps: f64,
    pub khung_dau_ms: Option<f64>,
    pub nguon_dau: &'static str,
    #[allow(dead_code)]
    pub pli: u64,
    pub giay: f64,
}

/// Ảnh xem trước do server tự giải mã: (rộng, cao, rgb, nhãn ms, số khung).
pub type AnhXemTruoc = (usize, usize, Arc<Vec<u8>>, u32, u32);

/// Trạng thái chung của máy chủ (dưới một khóa).
pub struct TT {
    ds: BTreeMap<u32, Khach>, // mã -> Khach (thứ tự mã = thứ tự vào)
    may_phat: Option<u32>,
    dang_phat: bool,
    thong_tin: Option<Value>,
    dong_ho: Option<Value>,
    bo_dem: Vec<Arc<Vec<u8>>>,
    bo_dem_byte: usize,
    bo_dem_so_i: Option<u32>,
    pub bo_dem_bat: bool,
    pub xem_truoc: bool, // true: tự giải mã một bản để giao diện hiện ảnh xem trước
    byte_vao: u64,
    goi_vao: u64,
    byte_ra: u64,
    goi_ra: u64,
    dem_vao_am: u64,
    dem_vao_hinh: u64,
    dem_ra: u64,
    kbps_vao_am: f64,
    kbps_vao_hinh: f64,
    kbps_ra: f64,
    pli_chuyen: u64,
    pli_bo: u64,
    luc_pli: f64,
    sai_nguon: u64,
    da_roi: Vec<TomTat>, // người xem đã rời (bảng tổng kết)
    anh_xem_truoc: Option<AnhXemTruoc>,
    ghep: GhepKhung,
    giai_ma: GiaiMaVideo,
    ma_moi: u32,
    cong_udp: u16,
}

impl TT {
    fn moi(bo_dem_bat: bool, xem_truoc: bool) -> TT {
        TT {
            ds: BTreeMap::new(),
            may_phat: None,
            dang_phat: false,
            thong_tin: None,
            dong_ho: None,
            bo_dem: Vec::new(),
            bo_dem_byte: 0,
            bo_dem_so_i: None,
            bo_dem_bat,
            xem_truoc,
            byte_vao: 0,
            goi_vao: 0,
            byte_ra: 0,
            goi_ra: 0,
            dem_vao_am: 0,
            dem_vao_hinh: 0,
            dem_ra: 0,
            kbps_vao_am: 0.0,
            kbps_vao_hinh: 0.0,
            kbps_ra: 0.0,
            pli_chuyen: 0,
            pli_bo: 0,
            luc_pli: -1e9,
            sai_nguon: 0,
            da_roi: Vec::new(),
            anh_xem_truoc: None,
            ghep: GhepKhung::new(),
            giai_ma: GiaiMaVideo::new(),
            ma_moi: 1,
            cong_udp: 0,
        }
    }

    fn ds_nguoi_xem(&self) -> Vec<u32> {
        self.ds.values().filter(|k| !k.phat).map(|k| k.ma).collect()
    }
}

/// Ảnh chụp trạng thái cho dòng lệnh và giao diện.
pub struct TrangThai {
    pub dang_phat: bool,
    pub ten_phat: Option<String>,
    pub bo_dem_bat: bool,
    pub bo_dem_goi: usize,
    pub bo_dem_byte: usize,
    pub bo_dem_so_i: Option<u32>,
    pub kbps_vao: f64,
    pub kbps_vao_am: f64,
    pub kbps_vao_hinh: f64,
    pub kbps_ra: f64,
    pub he_so: f64,
    pub byte_vao: u64,
    pub byte_ra: u64,
    pub goi_vao: u64,
    pub goi_ra: u64,
    pub pli_chuyen: u64,
    pub pli_bo: u64,
    pub nguoi_xem: Vec<TomTat>,
    pub da_roi: Vec<TomTat>,
    pub anh: Option<AnhXemTruoc>,
}

pub struct MayChuPhat {
    bao: Bao,
    pub khoa: Mutex<TT>,
    chay: AtomicBool,
    luong: Mutex<Vec<JoinHandle<()>>>,
    pub dia_chi: Mutex<String>,
}

impl MayChuPhat {
    pub fn new(bao: Bao) -> Arc<MayChuPhat> {
        Arc::new(MayChuPhat { bao, khoa: Mutex::new(TT::moi(true, false)), chay: AtomicBool::new(false), luong: Mutex::new(Vec::new()), dia_chi: Mutex::new(String::new()) })
    }

    pub fn dang_chay(&self) -> bool {
        self.chay.load(Ordering::SeqCst)
    }

    pub fn cong_udp(&self) -> u16 {
        self.khoa.lock().unwrap().cong_udp
    }

    // ------------------------------------------------------------ khởi động / dừng
    pub fn bat_dau(self: &Arc<Self>, host: &str, cong_tcp: u16, cong_udp: u16) -> std::io::Result<()> {
        let srv = TcpListener::bind((host, cong_tcp))?;
        let dia_udp = srv.local_addr().map(|a| SocketAddr::new(a.ip(), cong_udp))?;
        let udp = udp_dem_lon(dia_udp)?;
        srv.set_nonblocking(true)?; // vòng nhận kết nối kiểm tra cờ dừng mỗi 50 ms
        udp.set_read_timeout(Some(Duration::from_millis(500)))?;
        {
            let mut t = self.khoa.lock().unwrap();
            let (bat, xem) = (t.bo_dem_bat, t.xem_truoc);
            *t = TT::moi(bat, xem);
            t.cong_udp = cong_udp;
        }
        *self.dia_chi.lock().unwrap() = format!("{host}:{cong_tcp}");
        self.chay.store(true, Ordering::SeqCst);
        let (a, b) = (Arc::clone(self), Arc::clone(self));
        *self.luong.lock().unwrap() = vec![thread::spawn(move || a.nhan_ket_noi(srv)), thread::spawn(move || b.nhan_udp(udp))];
        let bat = self.khoa.lock().unwrap().bo_dem_bat;
        (self.bao)(&format!("TCP {host}:{cong_tcp}, UDP {cong_udp}; bộ đệm GOP: {}", if bat { "bật" } else { "tắt" }));
        Ok(())
    }

    pub fn dung(&self) {
        if !self.chay.swap(false, Ordering::SeqCst) {
            return;
        }
        for k in self.khoa.lock().unwrap().ds.values() {
            let _ = k.conn.shutdown(Shutdown::Both);
        }
        for t in self.luong.lock().unwrap().drain(..) {
            let _ = t.join();
        }
        (self.bao)("Server đã dừng");
    }

    // ------------------------------------------------------------ TCP điều khiển
    /// Gửi sự kiện cho mọi client (gọi khi đang giữ khóa).
    fn su_kien(&self, t: &TT, text: &str) {
        (self.bao)(&format!("=> {text}"));
        let tin = json!({"loai": "su_kien", "noi_dung": text});
        for k in t.ds.values() {
            k.gui(&tin);
        }
    }

    /// Gửi máy phát danh sách người xem mỗi khi danh sách đổi (gọi khi giữ khóa).
    fn bao_nguoi_xem(&self, t: &TT) {
        if let Some(p) = t.may_phat.and_then(|m| t.ds.get(&m)) {
            let ds: Vec<&str> = t.ds.values().filter(|k| !k.phat).map(|k| k.ten.as_str()).collect();
            p.gui(&json!({"loai": "nguoi_xem", "so": ds.len(), "ds": ds}));
        }
    }

    fn nhan_ket_noi(self: Arc<Self>, srv: TcpListener) {
        while self.dang_chay() {
            match srv.accept() {
                Ok((conn, addr)) => {
                    let _ = conn.set_nonblocking(false);
                    let _ = conn.set_nodelay(true);
                    let _ = conn.set_write_timeout(Some(Duration::from_secs(5)));
                    let s = Arc::clone(&self);
                    thread::spawn(move || s.phuc_vu(conn, addr));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(50)),
                Err(_) => break,
            }
        }
    }

    fn phuc_vu(self: Arc<Self>, conn: TcpStream, addr: SocketAddr) {
        let Ok(doc) = conn.try_clone() else { return };
        let mut tep = BufReader::new(doc);
        let mut ma = None;
        let _ = (|| -> std::io::Result<()> {
            let Some(tin) = doc_dong(&mut tep)? else { return Ok(()) };
            let vai_tro = chuoi(&tin, "vai_tro");
            if chuoi(&tin, "loai") != "chao" || !(vai_tro == "phat" || vai_tro == "xem") {
                return Ok(());
            }
            let ten = chuoi(&tin, "ten").to_string();
            if !tin.get("ten").is_some_and(Value::is_string) || !ten_hop_le(&ten) {
                gui_json(&conn, &json!({"loai": "loi", "ly_do": "tên không hợp lệ"}))?;
                return Ok(());
            }
            {
                let mut t = self.khoa.lock().unwrap();
                if vai_tro == "phat" && t.may_phat.is_some() {
                    gui_json(&conn, &json!({"loai": "loi", "ly_do": "đã có máy phát"}))?;
                    (self.bao)(&format!("Từ chối máy phát thứ hai {ten}"));
                    return Ok(());
                }
                let m = t.ma_moi;
                t.ma_moi += 1;
                let bay = bay_gio();
                let k = Khach {
                    ma: m,
                    ten: ten.clone(),
                    phat: vai_tro == "phat",
                    conn: conn.try_clone()?,
                    ip: addr.ip(),
                    udp: None,
                    am_thanh: true,
                    video: true,
                    luc_chao: bay,
                    luc_moc: bay,
                    can_bo_dem: false,
                    goi_ra: 0,
                    byte_ra: 0,
                    dem_byte: 0,
                    kbps: 0.0,
                    khung_dau_ms: None,
                    nguon_dau: "",
                    pli: 0,
                };
                k.gui(&json!({"loai": "chao", "ma": m, "cong_udp": t.cong_udp, "phat": t.dang_phat}));
                let phat = k.phat;
                if !phat {
                    if let Some(x) = &t.thong_tin {
                        k.gui(x); // bản đã lưu cho người vào sau
                    }
                    if let Some(x) = &t.dong_ho {
                        k.gui(x);
                    }
                }
                t.ds.insert(m, k);
                ma = Some(m);
                if phat {
                    t.may_phat = Some(m);
                    (self.bao)(&format!("#{m} {ten} (phát) vào từ {addr}"));
                    self.su_kien(&t, &format!("{ten} kết nối (máy phát)"));
                } else {
                    (self.bao)(&format!("#{m} {ten} (xem) vào từ {addr}"));
                    self.su_kien(&t, &format!("{ten} vào xem"));
                }
                self.bao_nguoi_xem(&t);
            }
            while self.dang_chay() {
                let Some(tin) = doc_dong(&mut tep)? else { break };
                let mut t = self.khoa.lock().unwrap();
                self.xu_ly(&mut t, m_cua(ma), tin);
            }
            Ok(())
        })();
        if let Some(m) = ma {
            let mut t = self.khoa.lock().unwrap();
            self.roi(&mut t, m, "rời");
        }
        let _ = conn.shutdown(Shutdown::Both);
    }

    fn roi(&self, t: &mut TT, ma: u32, ly_do: &str) {
        let Some(k) = t.ds.remove(&ma) else { return };
        if t.may_phat == Some(ma) {
            if t.dang_phat {
                self.ket_thuc_phat(t);
            }
            t.may_phat = None;
            self.su_kien(t, &format!("{} (máy phát) thoát", k.ten));
        } else {
            t.da_roi.push(k.tom_tat());
            self.su_kien(t, &format!("{} {ly_do}", k.ten));
            self.bao_nguoi_xem(t);
        }
    }

    fn ket_thuc_phat(&self, t: &mut TT) {
        t.dang_phat = false;
        t.thong_tin = None;
        t.dong_ho = None;
        t.bo_dem.clear();
        t.bo_dem_byte = 0;
        t.bo_dem_so_i = None;
        t.ghep = GhepKhung::new();
        t.giai_ma = GiaiMaVideo::new();
        let tin = json!({"loai": "ket_thuc"});
        for x in t.ds.values() {
            x.gui(&tin);
        }
    }

    fn xu_ly(&self, t: &mut TT, ma: u32, tin: Map<String, Value>) {
        let loai = chuoi(&tin, "loai").to_string();
        let Some(k) = t.ds.get(&ma) else { return };
        let ten = k.ten.clone();
        if k.phat {
            let v = Value::Object(tin.clone());
            match loai.as_str() {
                "thong_tin" => {
                    for x in t.ds.values().filter(|x| !x.phat) {
                        x.gui(&v); // chuyển tiếp nguyên văn
                    }
                    t.thong_tin = Some(v);
                    (self.bao)(&format!(
                        "{ten} phát {}x{} {} fps GOP {}, {} Hz",
                        hien_gia_tri(tin.get("rong")),
                        hien_gia_tri(tin.get("cao")),
                        hien_gia_tri(tin.get("fps")),
                        hien_gia_tri(tin.get("gop")),
                        hien_gia_tri(tin.get("tan_so_mau"))
                    ));
                    if !t.dang_phat {
                        t.dang_phat = true;
                        let bay = bay_gio();
                        for x in t.ds.values_mut().filter(|x| !x.phat) {
                            if x.khung_dau_ms.is_none() {
                                x.luc_moc = bay; // người vào trước lúc phát: đo từ lúc phát
                            }
                        }
                        self.su_kien(t, &format!("{ten} bắt đầu phát"));
                    }
                }
                "dong_ho" => {
                    for x in t.ds.values().filter(|x| !x.phat) {
                        x.gui(&v);
                    }
                    t.dong_ho = Some(v);
                }
                "dung_phat" if t.dang_phat => {
                    self.ket_thuc_phat(t);
                    self.su_kien(t, &format!("{ten} dừng phát"));
                }
                _ => {}
            }
            return;
        }
        if loai == "dang_ky" {
            let bat = t.bo_dem_bat;
            let k = t.ds.get_mut(&ma).unwrap();
            let co_video = k.video;
            k.am_thanh = tin.get("am_thanh").map_or(k.am_thanh, la_dung);
            k.video = tin.get("video").map_or(k.video, la_dung);
            if k.video && !co_video && bat && k.udp.is_some() {
                k.can_bo_dem = true; // bật lại video: gửi bộ đệm GOP
            }
            (self.bao)(&format!("{ten} đăng ký nhận: {}", k.nhan()));
        } else if loai == "xin_khung_khoa" {
            t.ds.get_mut(&ma).unwrap().pli += 1;
            let bay = bay_gio();
            let phat = t.may_phat.and_then(|m| t.ds.get(&m)).map(|p| p.ten.clone());
            match phat {
                Some(ten_phat) if t.dang_phat && bay - t.luc_pli >= PLI_SERVER_S => {
                    t.luc_pli = bay;
                    t.pli_chuyen += 1;
                    t.ds[&t.may_phat.unwrap()].gui(&json!({"loai": "xin_khung_khoa", "tu": ten}));
                    (self.bao)(&format!("PLI từ {ten} -> {ten_phat}"));
                }
                _ => {
                    t.pli_bo += 1;
                    (self.bao)(&format!("PLI từ {ten}: bỏ (lọc 500 ms)"));
                }
            }
        }
    }

    // ------------------------------------------------------------ UDP: nhận và nhân bản
    fn nhan_udp(self: Arc<Self>, sock: UdpSocket) {
        let mut moc = bay_gio();
        let mut buf = vec![0u8; 2048];
        while self.dang_chay() {
            let goi = match sock.recv_from(&mut buf) {
                Ok((n, addr)) => Some((buf[..n].to_vec(), addr)),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => None,
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => None, // Windows: ICMP "cổng không mở"
                Err(_) => break,
            };
            let bay = bay_gio();
            self.gui_bo_dem_cho(&sock); // bộ đệm GOP đi TRƯỚC mọi gói trực tiếp
            if let Some((goi, addr)) = goi {
                let ra = {
                    let mut t = self.khoa.lock().unwrap();
                    self.xu_ly_goi(&mut t, &goi, addr, bay)
                };
                for dich in ra {
                    let _ = sock.send_to(&goi, dich);
                }
            }
            if bay - moc >= 1.0 {
                moc += 1.0;
                if bay - moc > 1.0 {
                    moc = bay;
                }
                let mut t = self.khoa.lock().unwrap();
                t.kbps_vao_am = t.dem_vao_am as f64 * 8.0 / 1000.0;
                t.kbps_vao_hinh = t.dem_vao_hinh as f64 * 8.0 / 1000.0;
                t.kbps_ra = t.dem_ra as f64 * 8.0 / 1000.0;
                t.dem_vao_am = 0;
                t.dem_vao_hinh = 0;
                t.dem_ra = 0;
                for k in t.ds.values_mut() {
                    k.kbps = k.dem_byte as f64 * 8.0 / 1000.0;
                    k.dem_byte = 0;
                }
            }
        }
    }

    /// Gửi toàn bộ bộ đệm GOP cho người vừa có địa chỉ UDP / vừa bật lại video.
    /// Nghỉ 1 ms mỗi 32 gói để không làm tràn bộ đệm nhận của người xem.
    fn gui_bo_dem_cho(&self, sock: &UdpSocket) {
        let (ds, goi, so_i) = {
            let mut t = self.khoa.lock().unwrap();
            let ds: Vec<(u32, SocketAddr)> = t.ds.values().filter(|k| !k.phat && k.can_bo_dem && k.udp.is_some()).map(|k| (k.ma, k.udp.unwrap())).collect();
            if ds.is_empty() {
                return;
            }
            let goi = if t.bo_dem_bat { t.bo_dem.clone() } else { Vec::new() };
            for (m, _) in &ds {
                t.ds.get_mut(m).unwrap().can_bo_dem = false;
            }
            (ds, goi, t.bo_dem_so_i)
        };
        if goi.is_empty() {
            return;
        }
        for (m, dich) in ds {
            for (n, g) in goi.iter().enumerate() {
                let _ = sock.send_to(g, dich);
                if n % 32 == 31 {
                    thread::sleep(Duration::from_millis(1));
                }
            }
            let mut t = self.khoa.lock().unwrap();
            let b: u64 = goi.iter().map(|g| g.len() as u64).sum();
            let so_p = goi.iter().filter(|g| g[1] == LOAI_P).map(|g| [g[4], g[5], g[6], g[7]]).collect::<HashSet<_>>().len();
            t.goi_ra += goi.len() as u64;
            t.byte_ra += b;
            t.dem_ra += b;
            let Some(k) = t.ds.get_mut(&m) else { continue };
            k.goi_ra += goi.len() as u64;
            k.byte_ra += b;
            k.dem_byte += b;
            let ten = k.ten.clone();
            (self.bao)(&format!("{ten}: gửi bộ đệm GOP {} gói (I #{} + {so_p} P)", goi.len(), so_i.map_or("None".into(), |s| s.to_string())));
            if k.khung_dau_ms.is_none() {
                let ms = (bay_gio() - k.luc_moc) * 1000.0;
                k.khung_dau_ms = Some(ms);
                k.nguon_dau = "bộ đệm";
                (self.bao)(&format!("{ten}: khung I đầu sau {ms:.0} ms (bộ đệm)"));
            }
        }
    }

    /// Xử lý một datagram (đang giữ khóa). Trả về danh sách đích cần gửi nguyên gói này.
    fn xu_ly_goi(&self, t: &mut TT, goi: &[u8], addr: SocketAddr, bay: f64) -> Vec<SocketAddr> {
        let b0 = goi[0];
        let mut loai_so = None;
        if b0 == PHIEN_BAN {
            let Some(h) = tach_header(goi) else { return Vec::new() };
            if h.loai == LOAI_GIU {
                let bat = t.bo_dem_bat;
                if let Some(k) = t.ds.get_mut(&(h.ma as u32)) {
                    if addr.ip() == k.ip && k.udp != Some(addr) {
                        let lan_dau = k.udp.is_none();
                        k.udp = Some(addr);
                        (self.bao)(&format!("#{} {} đăng ký UDP {addr}", k.ma, k.ten));
                        if lan_dau && !k.phat && k.video && bat {
                            k.can_bo_dem = true;
                        }
                    }
                }
                return Vec::new();
            }
            loai_so = Some((h.loai, h.so));
        } else if b0 != DAU_RTP {
            return Vec::new();
        }
        let tu_phat = t.may_phat.and_then(|m| t.ds.get(&m)).is_some_and(|p| p.udp == Some(addr));
        if !tu_phat || !t.dang_phat {
            t.sai_nguon += 1; // chỉ nhận phương tiện từ máy phát
            return Vec::new();
        }
        t.goi_vao += 1;
        t.byte_vao += goi.len() as u64;
        let video = loai_so.is_some();
        if let Some((loai, so)) = loai_so {
            t.dem_vao_hinh += goi.len() as u64;
            self.cap_nhat_bo_dem(t, loai, so, goi);
            if t.xem_truoc {
                Self::giai_ma_xem_truoc(t, goi, bay);
            }
        } else {
            t.dem_vao_am += goi.len() as u64;
        }
        let mut ra = Vec::new();
        for k in t.ds.values_mut().filter(|k| !k.phat) {
            let Some(dich) = k.udp else { continue };
            if k.can_bo_dem || !(if video { k.video } else { k.am_thanh }) {
                continue;
            }
            ra.push(dich);
            k.goi_ra += 1;
            k.byte_ra += goi.len() as u64;
            k.dem_byte += goi.len() as u64;
            if video && loai_so.unwrap().0 == LOAI_I && k.khung_dau_ms.is_none() {
                let ms = (bay - k.luc_moc) * 1000.0;
                k.khung_dau_ms = Some(ms);
                k.nguon_dau = "trực tiếp";
                (self.bao)(&format!("{}: khung I đầu sau {ms:.0} ms (trực tiếp)", k.ten));
            }
        }
        t.goi_ra += ra.len() as u64;
        t.byte_ra += (goi.len() * ra.len()) as u64;
        t.dem_ra += (goi.len() * ra.len()) as u64;
        ra
    }

    /// Bộ đệm GOP: mảnh đầu tiên của khung I mới -> xóa; lưu mảnh của I và các P sau nó.
    fn cap_nhat_bo_dem(&self, t: &mut TT, loai: u8, so: u32, goi: &[u8]) {
        if !t.bo_dem_bat {
            return;
        }
        if loai == LOAI_I && Some(so) != t.bo_dem_so_i {
            t.bo_dem.clear();
            t.bo_dem_byte = 0;
            t.bo_dem_so_i = Some(so);
        } else if loai != LOAI_I && t.bo_dem_so_i.is_none() {
            return; // chưa có I (hoặc vừa tràn): chờ I kế tiếp
        }
        t.bo_dem.push(Arc::new(goi.to_vec()));
        t.bo_dem_byte += goi.len();
        if t.bo_dem_byte > BO_DEM_GOP_TOI_DA {
            t.bo_dem.clear();
            t.bo_dem_byte = 0;
            t.bo_dem_so_i = None;
            (self.bao)("Bộ đệm GOP quá 4 MB: xóa, chờ khung I kế tiếp");
        }
    }

    /// Server giải mã MỘT bản chỉ để hiện ảnh xem trước (không ảnh hưởng việc chuyển tiếp).
    fn giai_ma_xem_truoc(t: &mut TT, goi: &[u8], bay: f64) {
        let Some(m) = tach_header(goi) else { return };
        let Some(k) = t.ghep.them(&m, bay) else { return };
        t.giai_ma.kiem_tra(bay);
        for kh in t.giai_ma.them(k, bay) {
            t.anh_xem_truoc = Some((kh.w as usize, kh.h as usize, kh.anh, kh.ts, kh.so));
        }
    }

    // ------------------------------------------------------------ lệnh của bảng điều khiển
    pub fn dat_bo_dem(&self, bat: bool) {
        {
            let mut t = self.khoa.lock().unwrap();
            t.bo_dem_bat = bat;
            if !bat {
                t.bo_dem.clear();
                t.bo_dem_byte = 0;
                t.bo_dem_so_i = None;
            }
        }
        (self.bao)(&format!("Bộ đệm GOP: {}", if bat { "bật" } else { "tắt" }));
    }

    pub fn dat_xem_truoc(&self, bat: bool) {
        self.khoa.lock().unwrap().xem_truoc = bat;
    }

    /// Đuổi người xem theo tên (hoặc mã). Trả về số người bị ngắt.
    pub fn ngat(&self, ten: Option<&str>, ma: Option<u32>) -> usize {
        let mut conns = Vec::new();
        {
            let mut t = self.khoa.lock().unwrap();
            let ds: Vec<u32> = t.ds.values().filter(|k| !k.phat && (ma == Some(k.ma) || Some(k.ten.as_str()) == ten)).map(|k| k.ma).collect();
            for m in ds {
                let k = &t.ds[&m];
                k.gui(&json!({"loai": "bi_ngat", "ly_do": "server ngắt"}));
                if let Ok(c) = k.conn.try_clone() {
                    conns.push(c);
                }
                self.roi(&mut t, m, "bị server ngắt");
            }
        }
        for c in &conns {
            let _ = c.shutdown(Shutdown::Both);
        }
        conns.len()
    }

    /// Ảnh chụp trạng thái cho dòng lệnh và giao diện.
    pub fn trang_thai(&self) -> TrangThai {
        let t = self.khoa.lock().unwrap();
        let vao = t.kbps_vao_am + t.kbps_vao_hinh;
        TrangThai {
            dang_phat: t.dang_phat,
            ten_phat: t.may_phat.and_then(|m| t.ds.get(&m)).map(|k| k.ten.clone()),
            bo_dem_bat: t.bo_dem_bat,
            bo_dem_goi: t.bo_dem.len(),
            bo_dem_byte: t.bo_dem_byte,
            bo_dem_so_i: t.bo_dem_so_i,
            kbps_vao: vao,
            kbps_vao_am: t.kbps_vao_am,
            kbps_vao_hinh: t.kbps_vao_hinh,
            kbps_ra: t.kbps_ra,
            he_so: if vao > 0.0 { t.kbps_ra / vao } else { 0.0 },
            byte_vao: t.byte_vao,
            byte_ra: t.byte_ra,
            goi_vao: t.goi_vao,
            goi_ra: t.goi_ra,
            pli_chuyen: t.pli_chuyen,
            pli_bo: t.pli_bo,
            nguoi_xem: t.ds_nguoi_xem().iter().map(|m| t.ds[m].tom_tat()).collect(),
            da_roi: t.da_roi.clone(),
            anh: t.anh_xem_truoc.clone(),
        }
    }
}

fn m_cua(ma: Option<u32>) -> u32 {
    ma.unwrap_or(0)
}

/// Một dòng của bảng người xem (≤ 55 ký tự) – dùng chung cho trang_thai và tổng kết.
pub fn dong_bang(x: &TomTat, kbps: f64) -> String {
    let dau = match x.khung_dau_ms {
        None => "–".to_string(),
        Some(ms) => format!("{ms:4.0} ms {}", x.nguon_dau),
    };
    let ten: String = x.ten.chars().take(6).collect();
    format!("{ten:<6} {:<4} {:6} {:6.0} {kbps:6.0} {dau}", x.nhan, x.goi_ra, x.byte_ra as f64 / 1024.0)
}

pub fn tieu_de_bang() -> String {
    format!("{:<6} {:<4} {:>6} {:>6} {:>6} khung I đầu", "tên", "nhận", "gói ra", "KB ra", "kbit/s")
}

/// Các dòng in cho lệnh trang_thai.
pub fn bang_trang_thai(tt: &TrangThai) -> Vec<String> {
    let phat = if tt.dang_phat { format!("{} đang phát", tt.ten_phat.clone().unwrap_or_default()) } else { "chưa phát".into() };
    let mut ds = vec![
        format!("Trạng thái: {phat}, {} người xem", tt.nguoi_xem.len()),
        format!("bộ đệm GOP {}: {} gói, {:.0} KB", if tt.bo_dem_bat { "bật" } else { "tắt" }, tt.bo_dem_goi, tt.bo_dem_byte as f64 / 1024.0),
        format!("vào {:.0} kbit/s (tiếng {:.0}, hình {:.0})", tt.kbps_vao, tt.kbps_vao_am, tt.kbps_vao_hinh),
        format!("ra {:.0} kbit/s, hệ số nhân bản {:.2}", tt.kbps_ra, tt.he_so),
        format!("PLI chuyển {}, bỏ {}", tt.pli_chuyen, tt.pli_bo),
        tieu_de_bang(),
    ];
    ds.extend(tt.nguoi_xem.iter().map(|x| dong_bang(x, x.kbps)));
    ds
}

/// Các dòng in khi server kết thúc: mọi người xem đã vào (cả người đã rời).
pub fn bang_tong_ket(tt: &TrangThai) -> Vec<String> {
    let ds: Vec<&TomTat> = tt.da_roi.iter().chain(tt.nguoi_xem.iter()).collect();
    let he_so = if tt.byte_vao > 0 { tt.byte_ra as f64 / tt.byte_vao as f64 } else { 0.0 };
    let mut v = vec![
        format!("===== TỔNG KẾT: {} người xem =====", ds.len()),
        format!("vào {} gói {:.0} KB, ra {} gói {:.0} KB", tt.goi_vao, tt.byte_vao as f64 / 1024.0, tt.goi_ra, tt.byte_ra as f64 / 1024.0),
        format!("hệ số nhân bản (byte ra/vào) {he_so:.2}"),
        format!("PLI chuyển {}, bỏ {}", tt.pli_chuyen, tt.pli_bo),
        tieu_de_bang(),
    ];
    v.extend(ds.iter().map(|x| dong_bang(x, x.byte_ra as f64 * 8.0 / 1000.0 / x.giay.max(0.001))));
    v.push("(kbit/s = trung bình suốt thời gian xem)".into());
    v
}
