
//!
//! Tệp này GIỐNG NHAU ở cả 9 bài (chép nguyên, chỉ sửa dòng đầu); nghiệp vụ riêng của từng bài nằm ở
//! may_chu.rs (kiểu cài đặt `NghiepVu` và `LuatKiemToan`) và nguoi_ao.rs (người dùng ảo của bộ mô phỏng).
//! Gồm:
//!   1. Giao thức: gui_json, doc_dong (mỗi dòng một JSON UTF-8 kết thúc "\n", tối đa 64 KB), ten_hop_le,
//!      doc_lenh_stdin ("@t lệnh"), doc_tham_so ("khoa=gia_tri"), Args (tham số dòng lệnh).
//!   2. Đo đạc: phan_vi (theo hạng gần nhất), ThongKe (đếm yêu cầu, độ trễ xử lý µs, yc/s).
//!   3. KhoTranhChap: kho các "ô" tài nguyên và BỐN cách xử lý tranh chấp khi n luồng cùng ghi:
//!         khong_khoa  đọc – xử lý – ghi không đồng bộ        -> tranh chấp (race condition), mất cập nhật
//!         khoa        khóa loại trừ theo từng ô, giữ suốt đọc – xử lý – ghi
//!         lac_quan    đọc (giá trị, phiên bản) -> xử lý ngoài khóa -> so sánh-và-đổi (CAS) dưới khóa ngắn
//!                     -> phiên bản đã đổi thì thử lại (tối đa 5 lần, nghỉ ngẫu nhiên 0–1 ms)
//!         hang_doi    mọi yêu cầu ghi xếp hàng (kênh mpsc) cho MỘT luồng xử lý (mô hình actor)
//!   4. SoKiemToan: sổ ghi mọi giao dịch được chấp nhận (khóa riêng) và đếm sai phạm theo luật của bài.
//!   5. MayChu<N>: máy chủ TCP đa luồng (một luồng mỗi kết nối): chào, quản trị, ma_yc lặp lại, đẩy
//!      cap_nhat gộp tối đa 10 lần/giây; bài cụ thể chỉ cài đặt trait NghiepVu.
//!   6. Định dạng in: tien(), dong_thong_ke(), bang_so_sanh() – in GIỐNG HỆT bản Python.
//!
//! Giao thức (TCP, mỗi dòng một JSON) – giống hệt bản Python:
//!   client -> server  {"loai":"chao","ten":"An","vai_tro":"nguoi_dung"|"quan_tri"[,"so_nguoi":m]}  (đầu tiên)
//!                     {"loai":"<thao_tac>","ma_yc":k,["nguoi":"u17"],...}   (yêu cầu nghiệp vụ)
//!                     {"loai":"che_do","ma_yc":k,"gia_tri":"khoa"} | {"loai":"tre","ma_yc":k,"ms":2}
//!                     {"loai":"dat_lai","ma_yc":k,...tham số} | {"loai":"thong_ke","ma_yc":k}  (chỉ quan_tri)
//!   server -> client  {"loai":"chao","ma":id,"che_do":"khoa","may_chu":"..."} rồi ngay một cap_nhat
//!                     {"loai":"kq","ma_yc":k,"ok":true|false,["ma_loi":"..","ly_do":".."],...}
//!                     {"loai":"cap_nhat","che_do":"khoa",...trạng thái nghiệp vụ}  (gộp ≤ 10 lần/s)
//!                     {"loai":"su_kien","noi_dung":"..."} ; {"loai":"thong_ke","ma_yc":k,...}
//!                     {"loai":"loi","ly_do":"..."} (sai giao thức)
//! "so_nguoi" = m: kết nối gộp của bộ mô phỏng chở m người dùng ảo; mỗi yêu cầu ghi tên người ở "nguoi".
//! Một kết nối được gửi nhiều yêu cầu liên tiếp không chờ trả lời (pipelining) – trả lời ghép theo ma_yc.
//! Gửi lại một ma_yc đã xử lý: máy chủ KHÔNG làm lại mà gửi lại kết quả cũ kèm "lap_lai": true.
//!
//! An toàn bộ nhớ khi cố tình "không đồng bộ": giá trị mỗi ô nằm trong Mutex<Arc<V>> chỉ giữ trong lúc
//! chép/thay con trỏ (rất ngắn), phiên bản là AtomicU64. Từng lần đọc, từng lần ghi đều trọn vẹn (không
//! có data race, không UB) nhưng chuỗi "đọc – nghỉ – xử lý – ghi" thì không, nên chế độ khong_khoa vẫn
//! lộ đúng lỗi mất cập nhật mà bài muốn minh họa.

#![allow(dead_code)] // khung chung của 9 bài: mỗi bài không dùng hết

use serde_json::{json, Map, Value};
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, LazyLock, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub type Obj = Map<String, Value>;
/// Hàm báo một dòng nhật ký (dòng lệnh: in "[server] …"; giao diện: thêm vào ô nhật ký).
pub type Bao = Arc<dyn Fn(&str) + Send + Sync>;

pub const GIOI_HAN_DONG: usize = 64 * 1024; // một dòng JSON tối đa 64 KB
pub const CHE_DO: [&str; 4] = ["khong_khoa", "khoa", "lac_quan", "hang_doi"];
pub const TEN_CHE_DO: [&str; 4] = ["Không khóa (lỗi tranh chấp)", "Khóa loại trừ (mutex)", "Lạc quan (phiên bản + CAS)", "Hàng đợi một luồng (actor)"];
pub const VAI_TRO: [&str; 2] = ["nguoi_dung", "quan_tri"];
pub const SO_LAN_THU: usize = 5; // lac_quan: số lần thử tối đa
pub const CHO_KHOA_TOI_DA_S: f64 = 2.0; // khoa: chờ một khóa quá 2 s thì bỏ (nghi khóa chết – deadlock)
pub const NHO_KQ: usize = 1024; // mỗi kết nối nhớ kết quả của 1024 ma_yc gần nhất
pub const GIU_DO_TRE: usize = 200_000; // giữ tối đa 200 000 mẫu độ trễ cho phân vị
pub const NHIP_DAY_S: f64 = 0.1; // đẩy cap_nhat tối đa 10 lần/giây

pub fn ten_che_do(cd: &str) -> &'static str {
    CHE_DO.iter().position(|x| *x == cd).map_or("", |i| TEN_CHE_DO[i])
}

// ================================================================ 0. Thời gian, ngẫu nhiên, in
/// Đồng hồ đơn điệu chung (giây) – thay cho time.monotonic() của Python.
pub static T0: LazyLock<Instant> = LazyLock::new(Instant::now);
pub fn bay_gio() -> f64 {
    T0.elapsed().as_secs_f64()
}

/// Ngủ `giay` giây, chính xác cả với độ trễ dưới 1 ms. Windows: đồng hồ hệ thống mặc định ~15,6 ms làm
/// `thread::sleep(1 ms)` ngủ tới 15 ms (độ trễ giả lập 0–1 ms thành 0–15 ms); lần gọi đầu xin độ phân giải 1 ms bằng
/// `timeBeginPeriod(1)` (winmm) như bản C#, và phần cuối (dưới 2 ms) chờ bận bằng `Instant` + `yield_now` như bản Java.
pub fn ngu(giay: f64) {
    #[cfg(windows)]
    {
        #[link(name = "winmm")]
        extern "system" {
            fn timeBeginPeriod(u: u32) -> u32;
        }
        static MOT_LAN: std::sync::Once = std::sync::Once::new();
        MOT_LAN.call_once(|| unsafe {
            timeBeginPeriod(1);
        });
    }
    if giay <= 0.0 {
        return;
    }
    let den = Instant::now() + Duration::from_secs_f64(giay);
    if cfg!(windows) {
        let con = den.saturating_duration_since(Instant::now());
        if con > Duration::from_millis(3) {
            thread::sleep(con - Duration::from_millis(2));
        }
        while Instant::now() < den {
            thread::yield_now();
        }
    } else {
        thread::sleep(Duration::from_secs_f64(giay));
    }
}

/// Giờ địa phương HH:MM:SS (như time.strftime("%H:%M:%S") của Python).
pub fn gio_hien() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

thread_local! {
    /// Bộ sinh số giả ngẫu nhiên xorshift64* riêng mỗi luồng (không cần khóa).
    static NN: Cell<u64> = Cell::new(hat_giong());
}

fn hat_giong() -> u64 {
    static DEM: AtomicU64 = AtomicU64::new(0);
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    let mut z = t ^ DEM.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed); // splitmix64
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) | 1
}

/// Số ngẫu nhiên trong [0, 1).
pub fn ngau_nhien() -> f64 {
    NN.with(|c| {
        let mut x = c.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        c.set(x);
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    })
}

/// random.uniform(a, b)
pub fn ngau_nhien_khoang(a: f64, b: f64) -> f64 {
    a + (b - a) * ngau_nhien()
}

/// random.randint(a, b) – gồm cả hai đầu.
pub fn ngau_nhien_nguyen(a: i64, b: i64) -> i64 {
    a + ((ngau_nhien() * (b - a + 1) as f64) as i64).min(b - a)
}

/// random.choice(ds)
pub fn chon_ngau_nhien<T>(ds: &[T]) -> &T {
    &ds[ngau_nhien_nguyen(0, ds.len() as i64 - 1) as usize]
}

static KHOA_IN: Mutex<()> = Mutex::new(());

/// Nhiều luồng cùng in: mỗi dòng in trọn vẹn, có tiền tố "[tien_to] ".
pub fn in_dong(tien_to: &str, text: &str) {
    let _g = KHOA_IN.lock().unwrap_or_else(|e| e.into_inner());
    let mut o = io::stdout().lock();
    let _ = writeln!(o, "[{tien_to}] {text}");
    let _ = o.flush();
}

/// In một dòng không tiền tố (bảng so sánh).
pub fn in_tho(text: &str) {
    let _g = KHOA_IN.lock().unwrap_or_else(|e| e.into_inner());
    let mut o = io::stdout().lock();
    let _ = writeln!(o, "{text}");
    let _ = o.flush();
}

// ================================================================ 1. Giao thức
/// bytes của một dòng – dùng khi gửi nhiều thông điệp trong MỘT lần ghi (pipelining).
pub fn ma_hoa(obj: &Obj) -> Vec<u8> {
    let mut v = serde_json::to_vec(obj).unwrap_or_default();
    v.push(b'\n');
    v
}

/// Mỗi thông điệp là một dòng JSON UTF-8 kết thúc '\n'.
pub fn gui_json(w: &mut impl Write, obj: &Obj) -> io::Result<()> {
    w.write_all(&ma_hoa(obj))
}

fn loi_du_lieu(text: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, text.into())
}

/// Đọc một dòng JSON. Ok(None) khi kết nối đóng; dòng > 64 KB, bị cắt hoặc sai dạng -> lỗi InvalidData.
pub fn doc_dong(r: &mut impl BufRead) -> io::Result<Option<Obj>> {
    let mut dong = Vec::new();
    r.take(GIOI_HAN_DONG as u64 + 1).read_until(b'\n', &mut dong)?;
    if dong.is_empty() {
        return Ok(None);
    }
    if dong.len() > GIOI_HAN_DONG || dong.last() != Some(&b'\n') {
        return Err(loi_du_lieu("dòng quá dài hoặc bị cắt"));
    }
    match serde_json::from_slice::<Value>(&dong) {
        Ok(Value::Object(o)) => Ok(Some(o)),
        Ok(_) => Err(loi_du_lieu("thông điệp không phải đối tượng JSON")),
        Err(e) => Err(loi_du_lieu(e.to_string())),
    }
}

/// Tên 1–16 ký tự in được, không có khoảng trắng ở đầu/cuối (như ten_hop_le của Python).
pub fn ten_hop_le(ten: &str) -> bool {
    let n = ten.chars().count();
    (1..=16).contains(&n) && ten.trim() == ten && ten.chars().all(|c| !c.is_control() && (c == ' ' || !c.is_whitespace()) && !('\u{200b}'..='\u{200f}').contains(&c))
}

/// Số nguyên JSON (không phải số thực, không phải true/false).
pub fn la_so_nguyen(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => n.as_i64(),
        _ => None,
    }
}

pub fn lay_str<'a>(o: &'a Obj, k: &str) -> Option<&'a str> {
    o.get(k).and_then(Value::as_str)
}

/// Số nguyên (như la_so_nguyen) của trường k.
pub fn lay_i64(o: &Obj, k: &str) -> Option<i64> {
    la_so_nguyen(o.get(k))
}

/// Số bất kỳ (nguyên hoặc thực) của trường k.
pub fn lay_f64(o: &Obj, k: &str) -> Option<f64> {
    o.get(k).and_then(Value::as_f64)
}

/// Giá trị trường k có "đúng" theo kiểu Python (bool(x)) không – dùng cho tin.get("ok").
pub fn dung(o: &Obj, k: &str) -> bool {
    match o.get(k) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(m)) => !m.is_empty(),
    }
}

/// Đối tượng JSON từ macro json!({...}) (giữ thứ tự khóa).
pub fn obj(v: Value) -> Obj {
    match v {
        Value::Object(o) => o,
        _ => Obj::new(),
    }
}

/// Đọc lệnh từ stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ t0 (đồng hồ đơn điệu).
pub fn doc_lenh_stdin(t0: f64, mut xu_ly: impl FnMut(&str), ket_thuc: impl FnOnce()) {
    for dong in io::stdin().lock().lines() {
        let Ok(dong) = dong else { break };
        let mut dong = dong.trim().to_string();
        if let Some(sau) = dong.strip_prefix('@') {
            let (moc, con) = sau.split_once(' ').unwrap_or((sau, ""));
            ngu(moc.parse::<f64>().unwrap_or(0.0) - (bay_gio() - t0));
            dong = con.trim().to_string();
        }
        if !dong.is_empty() {
            xu_ly(&dong);
        }
    }
    ket_thuc();
}

/// ["thoi_gian_s=5", "ten_mon=Bình"] -> {"thoi_gian_s": 5, "ten_mon": "Bình"} (số nếu đọc được).
pub fn doc_tham_so(ds: &[String]) -> Result<Obj, String> {
    let mut kq = Obj::new();
    for muc in ds {
        let (k, v) = muc.split_once('=').ok_or_else(|| format!("cần dạng khóa=giá_trị: {muc}"))?;
        let gt = if let Ok(n) = v.parse::<i64>() {
            json!(n)
        } else if let Ok(x) = v.parse::<f64>() {
            json!(x)
        } else {
            json!(v)
        };
        kq.insert(k.to_string(), gt);
    }
    Ok(kq)
}

/// Tham số dòng lệnh: các giá trị vị trí và các cặp "--khóa giá_trị" (hoặc cờ "--khóa").
pub struct Args {
    pub pos: Vec<String>,
    pub opt: HashMap<String, String>,
}

impl Args {
    pub fn parse(args: &[String], flags: &[&str]) -> Args {
        let (mut pos, mut opt) = (Vec::new(), HashMap::new());
        let mut i = 0;
        while i < args.len() {
            if let Some(k) = args[i].strip_prefix("--") {
                if flags.contains(&k) {
                    opt.insert(k.to_string(), "1".to_string());
                    i += 1;
                } else {
                    opt.insert(k.to_string(), args.get(i + 1).cloned().unwrap_or_default());
                    i += 2;
                }
            } else {
                pos.push(args[i].clone());
                i += 1;
            }
        }
        Args { pos, opt }
    }
    pub fn f64(&self, k: &str, d: f64) -> f64 {
        self.opt.get(k).and_then(|v| v.parse().ok()).unwrap_or(d)
    }
    pub fn i64(&self, k: &str, d: i64) -> i64 {
        self.opt.get(k).and_then(|v| v.parse().ok()).unwrap_or(d)
    }
    pub fn str(&self, k: &str, d: &str) -> String {
        self.opt.get(k).cloned().unwrap_or_else(|| d.to_string())
    }
    pub fn co(&self, k: &str) -> bool {
        self.opt.contains_key(k)
    }
    /// host (vị trí 0, mặc định 127.0.0.1) và cổng (vị trí 1).
    pub fn host_cong(&self, cong: u16) -> (String, u16) {
        let host = self.pos.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
        (host, self.pos.get(1).and_then(|p| p.parse().ok()).unwrap_or(cong))
    }
}

// ================================================================ 2. Đo đạc
/// Phân vị p (0–100) theo HẠNG GẦN NHẤT: phần tử thứ ⌈p/100 · n⌉ (đếm từ 1) của dãy đã sắp xếp.
/// Ví dụ n = 1000: p99 là phần tử thứ 990. Dãy rỗng -> None.
pub fn phan_vi(ds_da_sap_xep: &[f64], p: f64) -> Option<f64> {
    if ds_da_sap_xep.is_empty() {
        return None;
    }
    let i = ((p / 100.0 * ds_da_sap_xep.len() as f64).ceil() as usize).max(1);
    Some(ds_da_sap_xep[i - 1])
}

/// round() của Python: làm tròn nửa về số chẵn.
pub fn round_py(x: f64) -> i64 {
    let t = x.trunc();
    if (x - t).abs() == 0.5 {
        if (t as i64) % 2 == 0 {
            t as i64
        } else {
            (t + x.signum()) as i64
        }
    } else {
        x.round() as i64
    }
}

pub const DEM: [&str; 8] = ["so_yc", "thanh_cong", "tu_choi", "xung_dot", "thu_lai", "het_lan_thu", "khoa_cho_lau", "hang_doi_max"];

fn chi_so_dem(ten: &str) -> usize {
    DEM.iter().position(|x| *x == ten).expect("tên bộ đếm không có trong DEM")
}

struct TkTrong {
    dem: [u64; 8],
    ma_loi: Vec<(String, u64)>, // lý do từ chối -> số lần (theo thứ tự gặp)
    tre_us: Vec<f64>,           // độ trễ xử lý từng yêu cầu (µs)
    t_dau: Option<f64>,
    t_cuoi: Option<f64>,
    giay: i64,
    dem_giay: u64,
    yc_s: u64,
}

impl TkTrong {
    fn moi() -> TkTrong {
        TkTrong { dem: [0; 8], ma_loi: Vec::new(), tre_us: Vec::new(), t_dau: None, t_cuoi: None, giay: bay_gio() as i64, dem_giay: 0, yc_s: 0 }
    }
    /// yc_s = số yêu cầu xong trong giây (đồng hồ) liền trước.
    fn sang_giay(&mut self, bay: f64) {
        let g = bay as i64;
        if g != self.giay {
            self.yc_s = if g == self.giay + 1 { self.dem_giay } else { 0 };
            self.giay = g;
            self.dem_giay = 0;
        }
    }
}

/// Số liệu của máy chủ từ lần đặt lại gần nhất. Mọi hàm an toàn đa luồng (một khóa nhỏ riêng).
pub struct ThongKe {
    k: Mutex<TkTrong>,
}

impl Default for ThongKe {
    fn default() -> Self {
        ThongKe { k: Mutex::new(TkTrong::moi()) }
    }
}

impl ThongKe {
    pub fn dat_lai(&self) {
        *self.k.lock().unwrap() = TkTrong::moi();
    }
    pub fn cong(&self, ten: &str, n: u64) {
        self.k.lock().unwrap().dem[chi_so_dem(ten)] += n;
    }
    pub fn dat_max(&self, ten: &str, x: u64) {
        let mut k = self.k.lock().unwrap();
        let i = chi_so_dem(ten);
        if x > k.dem[i] {
            k.dem[i] = x;
        }
    }
    pub fn bat_dau_yc(&self) {
        let mut k = self.k.lock().unwrap();
        k.dem[0] += 1;
        if k.t_dau.is_none() {
            k.t_dau = Some(bay_gio());
        }
    }
    pub fn xong_yc(&self, kq: &Obj, tre_us: f64) {
        let bay = bay_gio();
        let mut k = self.k.lock().unwrap();
        if dung(kq, "ok") {
            k.dem[1] += 1;
        } else {
            k.dem[2] += 1;
            let ma = lay_str(kq, "ma_loi").unwrap_or("khac").to_string();
            match k.ma_loi.iter_mut().find(|(m, _)| *m == ma) {
                Some(x) => x.1 += 1,
                None => k.ma_loi.push((ma, 1)),
            }
        }
        if k.tre_us.len() < GIU_DO_TRE {
            k.tre_us.push(tre_us);
        }
        k.t_cuoi = Some(bay);
        k.sang_giay(bay);
        k.dem_giay += 1;
    }
    pub fn anh_chup(&self) -> Obj {
        let mut d = Obj::new();
        let mut tre;
        {
            let mut k = self.k.lock().unwrap();
            k.sang_giay(bay_gio());
            for (i, ten) in DEM.iter().enumerate() {
                d.insert(ten.to_string(), json!(k.dem[i]));
            }
            tre = k.tre_us.clone();
            d.insert("ma_loi".into(), Value::Object(k.ma_loi.iter().map(|(m, s)| (m.clone(), json!(s))).collect()));
            d.insert("yc_s".into(), json!(k.yc_s));
            let xong = k.dem[1] + k.dem[2];
            let khoang = match (k.t_dau, k.t_cuoi) {
                (Some(a), Some(b)) => b - a,
                _ => 0.0,
            };
            d.insert("thong_luong".into(), json!(if khoang > 0.0 { round_py(xong as f64 / khoang) } else { 0 }));
        }
        tre.sort_by(f64::total_cmp);
        for p in [50, 95, 99] {
            d.insert(format!("p{p}_us"), phan_vi(&tre, p as f64).map_or(Value::Null, |v| json!(round_py(v))));
        }
        d
    }
}

// ================================================================ 3. Kho tài nguyên + 4 chế độ
/// Khóa của một ô (cho chế độ khoa): có HẠN CHỜ (std::sync::Mutex không có "lock có thời hạn") và
/// CÔNG BẰNG – ai chờ trước được trước (FIFO): khi trả khóa, khóa được trao thẳng cho người đứng đầu
/// hàng chờ, nên không ai bị "chen ngang" mãi tới quá hạn 2 s. Dựng từ Mutex + Condvar.
#[derive(Default)]
struct KhoaO {
    k: Mutex<KhoaTrong>,
}

#[derive(Default)]
struct KhoaTrong {
    bi_giu: bool,
    hang: VecDeque<Arc<NguoiCho>>,
}

#[derive(Default)]
struct NguoiCho {
    duoc: Mutex<bool>, // đã được trao khóa
    cv: Condvar,
}

impl KhoaO {
    fn lay(&self, han: Duration) -> bool {
        let cho = {
            let mut k = self.k.lock().unwrap();
            if !k.bi_giu && k.hang.is_empty() {
                k.bi_giu = true;
                return true;
            }
            let c = Arc::new(NguoiCho::default());
            k.hang.push_back(Arc::clone(&c));
            c
        };
        let het = Instant::now() + han;
        let mut d = cho.duoc.lock().unwrap();
        while !*d {
            let con = het.saturating_duration_since(Instant::now());
            if con.is_zero() {
                drop(d);
                let mut k = self.k.lock().unwrap(); // việc trao khóa diễn ra dưới khóa này: kiểm lại
                if *cho.duoc.lock().unwrap() {
                    return true;
                }
                k.hang.retain(|x| !Arc::ptr_eq(x, &cho));
                return false;
            }
            d = cho.cv.wait_timeout(d, con).unwrap().0;
        }
        true
    }

    fn tra(&self) {
        let mut k = self.k.lock().unwrap();
        match k.hang.pop_front() {
            Some(c) => {
                *c.duoc.lock().unwrap() = true; // trao thẳng: khóa vẫn "bị giữ", chủ mới là người đứng đầu hàng
                c.cv.notify_one();
            }
            None => k.bi_giu = false,
        }
    }
}

/// Một ô tài nguyên (món hàng, ghế, tài khoản…): giá trị, phiên bản (cho lac_quan), khóa riêng (cho khoa).
pub struct O<V> {
    gia_tri: Mutex<Arc<V>>,
    phien_ban: AtomicU64,
    khoa: KhoaO,
}

impl<V> O<V> {
    fn moi(v: V) -> O<V> {
        O { gia_tri: Mutex::new(Arc::new(v)), phien_ban: AtomicU64::new(0), khoa: KhoaO::default() }
    }
    fn doc(&self) -> Arc<V> {
        Arc::clone(&self.gia_tri.lock().unwrap())
    }
}

/// Giá trị đọc được của các ô (giữ thứ tự khóa), như dict `doc` của bản Python.
pub struct Doc<V> {
    pub ds: Vec<(String, Arc<V>)>,
}

impl<V> Doc<V> {
    pub fn get(&self, k: &str) -> Option<&V> {
        self.ds.iter().find(|(x, _)| x == k).map(|(_, v)| &**v)
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.ds.iter().map(|(_, v)| &**v)
    }
}

impl<V> std::ops::Index<&str> for Doc<V> {
    type Output = V;
    fn index(&self, k: &str) -> &V {
        self.get(k).unwrap_or_else(|| panic!("không có ô {k}"))
    }
}

/// Các ô cần ghi: (khóa ô, GIÁ TRỊ MỚI).
pub type Ghi<V> = Vec<(String, V)>;
/// Hàm THUẦN của nghiệp vụ: đọc doc, trả về (kq, ghi). Không sửa doc (giá trị chỉ đọc qua Arc).
pub type XuLy<V> = Box<dyn Fn(&Doc<V>) -> (Obj, Option<Ghi<V>>) + Send>;
pub type TraLoi = Box<dyn FnOnce(Obj) + Send>;
pub type KhiGhi = Box<dyn FnOnce(&mut Obj) + Send>;

struct Viec<V> {
    o: Vec<Arc<O<V>>>,
    cac_khoa: Vec<String>,
    xu_ly: XuLy<V>,
    khi_ghi: Option<KhiGhi>,
    tra_loi: TraLoi,
}

struct Cong {
    dong: bool,
    dang_chay: usize,
}

struct BangO<V> {
    ds: Vec<(String, Arc<O<V>>)>,
    chi_muc: HashMap<String, usize>,
}

/// Kho các ô tài nguyên; mọi thao tác GHI đi qua thuc_hien() và được xử lý theo chế độ hiện tại.
///
/// Nghiệp vụ viết hàm THUẦN  xu_ly(doc) -> (kq, ghi):
///   - doc: giá trị đọc được của các ô trong cac_khoa;
///   - kq:  đối tượng trả lời client ({"ok": true/false, "ma_loi", "ly_do", ...});
///   - ghi: None (không ghi) hoặc [(khóa ô, GIÁ TRỊ MỚI)] – chỉ các ô trong cac_khoa. lac_quan có thể
///          gọi xu_ly nhiều lần.
/// khi_ghi(kq) (tùy chọn) được gọi NGAY SAU khi ghi, vẫn trong vùng găng của chế độ (dưới khóa ô với
/// khoa, dưới khóa CAS với lac_quan, trong luồng xử lý với hang_doi): nơi ghi sổ kiểm toán đúng thứ tự.
/// tra_loi(kq) được gọi đúng một lần khi xong (luồng gọi, hoặc luồng xử lý của hang_doi).
///
/// "Cổng" (tam_dung): đổi chế độ / đặt lại chỉ diễn ra khi không còn thao tác nào đang chạy hay đang
/// xếp hàng – không bao giờ có hai chế độ chạy lẫn vào nhau.
pub struct KhoTranhChap<V> {
    thong_ke: Arc<ThongKe>,
    che_do: AtomicUsize,
    tre_ms: Mutex<f64>,
    pub khoa_ngay_tho: AtomicBool, // khoa: true = khóa theo thứ tự cac_khoa (có thể khóa chết)
    o: RwLock<BangO<V>>,
    khoa_cas: Mutex<()>, // lac_quan: khóa rất ngắn quanh đọc ảnh chụp và so sánh-và-đổi
    cong: Mutex<Cong>,
    cong_cv: Condvar,
    hang: Mutex<Option<mpsc::Sender<Viec<V>>>>,
    do_dai: AtomicUsize,
}

fn loi_panic(e: Box<dyn std::any::Any + Send>) -> Obj {
    let text = e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned()).unwrap_or_else(|| "?".into());
    obj(json!({"ok": false, "ma_loi": "loi", "ly_do": format!("lỗi xử lý: {text}")}))
}

impl<V: Send + Sync + 'static> KhoTranhChap<V> {
    pub fn new(thong_ke: Arc<ThongKe>, che_do: &str, tre_ms: f64) -> Result<Arc<Self>, String> {
        let cd = CHE_DO.iter().position(|x| *x == che_do).ok_or_else(|| format!("chế độ không hợp lệ: {che_do}"))?;
        let (tx, rx) = mpsc::channel::<Viec<V>>();
        let kho = Arc::new(KhoTranhChap {
            thong_ke,
            che_do: AtomicUsize::new(cd),
            tre_ms: Mutex::new(tre_ms),
            khoa_ngay_tho: AtomicBool::new(false),
            o: RwLock::new(BangO { ds: Vec::new(), chi_muc: HashMap::new() }),
            khoa_cas: Mutex::new(()),
            cong: Mutex::new(Cong { dong: false, dang_chay: 0 }),
            cong_cv: Condvar::new(),
            hang: Mutex::new(Some(tx)),
            do_dai: AtomicUsize::new(0),
        });
        let yeu = Arc::downgrade(&kho);
        thread::spawn(move || Self::vong_hang_doi(yeu, rx));
        Ok(kho)
    }

    // ------------------------------------------------------------ ô tài nguyên
    /// Thay toàn bộ các ô (đặt lại) – gọi trong tam_dung.
    pub fn thay_o(&self, ds: Vec<(String, V)>) {
        let mut b = self.o.write().unwrap();
        b.chi_muc = ds.iter().enumerate().map(|(i, (k, _))| (k.clone(), i)).collect();
        b.ds = ds.into_iter().map(|(k, v)| (k, Arc::new(O::moi(v)))).collect();
    }

    /// Thêm (hoặc thay) MỘT ô khi đang chạy – ví dụ ô của một người lần đầu xuất hiện (MR6.9, MR6.10).
    /// Chỉ đẩy ô mới vào cuối bảng và chỉ mục (O(1) khấu hao) dưới khóa ghi rất ngắn – KHÔNG chép lại cả
    /// bảng: với 100 000 người, chép cả bảng mỗi lần thêm một ô sẽ thành O(n²).
    pub fn dat_o(&self, khoa: &str, v: V) {
        let o = Arc::new(O::moi(v));
        let mut b = self.o.write().unwrap();
        match b.chi_muc.get(khoa) {
            Some(&i) => b.ds[i].1 = o,
            None => {
                let i = b.ds.len();
                b.chi_muc.insert(khoa.to_string(), i);
                b.ds.push((khoa.to_string(), o));
            }
        }
    }

    /// Có ô `khoa` chưa.
    pub fn co_o(&self, khoa: &str) -> bool {
        self.o.read().unwrap().chi_muc.contains_key(khoa)
    }

    /// Đọc không khóa ô (để hiển thị): giá trị được thay nguyên Arc nên luôn là một bản trọn vẹn.
    pub fn doc(&self, khoa: &str) -> Arc<V> {
        let b = self.o.read().unwrap();
        let i = *b.chi_muc.get(khoa).unwrap_or_else(|| panic!("không có ô {khoa}"));
        b.ds[i].1.doc()
    }

    pub fn anh_chup(&self) -> Doc<V> {
        let b = self.o.read().unwrap();
        Doc { ds: b.ds.iter().map(|(k, o)| (k.clone(), o.doc())).collect() }
    }

    pub fn do_dai_hang(&self) -> usize {
        self.do_dai.load(Ordering::SeqCst)
    }

    pub fn che_do(&self) -> &'static str {
        CHE_DO[self.che_do.load(Ordering::SeqCst)]
    }

    pub fn tre_ms(&self) -> f64 {
        *self.tre_ms.lock().unwrap()
    }

    // ------------------------------------------------------------ cấu hình
    /// Đóng cổng, chờ mọi thao tác đang chạy/đang xếp hàng xong, chạy f, rồi mở lại cổng.
    pub fn tam_dung<R>(&self, f: impl FnOnce() -> R) -> R {
        {
            let mut c = self.cong.lock().unwrap();
            while c.dong {
                c = self.cong_cv.wait(c).unwrap();
            }
            c.dong = true;
            while c.dang_chay > 0 {
                c = self.cong_cv.wait(c).unwrap();
            }
        }
        struct MoCong<'a>(&'a Mutex<Cong>, &'a Condvar);
        impl Drop for MoCong<'_> {
            fn drop(&mut self) {
                self.0.lock().unwrap_or_else(|e| e.into_inner()).dong = false;
                self.1.notify_all();
            }
        }
        let _mo = MoCong(&self.cong, &self.cong_cv);
        f()
    }

    pub fn dat_che_do(&self, che_do: &str) -> Result<(), String> {
        let cd = CHE_DO.iter().position(|x| *x == che_do).ok_or_else(|| format!("chế độ không hợp lệ: {che_do}"))?;
        self.tam_dung(|| self.che_do.store(cd, Ordering::SeqCst));
        Ok(())
    }

    pub fn dat_tre(&self, ms: f64) -> Result<(), String> {
        if !(0.0..=100.0).contains(&ms) {
            return Err("độ trễ 0–100 ms".into());
        }
        *self.tre_ms.lock().unwrap() = ms;
        Ok(())
    }

    /// Giả lập thời gian xử lý (đọc cơ sở dữ liệu, kiểm tra nghiệp vụ): ngẫu nhiên 0 … tre_ms.
    fn tre(&self) {
        let ms = self.tre_ms();
        if ms > 0.0 {
            ngu(ngau_nhien_khoang(0.0, ms) / 1000.0);
        }
    }

    // ------------------------------------------------------------ thực hiện một thao tác
    pub fn thuc_hien(&self, cac_khoa: Vec<String>, xu_ly: XuLy<V>, tra_loi: TraLoi, khi_ghi: Option<KhiGhi>) {
        let mut khoa: Vec<String> = Vec::with_capacity(cac_khoa.len()); // bỏ trùng, giữ thứ tự
        for k in cac_khoa {
            if !khoa.contains(&k) {
                khoa.push(k);
            }
        }
        let che_do = {
            let mut c = self.cong.lock().unwrap();
            while c.dong {
                c = self.cong_cv.wait(c).unwrap();
            }
            c.dang_chay += 1;
            self.che_do.load(Ordering::SeqCst)
        };
        let o = {
            let b = self.o.read().unwrap();
            khoa.iter().map(|k| b.chi_muc.get(k).map(|&i| Arc::clone(&b.ds[i].1))).collect::<Option<Vec<_>>>()
        };
        let Some(o) = o else {
            self.xong_mot();
            tra_loi(obj(json!({"ok": false, "ma_loi": "khong_co", "ly_do": "không có tài nguyên"})));
            return;
        };
        if che_do == 3 {
            let n = self.do_dai.fetch_add(1, Ordering::SeqCst) + 1;
            let viec = Viec { o, cac_khoa: khoa, xu_ly, khi_ghi, tra_loi };
            if let Some(tx) = self.hang.lock().unwrap().as_ref() {
                let _ = tx.send(viec);
            }
            self.thong_ke.dat_max("hang_doi_max", n as u64);
            return; // luồng xử lý sẽ gọi tra_loi
        }
        let mut khi_ghi = khi_ghi;
        let kq = catch_unwind(AssertUnwindSafe(|| match che_do {
            0 => self.mot_lan(&o, &khoa, &xu_ly, &mut khi_ghi),
            1 => self.co_khoa(&o, &khoa, &xu_ly, &mut khi_ghi),
            _ => self.lac_quan(&o, &khoa, &xu_ly, &mut khi_ghi),
        }))
        .unwrap_or_else(loi_panic); // lỗi nghiệp vụ: trả lời lỗi, không làm sập luồng
        self.xong_mot();
        tra_loi(kq);
    }

    fn xong_mot(&self) {
        let mut c = self.cong.lock().unwrap_or_else(|e| e.into_inner());
        c.dang_chay -= 1;
        if c.dang_chay == 0 {
            self.cong_cv.notify_all();
        }
    }

    fn doc_cac_o(o: &[Arc<O<V>>], khoa: &[String]) -> Doc<V> {
        Doc { ds: khoa.iter().zip(o).map(|(k, o)| (k.clone(), o.doc())).collect() }
    }

    fn ghi(o: &[Arc<O<V>>], khoa: &[String], ghi: Ghi<V>, kq: &mut Obj, khi_ghi: &mut Option<KhiGhi>) {
        for (k, v) in ghi {
            let i = khoa.iter().position(|x| *x == k).unwrap_or_else(|| panic!("ghi ô {k} ngoài cac_khoa"));
            *o[i].gia_tri.lock().unwrap() = Arc::new(v);
            o[i].phien_ban.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(f) = khi_ghi.take() {
            f(kq);
        }
    }

    /// khong_khoa (và luồng hang_doi): đọc -> trễ -> xử lý -> ghi, KHÔNG khóa.
    /// Hai luồng cùng đọc giá trị cũ rồi cùng ghi: bản ghi sau đè bản trước (mất cập nhật).
    fn mot_lan(&self, o: &[Arc<O<V>>], khoa: &[String], xu_ly: &XuLy<V>, khi_ghi: &mut Option<KhiGhi>) -> Obj {
        let doc = Self::doc_cac_o(o, khoa);
        self.tre();
        let (mut kq, ghi) = xu_ly(&doc);
        if let Some(g) = ghi.filter(|g| !g.is_empty()) {
            Self::ghi(o, khoa, g, &mut kq, khi_ghi);
        }
        kq
    }

    /// khoa: giữ khóa của MỌI ô liên quan suốt đọc – xử lý – ghi (vùng găng – critical section).
    /// Khóa theo thứ tự khóa ô đã sắp xếp => mọi luồng xin khóa cùng một thứ tự => không khóa chết.
    /// khoa_ngay_tho: khóa theo thứ tự yêu cầu (ví dụ nguồn rồi đích), nghỉ giữa hai lần khóa.
    fn co_khoa(&self, o: &[Arc<O<V>>], khoa: &[String], xu_ly: &XuLy<V>, khi_ghi: &mut Option<KhiGhi>) -> Obj {
        let ngay_tho = self.khoa_ngay_tho.load(Ordering::SeqCst);
        let mut thu_tu: Vec<usize> = (0..khoa.len()).collect();
        if !ngay_tho {
            thu_tu.sort_by(|&a, &b| khoa[a].cmp(&khoa[b]));
        }
        struct DaGiu<'a, V>(Vec<&'a O<V>>);
        impl<V> Drop for DaGiu<'_, V> {
            fn drop(&mut self) {
                for o in self.0.iter().rev() {
                    o.khoa.tra();
                }
            }
        }
        let mut da_giu = DaGiu(Vec::new());
        for (n, &i) in thu_tu.iter().enumerate() {
            if n > 0 && ngay_tho {
                self.tre();
            }
            if !o[i].khoa.lay(Duration::from_secs_f64(CHO_KHOA_TOI_DA_S)) {
                self.thong_ke.cong("khoa_cho_lau", 1);
                return obj(json!({"ok": false, "ma_loi": "khoa_cho_lau",
                    "ly_do": format!("chờ khóa {} quá {} s (khóa chết?)", khoa[i], fmt_g(CHO_KHOA_TOI_DA_S))}));
            }
            da_giu.0.push(&o[i]);
        }
        self.mot_lan(o, khoa, xu_ly, khi_ghi)
    }

    /// lac_quan (Kung & Robinson 1981): không giữ khóa khi xử lý; lúc ghi kiểm tra phiên bản.
    fn lac_quan(&self, o: &[Arc<O<V>>], khoa: &[String], xu_ly: &XuLy<V>, khi_ghi: &mut Option<KhiGhi>) -> Obj {
        for lan in 0..SO_LAN_THU {
            let (doc, phien_ban) = {
                let _g = self.khoa_cas.lock().unwrap(); // ảnh chụp nhất quán của các ô
                (Self::doc_cac_o(o, khoa), o.iter().map(|x| x.phien_ban.load(Ordering::SeqCst)).collect::<Vec<u64>>())
            };
            self.tre();
            let (mut kq, ghi) = xu_ly(&doc);
            let Some(ghi) = ghi.filter(|g| !g.is_empty()) else {
                return kq; // chỉ đọc / từ chối: không cần so sánh-và-đổi
            };
            {
                let _g = self.khoa_cas.lock().unwrap(); // so sánh-và-đổi (compare-and-swap)
                if o.iter().zip(&phien_ban).all(|(x, &pb)| x.phien_ban.load(Ordering::SeqCst) == pb) {
                    for (k, v) in ghi {
                        let i = khoa.iter().position(|x| *x == k).unwrap_or_else(|| panic!("ghi ô {k} ngoài cac_khoa"));
                        *o[i].gia_tri.lock().unwrap() = Arc::new(v);
                        // phiên bản: compare_exchange(cũ, cũ + 1) – chắc chắn thành công vì đã kiểm dưới khóa CAS
                        let _ = o[i].phien_ban.compare_exchange(phien_ban[i], phien_ban[i] + 1, Ordering::SeqCst, Ordering::SeqCst);
                    }
                    if let Some(f) = khi_ghi.take() {
                        f(&mut kq);
                    }
                    return kq;
                }
            }
            self.thong_ke.cong("xung_dot", 1); // có luồng khác ghi trước: làm lại từ đầu
            if lan < SO_LAN_THU - 1 {
                self.thong_ke.cong("thu_lai", 1);
                ngu(ngau_nhien_khoang(0.0, 0.001));
            }
        }
        self.thong_ke.cong("het_lan_thu", 1);
        obj(json!({"ok": false, "ma_loi": "xung_dot", "ly_do": "xung đột, thử lại sau"}))
    }

    /// hang_doi: MỘT luồng lấy lần lượt từng việc (đến trước phục vụ trước) – không cần khóa vì
    /// chỉ có luồng này chạm vào trạng thái.
    fn vong_hang_doi(kho: std::sync::Weak<Self>, rx: mpsc::Receiver<Viec<V>>) {
        while let Ok(mut v) = rx.recv() {
            let Some(kho) = kho.upgrade() else { return };
            kho.do_dai.fetch_sub(1, Ordering::SeqCst);
            let kq = catch_unwind(AssertUnwindSafe(|| kho.mot_lan(&v.o, &v.cac_khoa, &v.xu_ly, &mut v.khi_ghi))).unwrap_or_else(loi_panic);
            kho.xong_mot();
            let tra_loi = v.tra_loi;
            let _ = catch_unwind(AssertUnwindSafe(move || tra_loi(kq)));
        }
    }

    pub fn dong(&self) {
        self.hang.lock().unwrap().take();
    }
}

// ================================================================ 4. Sổ kiểm toán
pub type MaTen = &'static [(&'static str, &'static str)];

/// Nơi luật kiểm toán báo một sai phạm (đếm theo mã, giữ 5 ví dụ đầu).
pub struct SaiPham<'a> {
    ten: MaTen,
    dem: &'a mut [u64],
    vi_du: &'a mut Vec<String>,
}

impl SaiPham<'_> {
    pub fn bao(&mut self, ma: &str, mo_ta: String) {
        let i = self.ten.iter().position(|(m, _)| *m == ma).expect("mã sai phạm không có trong TEN_SAI_PHAM");
        self.dem[i] += 1;
        if self.vi_du.len() < 5 {
            self.vi_du.push(mo_ta);
        }
    }
}

/// Luật của sổ kiểm toán (mỗi bài một kiểu):
///   kiem_tra(gd)        – gọi khi ghi từng giao dịch (theo thứ tự ghi sổ), báo sp.bao(ma, mo_ta);
///   kiem_tra_cuoi(doc)  – đối chiếu sổ với trạng thái hiện tại của kho, trả về [(ma, số)].
pub trait LuatKiemToan: Send + 'static {
    type GiaoDich: Send + 'static;
    type GiaTri: Send + Sync + 'static;
    /// mã -> tên ngắn (hiện trong bảng, dòng in)
    const TEN_SAI_PHAM: MaTen;
    fn kiem_tra(&mut self, gd: &Self::GiaoDich, sp: &mut SaiPham);
    fn kiem_tra_cuoi(&self, _doc: &Doc<Self::GiaTri>) -> Vec<(&'static str, u64)> {
        Vec::new()
    }
}

struct SoTrong<L: LuatKiemToan> {
    luat: L,
    giao_dich: Vec<L::GiaoDich>,
    dem: Vec<u64>,
    vi_du: Vec<String>, // mô tả vài sai phạm đầu tiên
}

/// Kết quả tổng kết sổ: [(mã, tên, số)], tổng số sai phạm, vài ví dụ.
pub struct TongKet {
    pub ds: Vec<(&'static str, &'static str, u64)>,
    pub tong: u64,
    pub vi_du: Vec<String>,
}

/// Ghi mọi giao dịch ĐƯỢC CHẤP NHẬN (dưới khóa riêng của sổ – không ảnh hưởng tranh chấp của kho) và
/// đếm sai phạm theo luật L.
pub struct SoKiemToan<L: LuatKiemToan> {
    k: Mutex<SoTrong<L>>,
}

impl<L: LuatKiemToan> SoKiemToan<L> {
    pub fn new(luat: L) -> SoKiemToan<L> {
        SoKiemToan { k: Mutex::new(SoTrong { luat, giao_dich: Vec::new(), dem: vec![0; L::TEN_SAI_PHAM.len()], vi_du: Vec::new() }) }
    }

    pub fn ghi(&self, gd: L::GiaoDich) {
        let mut g = self.k.lock().unwrap();
        let t = &mut *g;
        t.giao_dich.push(gd);
        let mut sp = SaiPham { ten: L::TEN_SAI_PHAM, dem: &mut t.dem, vi_du: &mut t.vi_du };
        t.luat.kiem_tra(t.giao_dich.last().unwrap(), &mut sp);
    }

    /// Đọc trạng thái của luật (ví dụ số vé theo sổ) dưới khóa sổ.
    pub fn xem<R>(&self, f: impl FnOnce(&L) -> R) -> R {
        f(&self.k.lock().unwrap().luat)
    }

    pub fn tong_ket(&self, doc: &Doc<L::GiaTri>) -> TongKet {
        let t = self.k.lock().unwrap();
        let mut dem = t.dem.clone();
        for (ma, so) in t.luat.kiem_tra_cuoi(doc) {
            if let Some(i) = L::TEN_SAI_PHAM.iter().position(|(m, _)| *m == ma) {
                dem[i] += so;
            }
        }
        TongKet { ds: L::TEN_SAI_PHAM.iter().zip(&dem).map(|((m, t), &s)| (*m, *t, s)).collect(), tong: dem.iter().sum(), vi_du: t.vi_du.clone() }
    }
}

// ================================================================ 5. Máy chủ TCP đa luồng
struct GuiTrong {
    conn: TcpStream,
    da_tra: HashMap<i64, Option<Obj>>, // ma_yc -> kết quả đã gửi (None: đang xử lý)
    thu_tu: VecDeque<i64>,
}

/// Một kết nối tới máy chủ (người dùng thật, kết nối gộp của bộ mô phỏng, hoặc quản trị).
pub struct KhachKetNoi {
    pub ma: u64,
    pub ten: String,
    pub vai_tro: String,
    pub so_nguoi: Option<i64>, // None: người dùng thật; m: kết nối gộp m người ảo
    gui: Mutex<GuiTrong>,      // nhiều luồng (kết nối, hàng đợi, đẩy) cùng gửi
    sock: TcpStream,           // bản sao để dừng (shutdown) mà không cần khóa gửi
    pub so_yc: AtomicU64,
}

impl KhachKetNoi {
    pub fn gui(&self, o: &Obj) -> bool {
        gui_json(&mut self.gui.lock().unwrap().conn, o).is_ok()
    }

    fn gui_tho(&self, du_lieu: &[u8]) {
        let _ = self.gui.lock().unwrap().conn.write_all(du_lieu);
    }

    /// true nếu ma_yc mới. ma_yc đã gặp: gửi lại kết quả cũ (nếu đã có), trả về false.
    fn nhan_yc(&self, ma_yc: i64) -> bool {
        let mut g = self.gui.lock().unwrap();
        if let Some(cu) = g.da_tra.get(&ma_yc) {
            if let Some(cu) = cu.clone() {
                let mut lai = cu;
                lai.insert("lap_lai".into(), json!(true));
                let _ = gui_json(&mut g.conn, &lai);
            }
            return false;
        }
        g.da_tra.insert(ma_yc, None);
        g.thu_tu.push_back(ma_yc);
        if g.thu_tu.len() > NHO_KQ {
            if let Some(cu) = g.thu_tu.pop_front() {
                g.da_tra.remove(&cu);
            }
        }
        true
    }

    fn tra(&self, ma_yc: i64, tin: Obj) {
        let mut g = self.gui.lock().unwrap();
        let _ = gui_json(&mut g.conn, &tin);
        if g.da_tra.insert(ma_yc, Some(tin)).is_none() {
            g.thu_tu.push_back(ma_yc);
        }
    }
}

/// Phần nghiệp vụ của một bài. Lớp khung MayChu<N> gọi:
///   tham_so_mac_dinh()           tham số đặt lại và giá trị mặc định (kiểu của giá trị = kiểu tham số)
///   kiem_tham_so(ts)             kiểm tra thêm tham số đặt lại (Err = từ chối)
///   khoi_tao(mc)                 các ô ban đầu của kho + luật sổ kiểm toán (gọi trong tam_dung)
///   xu_ly_yeu_cau(mc, khach, nguoi, tin, tra_loi)   kiểm tra tin, gọi mc.kho.thuc_hien(...)
///   trang_thai(mc)               trạng thái nghiệp vụ cho cap_nhat (đọc không khóa)
///   tom_tat(mc)                  một dòng ngắn (≤ 48 ký tự) mô tả kết quả
///   dinh_ky(mc, bay_gio)         (tùy chọn) gọi mỗi 100 ms trên luồng nền (đồng hồ, hết hạn…)
/// Báo đổi trạng thái: mc.bao_doi() (đẩy cap_nhat ở nhịp kế tiếp); thông báo: mc.thong_bao(text).
pub trait NghiepVu: Sized + Send + Sync + 'static {
    type GiaTri: Send + Sync + 'static;
    type Luat: LuatKiemToan<GiaTri = Self::GiaTri>;
    const TEN: &'static str;
    const LOAI_YEU_CAU: &'static [&'static str];
    fn tham_so_mac_dinh() -> Obj;
    fn moi() -> Self;
    fn kiem_tham_so(_ts: &Obj) -> Result<(), String> {
        Ok(())
    }
    fn khoi_tao(mc: &MayChu<Self>) -> (Vec<(String, Self::GiaTri)>, Self::Luat);
    fn xu_ly_yeu_cau(mc: &Arc<MayChu<Self>>, khach: &KhachKetNoi, nguoi: &str, tin: &Obj, tra_loi: TraLoi);
    fn trang_thai(mc: &MayChu<Self>) -> Obj;
    fn tom_tat(mc: &MayChu<Self>) -> String;
    fn dinh_ky(_mc: &Arc<MayChu<Self>>, _bay_gio: f64) {}
}

/// Ép giá trị v về kiểu của giá trị mặc định (như type(mặc_định)(v) của Python).
pub fn ep_kieu(mac_dinh: &Value, v: &Value) -> Result<Value, String> {
    match mac_dinh {
        Value::Number(n) if n.is_i64() || n.is_u64() => match v {
            Value::Number(x) if x.is_i64() || x.is_u64() => Ok(v.clone()),
            Value::Number(x) => Ok(json!(x.as_f64().unwrap_or(0.0).trunc() as i64)),
            Value::Bool(b) => Ok(json!(*b as i64)),
            Value::String(s) => s.trim().parse::<i64>().map(|n| json!(n)).map_err(|_| format!("invalid literal for int() with base 10: '{s}'")),
            _ => Err(format!("int() không nhận {}", py_str(v))),
        },
        Value::Number(_) => match v {
            Value::Number(x) => Ok(json!(x.as_f64().unwrap_or(0.0))),
            Value::Bool(b) => Ok(json!(*b as i64 as f64)),
            Value::String(s) => s.trim().parse::<f64>().map(|n| json!(n)).map_err(|_| format!("could not convert string to float: '{s}'")),
            _ => Err(format!("float() không nhận {}", py_str(v))),
        },
        Value::String(_) => Ok(json!(py_str(v))),
        _ => Ok(v.clone()),
    }
}

/// Khung máy chủ: kho tài nguyên + thống kê + sổ kiểm toán + danh sách kết nối; phần nghiệp vụ là `nv`.
pub struct MayChu<N: NghiepVu> {
    pub nv: N,
    pub kho: Arc<KhoTranhChap<N::GiaTri>>,
    pub thong_ke: Arc<ThongKe>,
    pub bao: Bao,
    tham_so: Mutex<Obj>,
    so: RwLock<Option<Arc<SoKiemToan<N::Luat>>>>,
    ds: Mutex<Vec<Arc<KhachKetNoi>>>,
    chay: AtomicBool,
    dia_chi: Mutex<String>,
    doi: AtomicBool,
    su_kien: Mutex<VecDeque<String>>,
    luong: Mutex<Vec<JoinHandle<()>>>,
    ma: AtomicU64,
}

impl<N: NghiepVu> MayChu<N> {
    pub fn new(che_do: &str, tre_ms: f64, bao: Bao, tham_so: Obj) -> Result<Arc<Self>, String> {
        let thong_ke = Arc::new(ThongKe::default());
        let kho = KhoTranhChap::new(Arc::clone(&thong_ke), che_do, tre_ms)?;
        let mut ts = N::tham_so_mac_dinh();
        let mut moi = Obj::new();
        for (k, v) in &tham_so {
            let md = ts.get(k).ok_or_else(|| format!("tham số lạ: {k}"))?;
            moi.insert(k.clone(), ep_kieu(md, v)?);
        }
        N::kiem_tham_so(&moi)?;
        for (k, v) in moi {
            ts.insert(k, v);
        }
        let mc = Arc::new(MayChu {
            nv: N::moi(),
            kho,
            thong_ke,
            bao,
            tham_so: Mutex::new(ts),
            so: RwLock::new(None),
            ds: Mutex::new(Vec::new()),
            chay: AtomicBool::new(false),
            dia_chi: Mutex::new(String::new()),
            doi: AtomicBool::new(false),
            su_kien: Mutex::new(VecDeque::new()),
            luong: Mutex::new(Vec::new()),
            ma: AtomicU64::new(1),
        });
        mc.khoi_tao_lai();
        Ok(mc)
    }

    // ------------------------------------------------------------ truy cập
    pub fn so(&self) -> Arc<SoKiemToan<N::Luat>> {
        Arc::clone(self.so.read().unwrap().as_ref().expect("chưa khởi tạo sổ"))
    }
    pub fn tham_so(&self) -> Obj {
        self.tham_so.lock().unwrap().clone()
    }
    pub fn ts_i64(&self, k: &str) -> i64 {
        self.tham_so.lock().unwrap().get(k).and_then(Value::as_i64).unwrap_or(0)
    }
    pub fn ts_f64(&self, k: &str) -> f64 {
        self.tham_so.lock().unwrap().get(k).and_then(Value::as_f64).unwrap_or(0.0)
    }
    pub fn ts_str(&self, k: &str) -> String {
        self.tham_so.lock().unwrap().get(k).map(py_str).unwrap_or_default()
    }
    /// Đổi MỘT tham số ngay khi đang chạy, không đặt lại phiên (ví dụ tốc độ cấp lượt của MR6.13 –
    /// như self.tham_so[k] = v của bản Python). Kiểm tra bằng kiem_tham_so trước khi đổi.
    pub fn dat_mot_tham_so(&self, k: &str, v: Value) -> Result<(), String> {
        let md = N::tham_so_mac_dinh();
        let v = ep_kieu(md.get(k).ok_or_else(|| format!("tham số lạ: {k}"))?, &v)?;
        N::kiem_tham_so(&obj(json!({ k: v.clone() })))?;
        self.tham_so.lock().unwrap().insert(k.to_string(), v);
        Ok(())
    }
    pub fn dang_chay(&self) -> bool {
        self.chay.load(Ordering::SeqCst)
    }
    pub fn dia_chi(&self) -> String {
        self.dia_chi.lock().unwrap().clone()
    }

    // ------------------------------------------------------------ điều khiển
    fn khoi_tao_lai(&self) {
        self.kho.tam_dung(|| {
            // không còn thao tác nào đang chạy
            self.thong_ke.dat_lai();
            let (o, luat) = N::khoi_tao(self);
            self.kho.thay_o(o);
            *self.so.write().unwrap() = Some(Arc::new(SoKiemToan::new(luat)));
        });
        self.bao_doi();
    }

    pub fn dat_lai(&self, tham_so: &Obj) -> Result<(), String> {
        let mac_dinh = N::tham_so_mac_dinh();
        let mut ts = Obj::new();
        for (k, v) in tham_so {
            // ép kiểu theo giá trị mặc định
            let md = mac_dinh.get(k).ok_or_else(|| format!("tham số lạ: {k}"))?;
            ts.insert(k.clone(), ep_kieu(md, v)?);
        }
        N::kiem_tham_so(&ts)?;
        let tk = self.thong_ke.anh_chup();
        let so_yc = tk.get("so_yc").and_then(Value::as_u64).unwrap_or(0);
        if so_yc > 0 {
            let tong = self.so().tong_ket(&self.kho.anh_chup()).tong;
            (self.bao)(&format!("Lượt trước: {}, {} yc, tc {}, sai phạm {}", self.kho.che_do(), so_yc, tk["thanh_cong"], tong));
        }
        {
            let mut t = self.tham_so.lock().unwrap();
            for (k, v) in &ts {
                t.insert(k.clone(), v.clone());
            }
        }
        self.khoi_tao_lai();
        let them = if ts.is_empty() { String::new() } else { format!(" ({})", ts.iter().map(|(k, v)| format!("{k}={}", py_str(v))).collect::<Vec<_>>().join(", ")) };
        self.thong_bao(&format!("Đặt lại{them}"));
        Ok(())
    }

    pub fn dat_che_do(&self, che_do: &str) -> Result<(), String> {
        if che_do == self.kho.che_do() {
            return Ok(());
        }
        self.kho.dat_che_do(che_do)?;
        self.thong_bao(&format!("Chế độ: {che_do} – {}", ten_che_do(che_do)));
        self.bao_doi();
        Ok(())
    }

    pub fn dat_tre(&self, ms: f64) -> Result<(), String> {
        self.kho.dat_tre(ms)?;
        (self.bao)(&format!("Độ trễ xử lý: ngẫu nhiên 0–{} ms", fmt_g(self.kho.tre_ms())));
        Ok(())
    }

    pub fn bao_doi(&self) {
        self.doi.store(true, Ordering::SeqCst);
    }

    /// Ghi nhật ký máy chủ và gửi {"loai":"su_kien"} cho mọi client (ở nhịp đẩy kế tiếp).
    pub fn thong_bao(&self, text: &str) {
        (self.bao)(text);
        self.su_kien.lock().unwrap().push_back(text.to_string());
    }

    // ------------------------------------------------------------ khởi động / dừng
    pub fn bat_dau(self: &Arc<Self>, host: &str, cong: u16) -> Result<(), String> {
        let dc = (host, cong).to_socket_addrs().map_err(|e| e.to_string())?.next().ok_or("không phân giải được địa chỉ")?;
        let s = socket2::Socket::new(socket2::Domain::for_address(dc), socket2::Type::STREAM, Some(socket2::Protocol::TCP)).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        let _ = s.set_reuse_address(true);
        s.bind(&dc.into()).map_err(|e| e.to_string())?;
        s.listen(1024).map_err(|e| e.to_string())?;
        let srv: TcpListener = s.into();
        srv.set_nonblocking(true).map_err(|e| e.to_string())?;
        *self.dia_chi.lock().unwrap() = format!("{host}:{cong}");
        self.chay.store(true, Ordering::SeqCst);
        let (a, b) = (Arc::clone(self), Arc::clone(self));
        *self.luong.lock().unwrap() = vec![thread::spawn(move || a.nhan_ket_noi(srv)), thread::spawn(move || b.vong_nen())];
        (self.bao)(&format!("Lắng nghe {host}:{cong}; chế độ {}, trễ 0–{} ms", self.kho.che_do(), fmt_g(self.kho.tre_ms())));
        Ok(())
    }

    pub fn dung(&self) {
        if !self.chay.swap(false, Ordering::SeqCst) {
            return;
        }
        let ds: Vec<Arc<KhachKetNoi>> = self.ds.lock().unwrap().clone();
        for k in ds {
            let _ = k.sock.shutdown(Shutdown::Both);
        }
        let luong: Vec<JoinHandle<()>> = std::mem::take(&mut *self.luong.lock().unwrap());
        for t in luong {
            let _ = t.join();
        }
        (self.bao)("Máy chủ đã dừng");
    }

    // ------------------------------------------------------------ kết nối
    fn nhan_ket_noi(self: Arc<Self>, srv: TcpListener) {
        while self.dang_chay() {
            match srv.accept() {
                Ok((conn, _)) => {
                    let _ = conn.set_nonblocking(false);
                    let mc = Arc::clone(&self);
                    thread::spawn(move || mc.phuc_vu(conn));
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => ngu(0.005),
                Err(e) if e.kind() == io::ErrorKind::Interrupted || e.kind() == io::ErrorKind::ConnectionAborted => {}
                Err(_) => break,
            }
        }
    }

    /// Luồng của một kết nối: chào, rồi đọc và xử lý lần lượt từng yêu cầu (có thể pipelining).
    fn phuc_vu(self: Arc<Self>, conn: TcpStream) {
        let _ = conn.set_nodelay(true);
        let Ok(doc) = conn.try_clone() else { return };
        let mut tep = BufReader::new(doc);
        let mut k: Option<Arc<KhachKetNoi>> = None;
        let kq: io::Result<()> = (|| {
            let tin = doc_dong(&mut tep)?;
            let so_nguoi = tin.as_ref().and_then(|t| t.get("so_nguoi")).filter(|v| !v.is_null()).cloned();
            let hop_le = tin.as_ref().is_some_and(|t| {
                lay_str(t, "loai") == Some("chao")
                    && lay_str(t, "ten").is_some_and(ten_hop_le)
                    && t.get("vai_tro").map_or(true, |v| v.as_str().is_some_and(|s| VAI_TRO.contains(&s)))
                    && so_nguoi.as_ref().map_or(true, |v| la_so_nguyen(Some(v)).is_some_and(|m| m > 0))
            });
            if !hop_le {
                let mut c = conn.try_clone()?;
                gui_json(&mut c, &obj(json!({"loai": "loi", "ly_do": "cần chào: tên 1–16 ký tự, vai_tro hợp lệ"})))?;
                return Ok(());
            }
            let tin = tin.unwrap();
            let kn = Arc::new(KhachKetNoi {
                ma: self.ma.fetch_add(1, Ordering::SeqCst),
                ten: lay_str(&tin, "ten").unwrap_or("").to_string(),
                vai_tro: lay_str(&tin, "vai_tro").unwrap_or("nguoi_dung").to_string(),
                so_nguoi: so_nguoi.as_ref().and_then(Value::as_i64),
                gui: Mutex::new(GuiTrong { conn: conn.try_clone()?, da_tra: HashMap::new(), thu_tu: VecDeque::new() }),
                sock: conn.try_clone()?,
                so_yc: AtomicU64::new(0),
            });
            k = Some(Arc::clone(&kn));
            {
                // luồng đẩy chưa gửi được gì cho kết nối này trước lời chào
                let mut g = kn.gui.lock().unwrap();
                self.ds.lock().unwrap().push(Arc::clone(&kn));
                gui_json(&mut g.conn, &obj(json!({"loai": "chao", "ma": kn.ma, "che_do": self.kho.che_do(), "may_chu": N::TEN})))?;
                gui_json(&mut g.conn, &self.goi_cap_nhat())?;
            }
            if kn.so_nguoi.is_none() && kn.vai_tro == "nguoi_dung" {
                // kết nối gộp, quản trị: không in
                (self.bao)(&format!("{} vào", kn.ten));
            }
            while self.dang_chay() {
                let Some(tin) = doc_dong(&mut tep)? else { break };
                self.nhan_tin(&kn, tin);
            }
            Ok(())
        })();
        if let (Err(e), Some(kn)) = (&kq, &k) {
            if self.dang_chay() && kn.so_nguoi.is_none() {
                (self.bao)(&format!("{}: lỗi kết nối ({e})", kn.ten));
            }
        }
        if let Some(kn) = k {
            self.ds.lock().unwrap().retain(|x| x.ma != kn.ma);
            if kn.so_nguoi.is_none() && kn.vai_tro == "nguoi_dung" && self.dang_chay() {
                (self.bao)(&format!("{} rời", kn.ten));
            }
        }
        let _ = conn.shutdown(Shutdown::Both);
    }

    fn nhan_tin(self: &Arc<Self>, k: &Arc<KhachKetNoi>, tin: Obj) {
        let loai = tin.get("loai").map(py_str).unwrap_or_else(|| "None".into());
        let Some(ma_yc) = lay_i64(&tin, "ma_yc") else {
            k.gui(&obj(json!({"loai": "loi", "ly_do": "thiếu ma_yc (số nguyên)"})));
            return;
        };
        if !k.nhan_yc(ma_yc) {
            return; // ma_yc lặp lại: đã gửi lại kết quả cũ
        }
        k.so_yc.fetch_add(1, Ordering::Relaxed);
        let tu_choi = |ma_loi: &str, ly_do: String| obj(json!({"loai": "kq", "ma_yc": ma_yc, "ok": false, "ma_loi": ma_loi, "ly_do": ly_do}));
        if tin.get("loai").and_then(Value::as_str).is_some_and(|l| N::LOAI_YEU_CAU.contains(&l)) {
            let nguoi = if k.so_nguoi.is_some() { lay_str(&tin, "nguoi").map(str::to_string) } else { Some(k.ten.clone()) };
            let Some(nguoi) = nguoi.filter(|n| ten_hop_le(n)) else {
                k.tra(ma_yc, tu_choi("sai_du_lieu", "thiếu tên người (nguoi)".into()));
                return;
            };
            let t0 = Instant::now();
            self.thong_ke.bat_dau_yc();
            let (tk, kn) = (Arc::clone(&self.thong_ke), Arc::clone(k));
            let tra_loi: TraLoi = Box::new(move |kq: Obj| {
                // gọi từ luồng kết nối hoặc luồng hàng đợi
                tk.xong_yc(&kq, t0.elapsed().as_secs_f64() * 1e6);
                let mut tin = obj(json!({"loai": "kq", "ma_yc": ma_yc}));
                tin.extend(kq);
                kn.tra(ma_yc, tin);
            });
            N::xu_ly_yeu_cau(self, k, &nguoi, &tin, tra_loi);
        } else if ["che_do", "tre", "dat_lai", "thong_ke"].contains(&loai.as_str()) {
            if k.vai_tro != "quan_tri" {
                k.tra(ma_yc, tu_choi("khong_quyen", "chỉ quản trị".into()));
                return;
            }
            if loai == "thong_ke" {
                let mut t = obj(json!({"loai": "thong_ke", "ma_yc": ma_yc}));
                t.extend(self.thong_ke_day_du());
                k.tra(ma_yc, t);
                return;
            }
            let kq = match loai.as_str() {
                "che_do" => self.dat_che_do(&tin.get("gia_tri").map(py_str).unwrap_or_else(|| "None".into())),
                "tre" => py_float(tin.get("ms")).and_then(|ms| self.dat_tre(ms)),
                _ => self.dat_lai(&tin.iter().filter(|(x, _)| *x != "loai" && *x != "ma_yc").map(|(x, v)| (x.clone(), v.clone())).collect()),
            };
            match kq {
                Ok(()) => k.tra(ma_yc, obj(json!({"loai": "kq", "ma_yc": ma_yc, "ok": true}))),
                Err(e) => k.tra(ma_yc, tu_choi("sai_du_lieu", e)),
            }
        } else {
            k.tra(ma_yc, tu_choi("khong_ho_tro", format!("loại không hỗ trợ: {loai}")));
        }
    }

    // ------------------------------------------------------------ đẩy trạng thái (gộp ≤ 10 lần/giây)
    fn goi_cap_nhat(&self) -> Obj {
        let mut o = obj(json!({"loai": "cap_nhat", "che_do": self.kho.che_do()}));
        o.extend(N::trang_thai(self));
        o
    }

    fn vong_nen(self: Arc<Self>) {
        while self.dang_chay() {
            ngu(NHIP_DAY_S);
            if catch_unwind(AssertUnwindSafe(|| N::dinh_ky(&self, bay_gio()))).is_err() {
                (self.bao)("lỗi định kỳ");
            }
            let mut goi = Vec::new();
            while let Some(text) = self.su_kien.lock().unwrap().pop_front() {
                goi.extend(ma_hoa(&obj(json!({"loai": "su_kien", "noi_dung": text}))));
            }
            if self.doi.swap(false, Ordering::SeqCst) {
                // xóa cờ TRƯỚC khi chụp: đổi sau đó vẫn được đẩy
                goi.extend(ma_hoa(&self.goi_cap_nhat()));
            }
            if goi.is_empty() {
                continue;
            }
            let ds: Vec<Arc<KhachKetNoi>> = self.ds.lock().unwrap().clone();
            for k in ds {
                k.gui_tho(&goi);
            }
        }
    }

    // ------------------------------------------------------------ số liệu
    /// Trạng thái hiện tại (như cap_nhat gửi client) – giao diện máy chủ đọc định kỳ.
    pub fn trang_thai(&self) -> Obj {
        self.goi_cap_nhat()
    }

    pub fn so_ket_noi(&self) -> (usize, i64) {
        let ds = self.ds.lock().unwrap();
        let nguoi = ds.iter().filter(|k| k.vai_tro != "quan_tri").map(|k| k.so_nguoi.unwrap_or(1)).sum();
        (ds.len(), nguoi)
    }

    pub fn thong_ke_day_du(&self) -> Obj {
        let tk = self.thong_ke.anh_chup();
        let tong = self.so().tong_ket(&self.kho.anh_chup());
        let (so_kn, so_nguoi) = self.so_ket_noi();
        let mut d = obj(json!({"che_do": self.kho.che_do(), "tre_ms": self.kho.tre_ms()}));
        d.extend(tk);
        d.insert("hang_doi".into(), json!(self.kho.do_dai_hang()));
        d.insert("sai_pham".into(), Value::Array(tong.ds.iter().map(|(m, t, s)| json!({"ma": m, "ten": t, "so": s})).collect()));
        d.insert("sai_pham_tong".into(), json!(tong.tong));
        d.insert("vi_du_sai_pham".into(), json!(tong.vi_du.iter().take(3).collect::<Vec<_>>()));
        d.insert("tom_tat".into(), json!(N::tom_tat(self)));
        d.insert("so_ket_noi".into(), json!(so_kn));
        d.insert("so_nguoi".into(), json!(so_nguoi));
        d
    }
}

// ================================================================ 6. Định dạng in (giống nhau ở 5 ngôn ngữ)
/// 5300000 -> "5.300.000 đ" (dấu chấm ngăn nghìn).
pub fn tien(x: i64) -> String {
    let s = x.unsigned_abs().to_string();
    let mut ra = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            ra.push('.');
        }
        ra.push(c);
    }
    format!("{}{ra} đ", if x < 0 { "-" } else { "" })
}

/// repr() của số thực Python: 5.0, 0.25, 1e+16.
pub fn py_repr_f64(x: f64) -> String {
    if x.is_finite() && x.fract() == 0.0 && x.abs() < 1e16 {
        format!("{x:.1}")
    } else if x.is_nan() {
        "nan".into()
    } else if x.is_infinite() {
        if x > 0.0 { "inf".into() } else { "-inf".into() }
    } else {
        format!("{x}")
    }
}

/// str() của một giá trị JSON theo Python (số nguyên 5, số thực 5.0, None, True…).
pub fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) if n.is_i64() || n.is_u64() => n.to_string(),
        Value::Number(n) => py_repr_f64(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Null => "None".into(),
        _ => v.to_string(),
    }
}

/// float(x) của Python cho một giá trị JSON (số hoặc chuỗi số).
pub fn py_float(v: Option<&Value>) -> Result<f64, String> {
    match v {
        Some(Value::Number(n)) => Ok(n.as_f64().unwrap_or(0.0)),
        Some(Value::String(s)) => s.trim().parse().map_err(|_| format!("could not convert string to float: '{s}'")),
        Some(x) => Err(format!("float() argument must be a string or a real number, not {}", py_str(x))),
        None => Err("float() argument must be a string or a real number, not 'NoneType'".into()),
    }
}

/// f"{x:g}" của Python: 2.0 -> "2", 0.5 -> "0.5", 2.25 -> "2.25" (6 chữ số có nghĩa).
pub fn fmt_g(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return py_repr_f64(x);
    }
    let mu = x.abs().log10().floor() as i32;
    if !(-4..6).contains(&mu) {
        let s = format!("{:.5e}", x);
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
        let e: i32 = e.parse().unwrap_or(0);
        return format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    }
    let s = format!("{:.*}", (5 - mu).max(0) as usize, x);
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// Một số trong thông điệp thong_ke: số nguyên in nguyên, số thực như Python, null -> "–".
pub fn so_us(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "–".into(),
        Some(x) => py_str(x),
    }
}

fn tk_so(tk: &Obj, k: &str) -> String {
    so_us(tk.get(k))
}

/// "Sai phạm 37: giá thấp hơn 36, giá cuối sai 1".
pub fn dong_sai_pham(tk: &Obj) -> String {
    let ds: Vec<String> = tk
        .get("sai_pham")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|s| format!("{} {}", s.get("ten").map(py_str).unwrap_or_default(), s.get("so").map(py_str).unwrap_or_default())).collect())
        .unwrap_or_default();
    format!("Sai phạm {}: {}", tk_so(tk, "sai_pham_tong"), ds.join(", "))
}

/// Các dòng (≤ 64 ký tự) in số liệu máy chủ – dùng ở server, khach, mo_phong.
pub fn dong_thong_ke(d: &Obj) -> Vec<String> {
    let s = |k: &str| tk_so(d, k);
    vec![
        format!("Chế độ {}, trễ xử lý 0–{} ms", s("che_do"), fmt_g(lay_f64(d, "tre_ms").unwrap_or(0.0))),
        format!("Yêu cầu {}: thành công {}, từ chối {}", s("so_yc"), s("thanh_cong"), s("tu_choi")),
        format!("Xung đột {}, thử lại {}, hết lượt thử {}", s("xung_dot"), s("thu_lai"), s("het_lan_thu")),
        format!("Hàng đợi max {}, chờ khóa quá hạn {}", s("hang_doi_max"), s("khoa_cho_lau")),
        format!("Xử lý µs: p50 {} p95 {} p99 {}", s("p50_us"), s("p95_us"), s("p99_us")),
        format!("Thông lượng {} yc/s", s("thong_luong")),
        dong_sai_pham(d),
        s("tom_tat"),
    ]
}

pub const TIEU_DE_SO_SANH: &str = "  Chế độ       Thành công Sai phạm Xung đột  yc/s  p99 ms";

/// Một hàng của bảng so sánh 4 chế độ (bộ mô phỏng đo "Thành công", yc/s, p99; máy chủ báo sai phạm,
/// xung đột).
#[derive(Clone, Debug)]
pub struct HangSoSanh {
    pub che_do: String,
    pub thanh_cong: u64,
    pub sai_pham: i64,
    pub xung_dot: i64,
    pub yc_s: i64,
    pub p99_ms: Option<f64>,
    pub tu_choi: u64,
    pub tom_tat: String,
}

pub fn chu_p99(p: Option<f64>) -> String {
    p.map_or("–".into(), |v| format!("{v:.1}"))
}

/// Các dòng bảng so sánh.
pub fn bang_so_sanh(hang: &[HangSoSanh]) -> Vec<String> {
    let mut ds = vec![TIEU_DE_SO_SANH.to_string()];
    for h in hang {
        ds.push(format!("  {:<12} {:>10} {:>8} {:>8} {:>5} {:>7}", h.che_do, h.thanh_cong, h.sai_pham, h.xung_dot, h.yc_s, chu_p99(h.p99_ms)));
    }
    ds
}

#[cfg(test)]
mod kiem_thu {
    use super::*;

    #[test]
    fn dinh_dang() {
        assert_eq!(tien(5_300_000), "5.300.000 đ");
        assert_eq!(tien(90_000), "90.000 đ");
        assert_eq!(tien(0), "0 đ");
        assert_eq!(fmt_g(2.0), "2");
        assert_eq!(fmt_g(0.5), "0.5");
        assert_eq!(fmt_g(30.0), "30");
        assert_eq!(fmt_g(2.25), "2.25");
        assert_eq!(py_repr_f64(5.0), "5.0");
        assert_eq!(py_repr_f64(0.1), "0.1");
        assert_eq!(phan_vi(&(1..=1000).map(|x| x as f64).collect::<Vec<_>>(), 99.0), Some(990.0));
        assert_eq!(round_py(2.5), 2);
        assert_eq!(round_py(3.5), 4);
        assert!(ten_hop_le("Bình"));
        assert!(!ten_hop_le(" An"));
        assert!(!ten_hop_le(""));
    }
}
