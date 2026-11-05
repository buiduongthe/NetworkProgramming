//! Các thủ tục dùng chung cho server và client hội thoại âm thanh.
//!
//! Âm thanh số PCM (Pulse-Code Modulation) 16 bit, mono, tần số lấy mẫu 16000 Hz:
//!   - lấy mẫu: mỗi giây đo biên độ sóng âm 16000 lần; mỗi mẫu là số nguyên 16 bit có dấu
//!     (-32768 … 32767). Theo định lý Nyquist, 16 kHz biểu diễn được âm tới 8 kHz – đủ cho giọng nói;
//!   - tốc độ bit: 16000 mẫu/s × 16 bit = 256000 bit/s = 256 kbit/s (32000 byte/s);
//!   - khung 20 ms: 16000 × 0,020 = 320 mẫu = 640 byte -> 50 gói mỗi giây;
//!   - mỗi gói thêm 12 byte RTP + 8 byte UDP + 20 byte IPv4 = 40 byte phụ phí
//!     -> 50 × 40 × 8 = 16 kbit/s, tổng ≈ 272 kbit/s mỗi chiều cho mỗi client.
//! Mức âm đo bằng dBFS (decibel so với toàn thang – full scale): 0 dBFS là mức RMS lớn nhất (32768).
//! Ví dụ sóng sin biên độ 0,25 toàn thang có RMS 0,25/√2 -> -15,1 dBFS; nhân 2 -> +6,02 dB.
//!
//! Làm tròn: Python round() làm tròn "nửa về số chẵn"; bản Rust dùng f64::round_ties_even() để mẫu
//! âm thanh trùng từng giá trị với bản Python.

use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::net::TcpStream;

pub const TAN_SO_MAU: usize = 16000; // mẫu/giây
pub const KHUNG_MS: u64 = 20; // độ dài một khung (gói) âm thanh
pub const MAU_MOI_KHUNG: usize = TAN_SO_MAU * KHUNG_MS as usize / 1000; // 320 mẫu
pub const BYTE_MOI_KHUNG: usize = MAU_MOI_KHUNG * 2; // 640 byte
pub const PT_AM_THANH: u8 = 96; // payload type động (RFC 3551) dùng cho L16/16000/mono
pub const PT_GIU_KET_NOI: u8 = 127; // gói rỗng: đăng ký địa chỉ UDP, giữ kết nối khi mic tắt
pub const SSRC_SERVER: u32 = 0; // mã nguồn của luồng trộn do server gửi

// ---------------------------------------------------------------- Gói RTP (RFC 3550)
/// Header RTP cố định 12 byte: V=2|P|X|CC (1 byte), M|PT (1 byte), số thứ tự (16 bit),
/// nhãn thời gian (32 bit, đơn vị: mẫu), SSRC (32 bit, ở đây là mã client).
pub fn dong_goi_rtp(pt: u8, seq: u32, ts: u32, ssrc: u32, payload: &[u8]) -> Vec<u8> {
    let mut g = vec![0x80, pt & 0x7F];
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
    pub ssrc: u32,
    pub payload: &'a [u8],
}

/// Tách gói RTP; None nếu gói sai dạng.
pub fn tach_rtp(g: &[u8]) -> Option<Rtp<'_>> {
    if g.len() < 12 || g[0] >> 6 != 2 {
        return None; // quá ngắn hoặc phiên bản RTP khác 2
    }
    Some(Rtp {
        pt: g[1] & 0x7F,
        seq: u16::from_be_bytes([g[2], g[3]]),
        ts: u32::from_be_bytes([g[4], g[5], g[6], g[7]]),
        ssrc: u32::from_be_bytes([g[8], g[9], g[10], g[11]]),
        payload: &g[12..],
    })
}

// ---------------------------------------------------------------- PCM <-> byte
/// RTP truyền L16 theo thứ tự byte mạng (big-endian) – RFC 3551 mục 4.5.11.
pub fn pcm_sang_byte(mau: &[i16]) -> Vec<u8> {
    mau.iter().flat_map(|x| x.to_be_bytes()).collect()
}

pub fn byte_sang_pcm(b: &[u8]) -> Vec<i16> {
    b.chunks_exact(2).map(|c| i16::from_be_bytes([c[0], c[1]])).collect()
}

/// Làm tròn kiểu Python rồi cắt ngọn (clipping): giá trị vượt phạm vi 16 bit bị giữ ở biên.
pub fn cat_ngon(x: f64) -> i16 {
    x.round_ties_even().clamp(-32768.0, 32767.0) as i16
}

// ---------------------------------------------------------------- Mức âm
/// Mức RMS (căn bậc hai của trung bình bình phương) tính theo dBFS.
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

/// Thanh đo mức dạng chữ: -60 dBFS (trống) … 0 dBFS (đầy).
pub fn thanh_muc(db: f64, rong: usize) -> String {
    let n = if db == f64::NEG_INFINITY { 0 } else { ((db + 60.0) / 60.0 * rong as f64).round_ties_even().clamp(0.0, rong as f64) as usize };
    format!("|{}{}|", "#".repeat(n), "-".repeat(rong - n))
}

pub fn chu_db(db: f64, rong: usize) -> String {
    if db < -90.0 {
        format!("{:>rong$}", "---")
    } else {
        format!("{db:rong$.1}")
    }
}

// ---------------------------------------------------------------- Bộ đệm jitter
/// Bộ đệm chống rung trễ (jitter buffer) cho một luồng RTP.
///
/// Gói UDP đến không đều (jitter), có thể mất hoặc đảo thứ tự. Bộ đệm giữ vài khung (do_sau) trước
/// khi bắt đầu phát, rồi cứ mỗi 20 ms lấy đúng khung có số thứ tự kế tiếp:
///   - có khung      -> phát;
///   - thiếu khung nhưng đã có khung sau -> coi như mất, phát khoảng lặng (che lỗi đơn giản);
///   - bộ đệm rỗng   -> cạn, chờ nạp lại do_sau khung;
///   - khung đến sau khi đã qua lượt phát -> trễ, bỏ.
/// Đồng thời thống kê theo RFC 3550: số gói mất (từ khoảng trống số thứ tự) và jitter
/// (trung bình trượt 1/16 của chênh lệch thời gian truyền giữa hai gói liên tiếp).
pub struct BoDemJitter {
    do_sau: usize,
    toi_da: usize,
    khung: BTreeMap<i64, Vec<u8>>, // số thứ tự mở rộng -> payload
    ke_tiep: i64,                  // số thứ tự sẽ phát tiếp
    dang_phat: bool,
    seq_lon_nhat: Option<i64>, // số thứ tự mở rộng (không quay vòng) lớn nhất đã nhận
    seq_dau: Option<i64>,
    pub nhan: u64,
    pub tre: u64,
    pub thieu: u64,
    pub can: u64,
    pub tran: u64,
    jitter: f64, // đơn vị: mẫu (1 ms = 16 mẫu)
    truyen_truoc: Option<f64>,
}

impl BoDemJitter {
    pub fn new(do_sau: usize) -> BoDemJitter {
        BoDemJitter {
            do_sau,
            toi_da: 50,
            khung: BTreeMap::new(),
            ke_tiep: 0,
            dang_phat: false,
            seq_lon_nhat: None,
            seq_dau: None,
            nhan: 0,
            tre: 0,
            thieu: 0,
            can: 0,
            tran: 0,
            jitter: 0.0,
            truyen_truoc: None,
        }
    }

    /// Số thứ tự 16 bit quay vòng sau 65535; đổi thành số nguyên tăng mãi.
    fn mo_rong(&self, seq: u16) -> i64 {
        match self.seq_lon_nhat {
            None => seq as i64,
            Some(lon) => {
                let lech = (seq as i64 - lon) & 0xFFFF;
                if lech < 0x8000 {
                    lon + lech
                } else {
                    lon - (0x10000 - lech)
                }
            }
        }
    }

    /// luc_den: thời điểm gói đến (giây, đồng hồ đơn điệu).
    pub fn nhan_goi(&mut self, seq: u16, ts: u32, payload: &[u8], luc_den: f64) {
        let s = self.mo_rong(seq);
        self.seq_dau.get_or_insert(s);
        if self.seq_lon_nhat.is_none_or(|l| s > l) {
            self.seq_lon_nhat = Some(s);
        }
        self.nhan += 1;
        let truyen = luc_den * TAN_SO_MAU as f64 - ts as f64; // thời gian truyền (lệch hằng số không quan trọng)
        if let Some(tr) = self.truyen_truoc {
            let d = (truyen - tr).abs();
            if d < TAN_SO_MAU as f64 {
                // bỏ qua bước nhảy lớn (nhãn thời gian quay vòng)
                self.jitter += (d - self.jitter) / 16.0;
            }
        }
        self.truyen_truoc = Some(truyen);
        if self.dang_phat && s < self.ke_tiep {
            self.tre += 1; // đến quá muộn, đã qua lượt phát
            return;
        }
        self.khung.insert(s, payload.to_vec());
        if self.khung.len() > self.toi_da {
            self.khung.pop_first(); // tràn: bỏ khung cũ nhất
            self.tran += 1;
        }
    }

    /// Gọi mỗi 20 ms; trả về payload của khung cần phát hoặc None (phát khoảng lặng).
    pub fn lay(&mut self) -> Option<Vec<u8>> {
        if !self.dang_phat {
            if self.khung.len() < self.do_sau {
                return None;
            }
            self.dang_phat = true;
            self.ke_tiep = *self.khung.keys().next().unwrap();
        }
        if let Some(p) = self.khung.remove(&self.ke_tiep) {
            self.ke_tiep += 1;
            return Some(p);
        }
        if self.khung.is_empty() {
            // cạn bộ đệm: chờ nạp lại
            self.dang_phat = false;
            self.can += 1;
            return None;
        }
        self.thieu += 1; // mất khung này, đã có khung sau
        self.ke_tiep += 1;
        None
    }

    pub fn xoa(&mut self) {
        self.khung.clear();
        self.dang_phat = false;
    }

    /// Số gói mất theo RFC 3550: số gói mong đợi (từ số thứ tự) trừ số gói đã nhận.
    pub fn mat(&self) -> i64 {
        match (self.seq_dau, self.seq_lon_nhat) {
            (Some(d), Some(l)) => (l - d + 1 - self.nhan as i64).max(0),
            _ => 0,
        }
    }

    pub fn jitter_ms(&self) -> f64 {
        self.jitter / (TAN_SO_MAU as f64 / 1000.0)
    }
}

// ---------------------------------------------------------------- Phân tích tần số
/// Biên độ của thành phần tần số `tan_so` trong một khối mẫu, bằng thuật toán Goertzel.
///
/// Goertzel tính một điểm của biến đổi Fourier rời rạc bằng một bộ lọc cộng hưởng bậc hai:
///     s[n] = x[n] + 2cos(ω)·s[n-1] - s[n-2],  ω = 2π·f/fs
/// sau N mẫu: |X|² = s1² + s2² - 2cos(ω)·s1·s2. Tín hiệu được nhân trước với cửa sổ Hann để
/// giảm rò phổ từ các âm khác; biên độ sin A ≈ 2|X| / Σw.
pub fn goertzel_bien_do(mau: &[i16], tan_so: f64) -> f64 {
    let n = mau.len();
    let w = 2.0 * std::f64::consts::PI * tan_so / TAN_SO_MAU as f64;
    let he_so = 2.0 * w.cos();
    let (mut s1, mut s2, mut tong_w) = (0.0f64, 0.0f64, 0.0f64);
    let k = 2.0 * std::f64::consts::PI / (n as f64 - 1.0);
    for (i, &x) in mau.iter().enumerate() {
        let cs = 0.5 - 0.5 * (k * i as f64).cos(); // cửa sổ Hann
        tong_w += cs;
        let s0 = x as f64 * cs + he_so * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    2.0 * (s1 * s1 + s2 * s2 - he_so * s1 * s2).max(0.0).sqrt() / tong_w
}

/// Mức RMS (dBFS) của một âm trong đoạn dài: chia khối 0,1 s (độ phân giải 10 Hz), đo biên độ
/// từng khối rồi lấy trung bình công suất.
pub fn muc_am_dbfs(mau: &[i16], tan_so: f64) -> f64 {
    let khoi = 1600;
    let ds: Vec<f64> = (0..(mau.len() + 1).saturating_sub(khoi)).step_by(khoi).map(|i| goertzel_bien_do(&mau[i..i + khoi], tan_so)).collect();
    if ds.is_empty() {
        return f64::NEG_INFINITY;
    }
    let rms = (ds.iter().map(|a| a * a).sum::<f64>() / ds.len() as f64).sqrt() / 2f64.sqrt(); // biên độ sin -> RMS
    if rms > 0.5 {
        20.0 * (rms / 32768.0).log10()
    } else {
        f64::NEG_INFINITY
    }
}

// ---------------------------------------------------------------- Tệp WAV
/// Ghi WAV (RIFF) PCM 16 bit mono 16 kHz. WAV lưu mẫu theo little-endian, khác thứ tự byte mạng của RTP.
/// Thư viện chuẩn Rust không có mô-đun wave, nên tự ghi tiêu đề 44 byte.
pub fn ghi_wav(duong_dan: &str, mau: &[i16]) -> io::Result<()> {
    let du_lieu = (mau.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + du_lieu as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + du_lieu).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); // độ dài khối fmt
    b.extend_from_slice(&1u16.to_le_bytes()); // 1 = PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // số kênh
    b.extend_from_slice(&(TAN_SO_MAU as u32).to_le_bytes());
    b.extend_from_slice(&(TAN_SO_MAU as u32 * 2).to_le_bytes()); // byte/giây
    b.extend_from_slice(&2u16.to_le_bytes()); // byte mỗi khung mẫu
    b.extend_from_slice(&16u16.to_le_bytes()); // bit mỗi mẫu
    b.extend_from_slice(b"data");
    b.extend_from_slice(&du_lieu.to_le_bytes());
    for x in mau {
        b.extend_from_slice(&x.to_le_bytes());
    }
    std::fs::write(duong_dan, b)
}

/// Đọc WAV PCM 16 bit mono 16000 Hz (duyệt các khối RIFF để tìm "fmt " và "data").
pub fn doc_wav(duong_dan: &str) -> io::Result<Vec<i16>> {
    let b = std::fs::read(duong_dan)?;
    let sai = || io::Error::new(io::ErrorKind::InvalidData, "cần WAV PCM 16 bit, mono, 16000 Hz");
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err(sai());
    }
    let (mut i, mut fmt_ok) = (12, false);
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let n = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        let body = &b[i + 8..(i + 8 + n).min(b.len())];
        if id == b"fmt " && body.len() >= 16 {
            let kenh = u16::from_le_bytes([body[2], body[3]]);
            let fs = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            let bit = u16::from_le_bytes([body[14], body[15]]);
            fmt_ok = kenh == 1 && fs == TAN_SO_MAU as u32 && bit == 16;
        } else if id == b"data" {
            if !fmt_ok {
                return Err(sai());
            }
            return Ok(body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect());
        }
        i += 8 + n + (n & 1); // khối có độ dài lẻ được đệm 1 byte
    }
    Err(sai())
}

// ---------------------------------------------------------------- Kênh điều khiển JSON
/// Mỗi thông điệp điều khiển là một dòng JSON kết thúc bằng '\n' (định khung theo dòng).
pub fn gui_json(mut s: &TcpStream, obj: &Value) -> io::Result<()> {
    s.write_all(format!("{obj}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_025_la_tru_15_1_dbfs() {
        let w = 2.0 * std::f64::consts::PI * 440.0 / 16000.0;
        let mau: Vec<i16> = (0..16000).map(|i| cat_ngon(0.25 * 32767.0 * (w * i as f64).sin())).collect();
        assert_eq!(format!("{:.1}", muc_dbfs(&mau)), "-15.1");
        assert_eq!(format!("{:.1}", muc_am_dbfs(&mau, 440.0)), "-15.1");
        assert_eq!(muc_am_dbfs(&mau, 880.0), f64::NEG_INFINITY);
    }

    #[test]
    fn rtp_va_bo_dem() {
        let g = dong_goi_rtp(PT_AM_THANH, 65535, 320, 7, &[1, 2]);
        let r = tach_rtp(&g).unwrap();
        assert_eq!((r.pt, r.seq, r.ts, r.ssrc, r.payload), (96, 65535, 320, 7, &[1u8, 2][..]));
        let mut b = BoDemJitter::new(2);
        b.nhan_goi(65535, 0, &[1], 0.0);
        b.nhan_goi(1, 640, &[3], 0.04); // seq 0 mất, quay vòng 65535 -> 1
        assert_eq!(b.lay(), Some(vec![1]));
        assert_eq!(b.lay(), None); // thiếu seq 0
        assert_eq!(b.lay(), Some(vec![3]));
        assert_eq!(b.mat(), 1);
    }
}
