//! Bài mở rộng MR6.5 – Các thủ tục dùng chung cho máy chủ, máy phát và người xem (phát trực tiếp 1→n).
//!
//! Tự chứa (không dùng mã của bài khác): chép và mở rộng phần cần thiết của MR6.2 (âm thanh RTP, bộ
//! đệm jitter) và MR6.3 (header video, chia mảnh, ghép khung, phông chữ 5×7, PNG viết tay).
//!
//! Hai loại datagram trên CÙNG một socket UDP, phân biệt bằng byte đầu tiên:
//!   - 0x80: gói RTP âm thanh (RFC 3550): V=2, P=0, X=0, CC=0 -> byte đầu luôn là 0x80;
//!   - 0x01: gói video của bài (phiên bản 1): loại 2 = giữ kết nối, 3 = mảnh khung I, 4 = mảnh khung P.
//!
//! Nén liên khung (inter-frame) đơn giản – ý tưởng của mọi codec video (H.264, VP8, AV1):
//!   - khung khóa I (intra): nén một mình  ->  I = zlib(rgb thô);
//!   - khung sai khác P (predicted): chỉ mã hóa phần THAY ĐỔI so với khung ngay trước
//!         P = zlib(XOR từng byte(khung hiện tại, khung trước))
//!     cảnh tĩnh -> XOR gần như toàn 0 -> nén còn vài trăm byte. Giải mã P cần đúng khung trước:
//!     mất một khung là hỏng cả chuỗi tới khung I kế tiếp (nhóm khung – GOP, Group Of Pictures).
//!
//! Làm tròn: Python round() làm tròn "nửa về số chẵn"; bản Rust dùng f64::round_ties_even() để mẫu
//! âm thanh giả lập trùng từng giá trị với bản Python.

use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------- Âm thanh (như MR6.2)
pub const TAN_SO_MAU: usize = 16000; // mẫu/giây
pub const KHUNG_MS: u64 = 20; // một gói âm thanh = 20 ms
pub const MAU_MOI_KHUNG: usize = TAN_SO_MAU * KHUNG_MS as usize / 1000; // 320 mẫu
pub const BYTE_MOI_KHUNG: usize = MAU_MOI_KHUNG * 2; // 640 byte (L16)
pub const PT_AM_THANH: u8 = 96; // payload type động (RFC 3551) cho L16/16000/mono
pub const RTP_SIZE: usize = 12; // "!BBHII": V|P|X|CC, M|PT, seq, ts, ssrc
pub const DAU_RTP: u8 = 0x80;

// ---------------------------------------------------------------- Video (như MR6.3)
pub const RONG: usize = 160;
pub const CAO: usize = 120;
pub const FPS_MAC_DINH: u32 = 10;
pub const GOP_MAC_DINH: u32 = 20;
pub const MAX_DATAGRAM: usize = 1200;
pub const HEADER_SIZE: usize = 20; // "!BBHIHHHHI" của bản Python
pub const DU_LIEU_TOI_DA: usize = MAX_DATAGRAM - HEADER_SIZE; // 1180 byte
pub const PHIEN_BAN: u8 = 1; // cũng là byte đầu của gói video (0x01)
pub const LOAI_GIU: u8 = 2;
pub const LOAI_I: u8 = 3;
pub const LOAI_P: u8 = 4;
pub const MANH_TOI_DA: u16 = 256;

// ---------------------------------------------------------------- Giới hạn, thời gian chờ
pub const GIOI_HAN_DONG: usize = 64 * 1024; // một dòng JSON điều khiển tối đa 64 KB
pub const BO_DEM_GOP_TOI_DA: usize = 4 << 20; // bộ đệm GOP ở server tối đa 4 MB
pub const PLI_SERVER_S: f64 = 0.5; // server chuyển xin_khung_khoa tối đa 1 lần / 500 ms
pub const PLI_XEM_S: f64 = 1.0; // người xem gửi xin_khung_khoa tối đa 1 lần / 1 s
pub const CHO_THAM_CHIEU_S: f64 = 0.2; // chờ khung thiếu trước khi coi là mất tham chiếu
pub const CHO_MANH_S: f64 = 0.5; // chờ đủ mảnh của một khung
pub const HANG_HIEN_THI: usize = 30; // hàng khung chờ hiển thị tối đa 30 khung
pub const DO_SAU_MAC_DINH: usize = 5; // bộ đệm jitter 5 khung = 100 ms

/// Đồng hồ đơn điệu chung (giây) – thay cho time.monotonic() của Python.
pub static T0: LazyLock<Instant> = LazyLock::new(Instant::now);
pub fn bay_gio() -> f64 {
    T0.elapsed().as_secs_f64()
}

/// Giờ thực (ms Unix) – thay cho time.time() * 1000 của Python.
pub fn gio_unix_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

pub fn ngu(giay: f64) {
    if giay > 0.0 {
        std::thread::sleep(Duration::from_secs_f64(giay));
    }
}

// ================================================================ RTP âm thanh
/// Header RTP 12 byte: 0x80 (V=2), PT=96 (M=0), số thứ tự, nhãn thời gian (mẫu), SSRC.
pub fn dong_goi_rtp(seq: u32, ts: u32, ssrc: u32, payload: &[u8]) -> Vec<u8> {
    let mut g = Vec::with_capacity(RTP_SIZE + payload.len());
    g.extend_from_slice(&[DAU_RTP, PT_AM_THANH]);
    g.extend_from_slice(&(seq as u16).to_be_bytes());
    g.extend_from_slice(&ts.to_be_bytes());
    g.extend_from_slice(&ssrc.to_be_bytes());
    g.extend_from_slice(payload);
    g
}

pub struct Rtp<'a> {
    pub pt: u8,
    pub seq: u16,
    pub ts: u32,
    #[allow(dead_code)]
    pub ssrc: u32,
    pub payload: &'a [u8],
}

/// Tách gói RTP; None nếu sai dạng.
pub fn tach_rtp(g: &[u8]) -> Option<Rtp<'_>> {
    if g.len() < RTP_SIZE || g[0] >> 6 != 2 {
        return None;
    }
    Some(Rtp {
        pt: g[1] & 0x7F,
        seq: u16::from_be_bytes([g[2], g[3]]),
        ts: u32::from_be_bytes([g[4], g[5], g[6], g[7]]),
        ssrc: u32::from_be_bytes([g[8], g[9], g[10], g[11]]),
        payload: &g[RTP_SIZE..],
    })
}

/// L16 truyền theo thứ tự byte mạng (big-endian) – RFC 3551 mục 4.5.11.
pub fn pcm_sang_byte(mau: &[i16]) -> Vec<u8> {
    mau.iter().flat_map(|x| x.to_be_bytes()).collect()
}

pub fn byte_sang_pcm(b: &[u8]) -> Vec<i16> {
    b.chunks_exact(2).map(|c| i16::from_be_bytes([c[0], c[1]])).collect()
}

/// Mức RMS theo dBFS (0 dBFS = toàn thang 32768).
pub fn muc_dbfs(mau: &[i16]) -> f64 {
    if mau.is_empty() {
        return f64::NEG_INFINITY;
    }
    let rms = (mau.iter().map(|&x| x as f64 * x as f64).sum::<f64>() / mau.len() as f64).sqrt();
    if rms > 0.0 {
        20.0 * (rms / 32768.0).log10()
    } else {
        f64::NEG_INFINITY
    }
}

/// Bộ đệm chống rung trễ (jitter buffer) cho luồng RTP – như MR6.2, thêm nhãn thời gian.
///
/// Giữ `do_sau` khung trước khi bắt đầu phát, rồi cứ 20 ms lấy đúng khung có số thứ tự kế tiếp:
///   - có khung  -> trả về (ts, Some(payload));
///   - thiếu khung nhưng đã có khung sau -> mất: trả về (ts ước lượng, None) – phát im lặng,
///     đồng hồ phát vẫn tiến 20 ms;
///   - rỗng -> cạn: trả về None (đồng hồ phát dừng), chờ nạp lại `do_sau` khung.
pub struct BoDemJitter {
    pub do_sau: usize,
    toi_da: usize,
    khung: BTreeMap<i64, (u32, Vec<u8>)>, // số thứ tự mở rộng -> (ts, payload)
    ke_tiep: i64,
    dang_phat: bool,
    seq_lon_nhat: Option<i64>,
    seq_dau: Option<i64>,
    ts_cuoi: u32,
    pub nhan: u64,
    pub tre: u64,
    pub thieu: u64,
    pub can: u64,
    pub tran: u64,
}

impl BoDemJitter {
    pub fn new(do_sau: usize) -> BoDemJitter {
        BoDemJitter {
            do_sau,
            toi_da: 60,
            khung: BTreeMap::new(),
            ke_tiep: 0,
            dang_phat: false,
            seq_lon_nhat: None,
            seq_dau: None,
            ts_cuoi: 0,
            nhan: 0,
            tre: 0,
            thieu: 0,
            can: 0,
            tran: 0,
        }
    }

    /// Số thứ tự 16 bit quay vòng -> số nguyên tăng mãi.
    fn mo_rong(&self, seq: u16) -> i64 {
        let Some(lon) = self.seq_lon_nhat else { return seq as i64 };
        let lech = (seq as i64 - lon).rem_euclid(0x10000);
        if lech < 0x8000 {
            lon + lech
        } else {
            lon - (0x10000 - lech)
        }
    }

    pub fn nhan_goi(&mut self, seq: u16, ts: u32, payload: &[u8]) {
        let s = self.mo_rong(seq);
        if self.seq_dau.is_none() {
            self.seq_dau = Some(s);
        }
        if self.seq_lon_nhat.is_none_or(|l| s > l) {
            self.seq_lon_nhat = Some(s);
        }
        self.nhan += 1;
        if self.dang_phat && s < self.ke_tiep {
            self.tre += 1; // đến sau lượt phát của nó
            return;
        }
        self.khung.insert(s, (ts, payload.to_vec()));
        if self.khung.len() > self.toi_da {
            self.khung.pop_first();
            self.tran += 1;
        }
    }

    /// Gọi mỗi 20 ms. Trả về (ts, payload|None) hoặc None (đang chờ nạp).
    pub fn lay(&mut self) -> Option<(u32, Option<Vec<u8>>)> {
        if !self.dang_phat {
            if self.khung.len() < self.do_sau {
                return None;
            }
            self.dang_phat = true;
            self.ke_tiep = *self.khung.keys().next().unwrap();
        }
        if let Some((ts, p)) = self.khung.remove(&self.ke_tiep) {
            self.ke_tiep += 1;
            self.ts_cuoi = ts;
            return Some((ts, Some(p)));
        }
        if self.khung.is_empty() {
            self.dang_phat = false;
            self.can += 1;
            return None;
        }
        self.thieu += 1; // mất khung này
        self.ke_tiep += 1;
        self.ts_cuoi = self.ts_cuoi.wrapping_add(MAU_MOI_KHUNG as u32);
        Some((self.ts_cuoi, None))
    }

    /// Bỏ mọi khung đang giữ (khi tắt nhận âm thanh); lần sau nạp lại từ đầu.
    pub fn xoa(&mut self) {
        self.khung.clear();
        self.dang_phat = false;
    }

    /// Đổi độ sâu khi đang chạy: tăng -> dừng phát để nạp thêm; giảm -> bỏ khung cũ nhất.
    pub fn dat_do_sau(&mut self, n: usize) {
        self.do_sau = n;
        if self.khung.len() < n {
            self.dang_phat = false;
        }
        while self.khung.len() > n {
            let (s, _) = self.khung.pop_first().unwrap();
            if self.dang_phat {
                self.ke_tiep = s + 1;
            }
        }
    }

    /// Số gói mất theo RFC 3550: số gói mong đợi trừ số gói đã nhận.
    pub fn mat(&self) -> u64 {
        match (self.seq_dau, self.seq_lon_nhat) {
            (Some(d), Some(l)) => (l - d + 1 - self.nhan as i64).max(0) as u64,
            _ => 0,
        }
    }
}

// ================================================================ Gói video
fn header(loai: u8, ma: u16, so: u32, i: u16, tong: u16, w: u16, h: u16, ts: u32) -> Vec<u8> {
    let mut g = Vec::with_capacity(MAX_DATAGRAM);
    g.extend_from_slice(&[PHIEN_BAN, loai]);
    g.extend_from_slice(&ma.to_be_bytes());
    g.extend_from_slice(&so.to_be_bytes());
    for x in [i, tong, w, h] {
        g.extend_from_slice(&x.to_be_bytes());
    }
    g.extend_from_slice(&ts.to_be_bytes());
    g
}

/// Chia dữ liệu một khung (I hoặc P) thành datagram ≤ 1200 byte.
/// Header: phiên bản 1, loại, mã nguồn, số khung, chỉ số mảnh, tổng số mảnh, rộng, cao, nhãn ms.
pub fn chia_manh(loai: u8, ma: u16, so: u32, rong: usize, cao: usize, ts_ms: u64, du_lieu: &[u8]) -> Vec<Vec<u8>> {
    let tong = du_lieu.len().div_ceil(DU_LIEU_TOI_DA).max(1);
    assert!(tong <= MANH_TOI_DA as usize, "khung quá lớn");
    (0..tong)
        .map(|i| {
            let mut g = header(loai, ma, so, i as u16, tong as u16, rong as u16, cao as u16, ts_ms as u32);
            g.extend_from_slice(&du_lieu[(i * DU_LIEU_TOI_DA).min(du_lieu.len())..((i + 1) * DU_LIEU_TOI_DA).min(du_lieu.len())]);
            g
        })
        .collect()
}

/// Gói giữ kết nối / đăng ký địa chỉ UDP: loại 2, các trường khác 0.
pub fn goi_giu(ma: u32) -> Vec<u8> {
    header(LOAI_GIU, ma as u16, 0, 0, 0, 0, 0, 0)
}

/// Một mảnh video đã tách header.
pub struct Manh<'a> {
    pub loai: u8,
    pub ma: u16,
    pub so: u32,
    pub i: u16,
    pub tong: u16,
    pub w: u16,
    pub h: u16,
    pub ts: u32,
    pub payload: &'a [u8],
}

/// Tách header; None nếu sai dạng.
pub fn tach_header(g: &[u8]) -> Option<Manh<'_>> {
    if g.len() < HEADER_SIZE || g.len() > MAX_DATAGRAM {
        return None;
    }
    let u16_at = |o: usize| u16::from_be_bytes([g[o], g[o + 1]]);
    let m = Manh {
        loai: g[1],
        ma: u16_at(2),
        so: u32::from_be_bytes([g[4], g[5], g[6], g[7]]),
        i: u16_at(8),
        tong: u16_at(10),
        w: u16_at(12),
        h: u16_at(14),
        ts: u32::from_be_bytes([g[16], g[17], g[18], g[19]]),
        payload: &g[HEADER_SIZE..],
    };
    if g[0] != PHIEN_BAN || !matches!(m.loai, LOAI_GIU | LOAI_I | LOAI_P) {
        return None;
    }
    if m.loai != LOAI_GIU && (m.tong == 0 || m.tong > MANH_TOI_DA || m.i >= m.tong || m.w == 0 || m.h == 0) {
        return None;
    }
    Some(m)
}

/// Một khung vừa đủ mảnh (dữ liệu nén của I hoặc P).
pub struct KhungDu {
    pub loai: u8,
    pub so: u32,
    pub ts: u32,
    pub w: u16,
    pub h: u16,
    pub du_lieu: Vec<u8>,
}

struct Dang {
    dau: (u8, u16, u16, u16, u32), // (loại, tổng, rộng, cao, ts) của mảnh đầu tiên
    manh: HashMap<u16, Vec<u8>>,
    t: f64,
}

/// Ghép mảnh thành khung (I hoặc P). Khác MR6.3: KHÔNG bỏ khung cũ còn dở khi khung mới đủ,
/// vì khung P cần mọi khung trước nó – khung dở chỉ bị bỏ khi chờ quá `cho` giây.
pub struct GhepKhung {
    cho: f64,
    dang: HashMap<u32, Dang>,
    xong: HashSet<u32>, // khung đã đủ hoặc đã bỏ (mảnh đến sau = trễ)
    lon_nhat: i64,
    pub manh_nhan: u64,
    pub khung_du: u64,
    pub khung_hong: u64,
    pub manh_mat: u64,
    pub tre: u64,
}

impl GhepKhung {
    pub fn new() -> GhepKhung {
        GhepKhung { cho: CHO_MANH_S, dang: HashMap::new(), xong: HashSet::new(), lon_nhat: -1, manh_nhan: 0, khung_du: 0, khung_hong: 0, manh_mat: 0, tre: 0 }
    }

    pub fn don_dep(&mut self, bay_gio: f64, tru: Option<u32>) {
        let qua: Vec<u32> = self.dang.iter().filter(|(s, d)| bay_gio - d.t > self.cho && Some(**s) != tru).map(|(s, _)| *s).collect();
        for s in qua {
            let d = self.dang.remove(&s).unwrap();
            self.khung_hong += 1;
            self.manh_mat += (d.dau.1 as usize).saturating_sub(d.manh.len()) as u64;
            self.xong.insert(s);
        }
    }

    /// Bỏ các khung đang ghép dở mà không tính là hỏng (khi tắt nhận video).
    pub fn bo_dang_do(&mut self) {
        self.xong.extend(self.dang.keys().copied());
        self.dang.clear();
    }

    /// Trả về khung khi vừa đủ mảnh, ngược lại None.
    pub fn them(&mut self, m: &Manh, bay_gio: f64) -> Option<KhungDu> {
        self.manh_nhan += 1;
        if self.xong.contains(&m.so) || (m.so as i64) < self.lon_nhat - 1000 {
            self.tre += 1;
            return None;
        }
        let dau = (m.loai, m.tong, m.w, m.h, m.ts);
        let k = self.dang.entry(m.so).or_insert_with(|| Dang { dau, manh: HashMap::new(), t: bay_gio });
        if k.dau != dau {
            return None; // mâu thuẫn với mảnh trước: bỏ mảnh
        }
        k.manh.insert(m.i, m.payload.to_vec());
        let du = k.manh.len() >= m.tong as usize;
        self.don_dep(bay_gio, Some(m.so));
        if !du {
            return None;
        }
        let k = self.dang.remove(&m.so).unwrap();
        self.xong.insert(m.so);
        self.lon_nhat = self.lon_nhat.max(m.so as i64);
        if self.xong.len() > 3000 {
            let lon = self.lon_nhat;
            self.xong.retain(|&s| s as i64 > lon - 1000);
        }
        self.khung_du += 1;
        let du_lieu: Vec<u8> = (0..m.tong).flat_map(|j| k.manh[&j].iter().copied()).collect();
        Some(KhungDu { loai: m.loai, so: m.so, ts: m.ts, w: m.w, h: m.h, du_lieu })
    }
}

// ================================================================ Mã hóa / giải mã I, P
/// XOR từng byte hai khung cùng kích thước.
pub fn xor_khung(a: &[u8], b: &[u8]) -> Vec<u8> {
    a.iter().zip(b).map(|(x, y)| x ^ y).collect()
}

/// Giải nén zlib có giới hạn kích thước đầu ra (chống "bom nén").
pub fn giai_nen(du_lieu: &[u8]) -> Option<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(du_lieu, 4 << 20).ok()
}

/// Nén zlib (RFC 1950) mức 6 như zlib.compress(…, 6) của bản Python.
pub fn nen(du_lieu: &[u8], muc: u8) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(du_lieu, muc)
}

pub fn ma_hoa_i(anh: &[u8]) -> Vec<u8> {
    nen(anh, 6)
}

pub fn ma_hoa_p(anh: &[u8], truoc: &[u8]) -> Vec<u8> {
    nen(&xor_khung(anh, truoc), 6)
}

/// Một khung đã giải mã (ảnh RGB thô).
#[derive(Clone)]
pub struct KhungGiai {
    pub so: u32,
    pub loai: u8,
    pub ts: u32,
    pub w: u16,
    pub h: u16,
    pub anh: std::sync::Arc<Vec<u8>>,
}

/// Giải mã chuỗi I/P theo thứ tự số khung.
///
/// - I: giải nén -> thành khung tham chiếu (bỏ mọi khung cũ hơn đang chờ).
/// - P số k: giải được khi tham chiếu là k−1. Nếu khung k đủ mảnh mà tham chiếu < k−1: giữ chờ
///   khung thiếu tối đa CHO_THAM_CHIEU_S; quá hạn -> MẤT THAM CHIẾU: đếm đứng hình, bỏ mọi P tới I
///   kế tiếp, kiem_tra() trả true để người xem gửi xin_khung_khoa.
/// - Chưa từng có tham chiếu (vừa vào xem): chỉ chờ I, không tính đứng hình.
pub struct GiaiMaVideo {
    cho: f64,
    pub tham_chieu: Option<(u32, std::sync::Arc<Vec<u8>>, u16, u16)>, // (số khung, ảnh, rộng, cao)
    pub mat_tham_chieu: bool,
    pub luc_mat: Option<f64>,
    pub doi: BTreeMap<u32, (u32, u16, u16, Vec<u8>, f64)>, // số khung P -> (ts, w, h, dữ liệu, lúc đủ mảnh)
    pub so_i: u64,
    pub so_p: u64,
    pub bo_p: u64,
    pub dung_hinh: u64,
    pub loi: u64,
}

impl GiaiMaVideo {
    pub fn new() -> GiaiMaVideo {
        GiaiMaVideo { cho: CHO_THAM_CHIEU_S, tham_chieu: None, mat_tham_chieu: false, luc_mat: None, doi: BTreeMap::new(), so_i: 0, so_p: 0, bo_p: 0, dung_hinh: 0, loi: 0 }
    }

    /// Trả về danh sách khung vừa giải được.
    pub fn them(&mut self, k: KhungDu, bay_gio: f64) -> Vec<KhungGiai> {
        let mut ra = Vec::new();
        if k.loai == LOAI_I {
            if self.tham_chieu.as_ref().is_some_and(|t| k.so <= t.0) {
                return ra; // I cũ hơn tham chiếu hiện tại
            }
            let Some(anh) = giai_nen(&k.du_lieu) else {
                self.loi += 1;
                return ra;
            };
            if anh.len() != k.w as usize * k.h as usize * 3 {
                self.loi += 1;
                return ra;
            }
            let anh = std::sync::Arc::new(anh);
            self.tham_chieu = Some((k.so, anh.clone(), k.w, k.h));
            self.mat_tham_chieu = false;
            self.doi.retain(|&s, _| s > k.so);
            self.so_i += 1;
            ra.push(KhungGiai { so: k.so, loai: LOAI_I, ts: k.ts, w: k.w, h: k.h, anh });
            self.giai_tiep(&mut ra);
            return ra;
        }
        if self.tham_chieu.is_none() || self.mat_tham_chieu {
            self.bo_p += 1; // chưa có / mất tham chiếu: chờ I
            return ra;
        }
        if k.so <= self.tham_chieu.as_ref().unwrap().0 {
            return ra;
        }
        self.doi.insert(k.so, (k.ts, k.w, k.h, k.du_lieu, bay_gio));
        self.giai_tiep(&mut ra);
        ra
    }

    fn giai_tiep(&mut self, ra: &mut Vec<KhungGiai>) {
        while let Some((so_tc, truoc, tw, th)) = self.tham_chieu.clone() {
            let so = so_tc.wrapping_add(1);
            let Some((ts, w, h, du_lieu, _)) = self.doi.remove(&so) else { break };
            let x = giai_nen(&du_lieu).unwrap_or_default();
            if x.len() != truoc.len() || (w, h) != (tw, th) {
                self.loi += 1; // P hỏng: coi như thiếu khung này
                return;
            }
            let anh = std::sync::Arc::new(xor_khung(&x, &truoc));
            self.tham_chieu = Some((so, anh.clone(), w, h));
            self.so_p += 1;
            ra.push(KhungGiai { so, loai: LOAI_P, ts, w, h, anh });
        }
    }

    /// true nếu vừa mất tham chiếu (khung thiếu không tới sau CHO_THAM_CHIEU_S).
    pub fn kiem_tra(&mut self, bay_gio: f64) -> bool {
        if self.doi.is_empty() {
            return false;
        }
        let som_nhat = self.doi.values().map(|v| v.4).fold(f64::INFINITY, f64::min);
        if bay_gio - som_nhat < self.cho {
            return false;
        }
        self.mat_tham_chieu = true;
        self.luc_mat = Some(bay_gio);
        self.dung_hinh += 1;
        self.bo_p += self.doi.len() as u64;
        self.doi.clear();
        true
    }
}

// ================================================================ Ảnh RGB, phông 5×7, PNG
/// Phông chữ bitmap 5×7: mỗi ký tự 7 hàng, mỗi hàng 5 bit (bit cao = điểm bên trái), viết hex.
fn phong(c: char) -> &'static str {
    match c {
        'A' => "0E11111F111111", 'B' => "1E11111E11111E", 'C' => "0E11101010110E", 'D' => "1E11111111111E",
        'E' => "1F10101E10101F", 'F' => "1F10101E101010", 'G' => "0E11101711110F", 'H' => "1111111F111111",
        'I' => "0E04040404040E", 'J' => "0702020202120C", 'K' => "11121418141211", 'L' => "1010101010101F",
        'M' => "111B1515111111", 'N' => "11111915131111", 'O' => "0E11111111110E", 'P' => "1E11111E101010",
        'Q' => "0E11111115120D", 'R' => "1E11111E141211", 'S' => "0F10100E01011E", 'T' => "1F040404040404",
        'U' => "1111111111110E", 'V' => "11111111110A04", 'W' => "1111111515150A", 'X' => "11110A040A1111",
        'Y' => "1111110A040404", 'Z' => "1F01020408101F", '0' => "0E11131519110E", '1' => "040C040404040E",
        '2' => "0E11010204081F", '3' => "1F02040201110E", '4' => "02060A121F0202", '5' => "1F101E0101110E",
        '6' => "0608101E11110E", '7' => "1F010204080808", '8' => "0E11110E11110E", '9' => "0E11110F01020C",
        '#' => "0A0A1F0A1F0A0A", '/' => "01010204081010", ':' => "000C0C000C0C00", ';' => "000C0C000C0408",
        '.' => "00000000000C0C", '-' => "0000001F000000", '(' => "02040808080402", ')' => "08040202020408",
        '=' => "00001F001F0000", ',' => "000000000C0408", '+' => "0004041F040400", ' ' => "00000000000000",
        _ => "0E110102040004", // '?'
    }
}

/// Tô hình chữ nhật (x, y, rw, rh) bằng màu (r, g, b).
pub fn to_mau(anh: &mut [u8], w: usize, x: usize, y: usize, rw: usize, rh: usize, mau: [u8; 3]) {
    let rw = rw.min(w.saturating_sub(x));
    let hang: Vec<u8> = mau.iter().copied().cycle().take(rw * 3).collect();
    for yy in y..y + rh {
        let dau = (yy * w + x) * 3;
        if dau + rw * 3 > anh.len() {
            break;
        }
        anh[dau..dau + rw * 3].copy_from_slice(&hang);
    }
}

/// Vẽ chuỗi (chữ in hoa không dấu, số, vài ký hiệu) bằng phông 5×7 phóng `co` lần.
pub fn ve_chu(anh: &mut [u8], w: usize, h: usize, mut x: usize, y: usize, chu: &str, co: usize, mau: [u8; 3]) {
    for c in chu.chars().flat_map(char::to_uppercase) {
        let ma = phong(c);
        for hang in 0..7 {
            let bits = u8::from_str_radix(&ma[2 * hang..2 * hang + 2], 16).unwrap();
            for cot in 0..5 {
                if bits & (0x10 >> cot) != 0 {
                    let (px, py) = (x + cot * co, y + hang * co);
                    if px + co <= w && py + co <= h {
                        to_mau(anh, w, px, py, co, co, mau);
                    }
                }
            }
        }
        x += 6 * co;
    }
}

/// Phóng to k lần bằng lặp điểm gần nhất.
pub fn phong_to(anh: &[u8], w: usize, h: usize, k: usize) -> Vec<u8> {
    if k == 1 {
        return anh.to_vec();
    }
    let mut ra = Vec::with_capacity(anh.len() * k * k);
    for y in 0..h {
        let mut rong = Vec::with_capacity(w * k * 3);
        for x in 0..w {
            for _ in 0..k {
                rong.extend_from_slice(&anh[(y * w + x) * 3..(y * w + x) * 3 + 3]);
            }
        }
        for _ in 0..k {
            ra.extend_from_slice(&rong);
        }
    }
    ra
}

/// CRC-32 (đa thức 0xEDB88320, như zlib.crc32) dùng cho các khối PNG.
fn crc32(du_lieu: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in du_lieu {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// PNG viết tay: chữ ký 8 byte + khối IHDR, IDAT (zlib các hàng), IEND; mỗi khối CRC-32.
/// Mỗi hàng chọn một bộ lọc PNG (0 None, 1 Sub, 2 Up, 4 Paeth) cho tổng |byte| nhỏ nhất – cách chọn
/// thường dùng của libpng; ảnh chụp cửa sổ nhỏ đi vài lần so với chỉ dùng bộ lọc 0.
pub fn ghi_png(duong_dan: &str, anh: &[u8], w: usize, h: usize) -> io::Result<()> {
    let khoi = |out: &mut Vec<u8>, kieu: &[u8], du_lieu: &[u8]| {
        out.extend_from_slice(&(du_lieu.len() as u32).to_be_bytes());
        let mut kd = kieu.to_vec();
        kd.extend_from_slice(du_lieu);
        out.extend_from_slice(&kd);
        out.extend_from_slice(&crc32(&kd).to_be_bytes());
    };
    let n = w * 3;
    let mut hang = Vec::with_capacity((n + 1) * h);
    let khong = vec![0u8; n];
    for y in 0..h {
        let cur = &anh[y * n..(y + 1) * n];
        let tren = if y > 0 { &anh[(y - 1) * n..y * n] } else { &khong[..] };
        let mut tot: Option<(u64, u8, Vec<u8>)> = None;
        for loc in [0u8, 1, 2, 4] {
            let v: Vec<u8> = (0..n)
                .map(|i| {
                    let a = if i >= 3 { cur[i - 3] } else { 0 };
                    let b = tren[i];
                    let c = if i >= 3 { tren[i - 3] } else { 0 };
                    let du_doan = match loc {
                        0 => 0,
                        1 => a,
                        2 => b,
                        _ => {
                            let p = a as i16 + b as i16 - c as i16;
                            let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
                            if pa <= pb && pa <= pc {
                                a
                            } else if pb <= pc {
                                b
                            } else {
                                c
                            }
                        }
                    };
                    cur[i].wrapping_sub(du_doan)
                })
                .collect();
            let diem: u64 = v.iter().map(|&x| (x as i8).unsigned_abs() as u64).sum();
            if tot.as_ref().is_none_or(|t| diem < t.0) {
                tot = Some((diem, loc, v));
            }
        }
        let (_, loc, v) = tot.unwrap();
        hang.push(loc);
        hang.extend_from_slice(&v);
    }
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bit/kênh, kiểu màu 2 = RGB
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    khoi(&mut out, b"IHDR", &ihdr);
    khoi(&mut out, b"IDAT", &nen(&hang, 9));
    khoi(&mut out, b"IEND", &[]);
    std::fs::write(duong_dan, out)
}

/// Ghép các ảnh (rộng, cao, rgb, nhãn) thành một PNG: `cot` cột, nhãn dưới mỗi ảnh.
pub fn ghep_anh(duong_dan: &str, ds: &[(usize, usize, Vec<u8>, String)], cot: usize) -> io::Result<(usize, usize)> {
    let o_w = ds.iter().map(|d| d.0).max().unwrap_or(0) + 16;
    let cao_hang: Vec<usize> = ds.chunks(cot).map(|c| c.iter().map(|d| d.1).max().unwrap_or(0) + 30).collect();
    let (ww, hh) = (cot.min(ds.len()) * o_w + 16, cao_hang.iter().sum::<usize>() + 8);
    let mut anh = vec![0xFFu8; ww * hh * 3];
    for (n, (w, h, du_lieu, nhan)) in ds.iter().enumerate() {
        let x0 = 16 + (n % cot) * o_w;
        let y0 = 8 + cao_hang[..n / cot].iter().sum::<usize>();
        for y in 0..*h {
            let d = ((y0 + y) * ww + x0) * 3;
            anh[d..d + w * 3].copy_from_slice(&du_lieu[y * w * 3..(y + 1) * w * 3]);
        }
        let nhan: String = nhan.chars().take(o_w / 12).collect();
        ve_chu(&mut anh, ww, hh, x0, y0 + h + 8, &nhan, 2, [0, 0, 0]);
    }
    ghi_png(duong_dan, &anh, ww, hh)?;
    Ok((ww, hh))
}

/// Bộ sinh số giả ngẫu nhiên nhỏ (xorshift64*) – hạt nhiễu và giả lập mất gói tất định
/// (thay cho random.Random của Python; dãy số khác nhưng cùng phân bố đều).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn random(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ================================================================ Nguồn giả lập "bảng clapper"
const BANG_MAU: [[u8; 3]; 5] = [[230, 120, 60], [70, 170, 90], [70, 120, 220], [200, 80, 180], [200, 190, 60]];

/// Camera giả lập kiểu bảng clapper của nhà quay phim: nền tĩnh (sóng chéo + hạt nhiễu thưa như
/// MR6.3), một ô vuông chạy ngang (chuyển động nhỏ), đồng hồ phương tiện T=12.345 và số khung.
/// Ở mỗi giây tròn (ts % 1000 < 1000/fps) cả khung CHỚP TRẮNG (giữ mờ vân nền) – cùng lúc tiếng bíp
/// của NguonAmClapper: nhìn hình, nghe tiếng là thấy lệch tiếng-hình.
pub struct NguonClapper {
    ten: String,
    pub fps: u32,
    nen: Vec<u8>,
    nen_chop: Vec<u8>,
}

impl NguonClapper {
    pub fn new(ten: &str, ma: u32, fps: u32) -> NguonClapper {
        let mau = BANG_MAU[(ma.max(1) as usize - 1) % BANG_MAU.len()];
        let mut r = Rng::new(ma as u64);
        let mut nen = vec![0u8; RONG * CAO * 3];
        for y in 0..CAO {
            for x in 0..RONG {
                let k = 0.35 + 0.65 * (0.5 + 0.5 * (2.0 * std::f64::consts::PI * (x as f64 / 40.0 + y as f64 / 60.0)).sin());
                let hat = if r.random() < 0.04 { (r.random() * 24.0) as u32 } else { 0 };
                let i = (y * RONG + x) * 3;
                for c in 0..3 {
                    nen[i + c] = ((mau[c] as f64 * k) as u32 / 16 * 16 + hat).min(255) as u8;
                }
            }
        }
        // Khung chớp gần như trắng nhưng còn vân nền: khung I rơi vào lúc chớp vẫn có kích thước
        // như khung I thường (bảng tra đổi từng byte: v -> 255 − (255 − v)/4).
        let bang: Vec<u8> = (0..=255u32).map(|v| (255 - (255 - v) / 4) as u8).collect();
        let nen_chop = nen.iter().map(|&v| bang[v as usize]).collect();
        NguonClapper { ten: ten.to_string(), fps, nen, nen_chop }
    }

    pub fn lay_khung(&self, so: u32, ts_ms: u64) -> Vec<u8> {
        let chop = ((ts_ms % 1000) as f64) < 1000.0 / self.fps as f64;
        let x = 6 + (ts_ms / 10) as usize % (RONG - 24); // ô vuông chạy 100 điểm ảnh/giây
        let (mut anh, mau_chu, nen_chu) = if chop {
            (self.nen_chop.clone(), [0, 0, 0], [255, 255, 255]) // chớp trắng
        } else {
            let mut a = self.nen.clone();
            to_mau(&mut a, RONG, x, 52, 12, 12, [255, 230, 0]);
            (a, [255, 255, 255], [20, 20, 20])
        };
        let dong_ho = format!("T={}.{:03}", ts_ms / 1000, ts_ms % 1000);
        to_mau(&mut anh, RONG, 4, 4, 6 * 2 * dong_ho.len() + 4, 18, nen_chu);
        ve_chu(&mut anh, RONG, CAO, 6, 6, &dong_ho, 2, mau_chu);
        let nhan = format!("#{so:04}");
        to_mau(&mut anh, RONG, 4, CAO - 22, 6 * 2 * nhan.len() + 4, 18, nen_chu);
        ve_chu(&mut anh, RONG, CAO, 6, CAO - 20, &nhan, 2, mau_chu);
        to_mau(&mut anh, RONG, RONG - 30, CAO - 15, 27, 11, [200, 0, 0]);
        ve_chu(&mut anh, RONG, CAO, RONG - 28, CAO - 13, "LIVE", 1, [255, 255, 255]);
        let ten3: String = self.ten.chars().take(3).collect();
        let x_ten = RONG.saturating_sub(6 * 2 * ten3.chars().count() + 4);
        ve_chu(&mut anh, RONG, CAO, x_ten, 6, &ten3, 2, mau_chu);
        anh
    }
}

/// Micro giả lập: âm 440 Hz biên độ thấp liên tục + tiếng "bíp" 1000 Hz dài 100 ms ở mỗi giây
/// tròn của đồng hồ phương tiện (mẫu thứ n: n % 16000 < 1600). Mẫu đầu tiên = lúc bấm Phát.
pub struct NguonAmClapper {
    n: u64,
    w: f64,
    a: f64,
    wb: f64,
    ab: f64,
}

impl NguonAmClapper {
    pub fn new() -> NguonAmClapper {
        let (tan_so, bien_do, tan_so_bip, bien_do_bip) = (440.0, 0.08, 1000.0, 0.4);
        let tsm = TAN_SO_MAU as f64;
        NguonAmClapper { n: 0, w: 2.0 * std::f64::consts::PI * tan_so / tsm, a: bien_do * 32767.0, wb: 2.0 * std::f64::consts::PI * tan_so_bip / tsm, ab: bien_do_bip * 32767.0 }
    }

    pub fn lay_khung(&mut self) -> Vec<i16> {
        let ra = (self.n..self.n + MAU_MOI_KHUNG as u64)
            .map(|n| {
                let mut x = self.a * (self.w * n as f64).sin();
                if n % (TAN_SO_MAU as u64) < (TAN_SO_MAU as u64) / 10 {
                    x += self.ab * (self.wb * n as f64).sin();
                }
                x.round_ties_even() as i16
            })
            .collect();
        self.n += MAU_MOI_KHUNG as u64;
        ra
    }
}

// ================================================================ Kênh điều khiển JSON theo dòng
/// Mỗi thông điệp là một dòng JSON UTF-8 kết thúc '\n' (serde_json giữ nguyên chữ có dấu – như
/// json.dumps(ensure_ascii=False) của Python).
pub fn gui_json(mut s: &TcpStream, obj: &Value) -> io::Result<()> {
    s.write_all(format!("{obj}\n").as_bytes())
}

/// Đọc một dòng JSON. Ok(None) khi kết nối đóng; dòng > 64 KB, bị cắt hoặc sai dạng -> lỗi InvalidData.
pub fn doc_dong<R: Read>(r: &mut BufReader<R>) -> io::Result<Option<Map<String, Value>>> {
    let mut dong = Vec::new();
    let n = r.by_ref().take(GIOI_HAN_DONG as u64 + 1).read_until(b'\n', &mut dong)?;
    if n == 0 {
        return Ok(None);
    }
    let sai = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
    if dong.len() > GIOI_HAN_DONG || !dong.ends_with(b"\n") {
        return Err(sai("dòng điều khiển quá dài hoặc bị cắt"));
    }
    match serde_json::from_slice::<Value>(&dong) {
        Ok(Value::Object(m)) => Ok(Some(m)),
        Ok(_) => Err(sai("thông điệp không phải đối tượng JSON")),
        Err(e) => Err(sai(&e.to_string())),
    }
}

/// Lấy trường chuỗi của thông điệp ("" nếu thiếu).
pub fn chuoi<'a>(tin: &'a Map<String, Value>, k: &str) -> &'a str {
    tin.get(k).and_then(Value::as_str).unwrap_or("")
}

/// In một trường như Python in giá trị (số giữ nguyên, thiếu -> None).
pub fn hien_gia_tri(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "None".into(),
        Some(Value::String(s)) => s.clone(),
        Some(x) => x.to_string(),
    }
}

/// Giá trị "đúng/sai" kiểu bool() của Python.
pub fn la_dung(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

pub fn ten_hop_le(ten: &str) -> bool {
    let n = ten.chars().count();
    (1..=16).contains(&n) && ten.trim() == ten && ten.chars().all(|c| c == ' ' || !(c.is_control() || c.is_whitespace()))
}

/// Đọc lệnh từ stdin ở luồng riêng; dòng "@t lệnh" = chờ tới giây thứ t kể từ t0 (đồng hồ đơn điệu).
pub fn doc_lenh_stdin(t0: f64, xu_ly: impl Fn(&str) + Send + 'static) {
    std::thread::spawn(move || {
        for dong in io::stdin().lock().lines().map_while(Result::ok) {
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
    });
}

/// Số ms để in: "–" nếu chưa có; dau = true in dấu +/−.
pub fn chu_ms(x: Option<f64>, dau: bool) -> String {
    match x {
        None => "–".into(),
        Some(v) if dau => format!("{v:+.0}"),
        Some(v) => format!("{v:.0}"),
    }
}

/// Socket UDP có bộ đệm gửi/nhận 1 MB (std không đặt được SO_RCVBUF/SO_SNDBUF nên dùng socket2).
pub fn udp_dem_lon(dia_chi: std::net::SocketAddr) -> io::Result<std::net::UdpSocket> {
    use socket2::{Domain, Socket, Type};
    let s = Socket::new(Domain::for_address(dia_chi), Type::DGRAM, None)?;
    let _ = s.set_recv_buffer_size(1 << 20);
    let _ = s.set_send_buffer_size(1 << 20);
    s.bind(&dia_chi.into())?;
    Ok(s.into())
}

/// Kết nối TCP có thời gian chờ 5 s (như socket.create_connection(timeout=5)).
pub fn ket_noi_tcp(host: &str, cong: u16) -> io::Result<TcpStream> {
    use std::net::ToSocketAddrs;
    let mut loi = io::Error::new(io::ErrorKind::NotFound, format!("không phân giải được {host}"));
    for a in (host, cong).to_socket_addrs()? {
        match TcpStream::connect_timeout(&a, Duration::from_secs(5)) {
            Ok(s) => {
                s.set_nodelay(true)?;
                return Ok(s);
            }
            Err(e) => loi = e,
        }
    }
    Err(loi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chia_roi_ghep_lai_va_giai_ma_ip() {
        let a: Vec<u8> = (0..RONG * CAO * 3).map(|i| (i * 7 % 251) as u8).collect();
        let mut b = a.clone();
        b[100] ^= 0x55;
        let (gi, gp) = (chia_manh(LOAI_I, 3, 0, RONG, CAO, 0, &ma_hoa_i(&a)), chia_manh(LOAI_P, 3, 1, RONG, CAO, 100, &ma_hoa_p(&b, &a)));
        let mut g = GhepKhung::new();
        let mut gm = GiaiMaVideo::new();
        let mut ra = Vec::new();
        for p in gp.iter().chain(gi.iter().rev()) {
            if let Some(k) = g.them(&tach_header(p).unwrap(), 0.0) {
                ra.extend(gm.them(k, 0.0));
            }
        }
        // P tới trước I: P bị bỏ (chưa có tham chiếu), sau đó I giải được
        assert_eq!(ra.len(), 1);
        assert_eq!(*ra[0].anh, a);
        assert_eq!(gm.bo_p, 1);
    }

    #[test]
    fn jitter_va_rtp() {
        let mut j = BoDemJitter::new(2);
        for s in [0u16, 1, 3] {
            j.nhan_goi(s, s as u32 * 320, &[0; 640]);
        }
        assert_eq!(j.lay().unwrap().0, 0);
        assert_eq!(j.lay().unwrap().0, 320);
        let (ts, p) = j.lay().unwrap();
        assert!(p.is_none() && ts == 640); // mất gói 2
        assert_eq!(j.mat(), 1);
        let g = dong_goi_rtp(7, 640, 5, &[1, 2]);
        assert_eq!(&g[..4], &[0x80, 96, 0, 7]);
    }

    #[test]
    fn crc_va_am_clapper() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        let mut a = NguonAmClapper::new();
        let k = a.lay_khung();
        assert_eq!(k.len(), 320);
        assert!(muc_dbfs(&k) > -20.0); // đang bíp ở giây 0
    }
}
