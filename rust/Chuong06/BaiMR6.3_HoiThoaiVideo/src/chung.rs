//! Các thủ tục dùng chung cho server và client hội thoại video.
//!
//! Khung hình (frame) là ma trận điểm ảnh; mỗi điểm RGB 3 byte. Khung 160×120:
//!   - kích thước thô: 160 × 120 × 3 = 57 600 byte; ở 10 khung/giây (fps) cần
//!     57 600 × 8 × 10 ≈ 4,6 Mbit/s – quá lớn, nên phải nén trước khi gửi;
//!   - ở đây nén bằng zlib (không mất dữ liệu; crate miniz_oxide); khung tổng hợp có "hạt nhiễu" giống
//!     cảm biến camera nên nén còn khoảng 7 KB -> 10 fps × 7 KB × 8 ≈ 560 kbit/s;
//!   - giảm độ phân giải 1/2 mỗi chiều -> số điểm ảnh còn 1/4, dung lượng giảm tương ứng.
//! Một khung nén (vài KB) lớn hơn một gói UDP an toàn, nên được chia mảnh (fragmentation):
//! mỗi datagram ≤ 1200 byte (mức mà QUIC – RFC 9000 – coi là an toàn trên Internet), gồm header
//! 20 byte + tối đa 1180 byte dữ liệu. Bên nhận ghép đủ mảnh mới giải nén được; mất một mảnh là
//! hỏng cả khung (khung bị bỏ).

use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::LazyLock;
use std::time::Instant;

pub const RONG: usize = 160; // độ phân giải gốc của camera
pub const CAO: usize = 120;
pub const MAX_DATAGRAM: usize = 1200;
pub const HEADER_SIZE: usize = 20; // "!BBHIHHHHI" của bản Python
pub const DU_LIEU_TOI_DA: usize = MAX_DATAGRAM - HEADER_SIZE; // 1180 byte
const PHIEN_BAN: u8 = 1;
pub const LOAI_MANH: u8 = 1; // mảnh khung hình
pub const LOAI_GIU: u8 = 2; // gói đăng ký/giữ kết nối
const MANH_TOI_DA: u16 = 256; // tối đa 256 mảnh/khung (≈ 300 KB): chặn khung giả quá lớn
pub const MA_SERVER: u16 = 0;

/// Đồng hồ đơn điệu chung (giây) – thay cho time.monotonic() của Python.
pub static T0: LazyLock<Instant> = LazyLock::new(Instant::now);
pub fn bay_gio() -> f64 {
    T0.elapsed().as_secs_f64()
}

// ---------------------------------------------------------------- Chia mảnh và ghép khung
/// Header: phiên bản, loại, mã nguồn, số khung, chỉ số mảnh, tổng số mảnh, rộng, cao,
/// nhãn thời gian (ms). Trả về danh sách datagram.
fn header(loai: u8, ma: u16, so: u32, i: u16, tong: u16, w: u16, h: u16, ts: u32) -> Vec<u8> {
    let mut g = vec![PHIEN_BAN, loai];
    g.extend_from_slice(&ma.to_be_bytes());
    g.extend_from_slice(&so.to_be_bytes());
    for x in [i, tong, w, h] {
        g.extend_from_slice(&x.to_be_bytes());
    }
    g.extend_from_slice(&ts.to_be_bytes());
    g
}

pub fn chia_manh(ma: u16, so: u32, w: usize, h: usize, du_lieu: &[u8]) -> Vec<Vec<u8>> {
    let tong = du_lieu.len().div_ceil(DU_LIEU_TOI_DA).max(1);
    let ts = (bay_gio() * 1000.0) as u64 as u32;
    (0..tong)
        .map(|i| {
            let mut g = header(LOAI_MANH, ma, so, i as u16, tong as u16, w as u16, h as u16, ts);
            g.extend_from_slice(&du_lieu[(i * DU_LIEU_TOI_DA).min(du_lieu.len())..((i + 1) * DU_LIEU_TOI_DA).min(du_lieu.len())]);
            g
        })
        .collect()
}

pub fn goi_giu(ma: u16) -> Vec<u8> {
    header(LOAI_GIU, ma, 0, 0, 0, 0, 0, 0)
}

pub struct Manh<'a> {
    pub loai: u8,
    pub ma: u16,
    pub so: u32,
    pub i: u16,
    pub tong: u16,
    pub w: u16,
    pub h: u16,
    pub payload: &'a [u8],
}

/// Tách header; None nếu sai dạng.
pub fn tach_header(g: &[u8]) -> Option<Manh<'_>> {
    if g.len() < HEADER_SIZE {
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
        payload: &g[HEADER_SIZE..],
    };
    if g[0] != PHIEN_BAN || (m.loai == LOAI_MANH && (m.tong == 0 || m.tong > MANH_TOI_DA || m.i >= m.tong)) {
        return None;
    }
    Some(m)
}

struct Dang {
    tong: u16,
    manh: HashMap<u16, Vec<u8>>,
    t: f64,
    w: u16,
    h: u16,
}

/// Ghép các mảnh thành khung cho một luồng (một nguồn gửi).
///
/// Mảnh có thể đến lệch thứ tự hoặc mất. Giữ các khung đang ghép dở; khi một khung đủ mảnh:
/// trả về khung đó và bỏ mọi khung cũ hơn còn dở (video trực tiếp chỉ cần khung mới nhất).
/// Khung dở quá `cho` giây cũng bị bỏ. Đếm: mảnh nhận, khung đủ, khung hỏng (thiếu mảnh),
/// mảnh mất, khung mất hẳn (không nhận được mảnh nào), mảnh đến trễ.
pub struct GhepKhung {
    cho: f64,
    dang: BTreeMap<u32, Dang>, // số khung -> khung đang ghép
    cuoi: Option<u32>,         // số khung đã xong/bỏ gần nhất
    pub manh_nhan: u64,
    pub khung_du: u64,
    pub khung_hong: u64,
    pub manh_mat: u64,
    pub khung_mat_han: u64,
    pub tre: u64,
}

impl GhepKhung {
    pub fn new() -> GhepKhung {
        GhepKhung { cho: 0.5, dang: BTreeMap::new(), cuoi: None, manh_nhan: 0, khung_du: 0, khung_hong: 0, manh_mat: 0, khung_mat_han: 0, tre: 0 }
    }

    fn bo(&mut self, so: u32) {
        if let Some(k) = self.dang.remove(&so) {
            self.khung_hong += 1;
            self.manh_mat += (k.tong as usize - k.manh.len()) as u64;
        }
    }

    /// Thêm một mảnh; trả về (rộng, cao, dữ liệu nén) khi đủ mảnh.
    pub fn them(&mut self, m: &Manh, bay_gio: f64) -> Option<(u16, u16, Vec<u8>)> {
        self.manh_nhan += 1;
        if self.cuoi.is_some_and(|c| m.so <= c) {
            self.tre += 1; // thuộc khung đã xong hoặc đã bỏ
            return None;
        }
        let k = self.dang.entry(m.so).or_insert_with(|| Dang { tong: m.tong, manh: HashMap::new(), t: bay_gio, w: m.w, h: m.h });
        if m.i >= k.tong {
            return None; // mảnh mâu thuẫn với tổng số mảnh đã ghi nhận (gói giả/hỏng): bỏ
        }
        k.manh.insert(m.i, m.payload.to_vec());
        let (du, tong, w, h) = (k.manh.len() >= k.tong as usize, k.tong, k.w, k.h);
        let qua_lau: Vec<u32> = self.dang.iter().filter(|(s, d)| bay_gio - d.t > self.cho && **s != m.so).map(|(s, _)| *s).collect();
        for s in qua_lau {
            self.bo(s); // chờ quá lâu
        }
        if !du {
            return None;
        }
        let k = self.dang.remove(&m.so).unwrap();
        let du_lieu: Vec<u8> = (0..tong).flat_map(|j| k.manh[&j].clone()).collect();
        let cu: Vec<u32> = self.dang.keys().copied().filter(|s| *s < m.so).collect();
        for s in &cu {
            self.bo(*s); // khung cũ còn dở: bỏ
        }
        if let Some(c) = self.cuoi {
            self.khung_mat_han += (m.so as i64 - c as i64 - 1 - cu.len() as i64).max(0) as u64;
        }
        self.cuoi = Some(m.so);
        self.khung_du += 1;
        Some((w, h, du_lieu))
    }
}

// ---------------------------------------------------------------- Ảnh RGB thô (Vec<u8>)
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
        '=' => "00001F001F0000", ',' => "000000000C0408", ' ' => "00000000000000",
        _ => "0E110102040004", // '?'
    }
}

/// Tô hình chữ nhật (x, y, rw, rh) bằng màu (r, g, b).
pub fn to_mau(anh: &mut [u8], w: usize, x: usize, y: usize, rw: usize, rh: usize, mau: [u8; 3]) {
    let hang: Vec<u8> = mau.iter().copied().cycle().take(rw * 3).collect();
    for yy in y..y + rh {
        anh[(yy * w + x) * 3..(yy * w + x + rw) * 3].copy_from_slice(&hang);
    }
}

/// Vẽ chuỗi chữ (chỉ chữ in hoa không dấu, số, vài ký hiệu) bằng phông 5×7 phóng `co` lần.
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

/// Giảm độ phân giải d lần mỗi chiều: lấy 1 điểm trong mỗi khối d×d (lấy mẫu thưa).
pub fn thu_nho(anh: &[u8], w: usize, h: usize, d: usize) -> (Vec<u8>, usize, usize) {
    if d == 1 {
        return (anh.to_vec(), w, h);
    }
    let (w2, h2) = (w / d, h / d);
    let mut ra = Vec::with_capacity(w2 * h2 * 3);
    for y in 0..h2 {
        for x in 0..w2 {
            let i = (y * d * w + x * d) * 3;
            ra.extend_from_slice(&anh[i..i + 3]);
        }
    }
    (ra, w2, h2)
}

/// Phóng to k lần bằng lặp điểm gần nhất (nearest neighbour): ảnh độ phân giải thấp thành ô vuông.
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

/// Bộ sinh số giả ngẫu nhiên nhỏ (xorshift64*) – tạo hạt nhiễu và giả lập mất mảnh tất định.
pub struct Rng(pub u64);

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

// ---------------------------------------------------------------- Camera tổng hợp
const BANG_MAU: [[u8; 3]; 5] = [[230, 120, 60], [70, 170, 90], [70, 120, 220], [200, 80, 180], [200, 190, 60]];

/// Khung hình giả lập: nền sóng chéo màu riêng của từng client, có hạt nhiễu thưa như cảm biến thật,
/// trượt ngang theo thời gian (chuyển động); vẽ tên client và số khung.
pub struct CameraTongHop {
    ten: String,
    nen: Vec<u8>,
}

impl CameraTongHop {
    pub fn new(ten: &str, ma: u16) -> CameraTongHop {
        let mau = BANG_MAU[(ma as usize + BANG_MAU.len() - 1) % BANG_MAU.len()];
        let mut r = Rng::new(ma as u64);
        let mut nen = vec![0u8; RONG * CAO * 3];
        for y in 0..CAO {
            for x in 0..RONG {
                let k = 0.35 + 0.65 * (0.5 + 0.5 * (2.0 * std::f64::consts::PI * (x as f64 / 40.0 + y as f64 / 60.0)).sin());
                let hat = if r.random() < 0.04 { (r.random() * 24.0) as u32 } else { 0 }; // hạt nhiễu
                let i = (y * RONG + x) * 3;
                for c in 0..3 {
                    nen[i + c] = ((mau[c] as f64 * k) as u32 / 16 * 16 + hat).min(255) as u8;
                }
            }
        }
        CameraTongHop { ten: ten.to_string(), nen }
    }

    pub fn lay_khung(&self, so: u32) -> Vec<u8> {
        let dich = (so as usize * 2) % RONG * 3; // trượt 2 điểm ảnh mỗi khung
        let mut anh = Vec::with_capacity(self.nen.len());
        for y in 0..CAO {
            let hang = &self.nen[y * RONG * 3..(y + 1) * RONG * 3];
            anh.extend_from_slice(&hang[dich..]);
            anh.extend_from_slice(&hang[..dich]);
        }
        let n = self.ten.chars().count();
        to_mau(&mut anh, RONG, 6, 6, 6 * 4 * n + 6, 7 * 4 + 6, [20, 20, 20]);
        ve_chu(&mut anh, RONG, CAO, 9, 9, &self.ten, 4, [255, 255, 255]);
        let nhan = format!("#{so:04}");
        to_mau(&mut anh, RONG, 6, CAO - 34, 6 * 2 * nhan.len() + 4, 7 * 2 + 4, [20, 20, 20]);
        ve_chu(&mut anh, RONG, CAO, 8, CAO - 32, &nhan, 2, [255, 255, 255]);
        anh
    }
}

/// Nén zlib (RFC 1950). Mức 1 của miniz_oxide dùng đường nén "siêu nhanh" nên kém hơn mức 1 của
/// zlib C (khung thử: 12,2 KB so với 7,3 KB); bài dùng MUC_NHANH = 2 (8,1 KB) cho gần với bản Python.
pub const MUC_NHANH: u8 = 2;

pub fn nen(du_lieu: &[u8], muc: u8) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(du_lieu, muc)
}

/// Giải nén zlib có giới hạn kích thước đầu ra (chống "bom nén" từ client xấu).
pub fn giai_nen(du_lieu: &[u8]) -> Option<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(du_lieu, 4 << 20).ok()
}

// ---------------------------------------------------------------- PNG viết tay
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

/// PNG = chữ ký 8 byte + các khối (độ dài, kiểu, dữ liệu, CRC-32): IHDR, IDAT, IEND.
/// IDAT là zlib của các hàng ảnh, mỗi hàng bắt đầu bằng 1 byte bộ lọc (0 = không lọc).
pub fn ghi_png(duong_dan: &str, anh: &[u8], w: usize, h: usize) -> io::Result<()> {
    let khoi = |out: &mut Vec<u8>, kieu: &[u8], du_lieu: &[u8]| {
        out.extend_from_slice(&(du_lieu.len() as u32).to_be_bytes());
        let mut kd = kieu.to_vec();
        kd.extend_from_slice(du_lieu);
        out.extend_from_slice(&kd);
        out.extend_from_slice(&crc32(&kd).to_be_bytes());
    };
    let mut hang = Vec::with_capacity((w * 3 + 1) * h);
    for y in 0..h {
        hang.push(0);
        hang.extend_from_slice(&anh[y * w * 3..(y + 1) * w * 3]);
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

// ---------------------------------------------------------------- Kênh điều khiển JSON
/// Mỗi thông điệp điều khiển là một dòng JSON kết thúc bằng '\n'.
pub fn gui_json(mut s: &TcpStream, obj: &Value) -> io::Result<()> {
    s.write_all(format!("{obj}\n").as_bytes())
}

/// Socket UDP có bộ đệm nhận 1 MB (mảnh đến dồn dập) – std không đặt được SO_RCVBUF nên dùng socket2.
pub fn udp_dem_lon(dia_chi: std::net::SocketAddr) -> io::Result<std::net::UdpSocket> {
    use socket2::{Domain, Socket, Type};
    let s = Socket::new(Domain::for_address(dia_chi), Type::DGRAM, None)?;
    s.set_recv_buffer_size(1 << 20)?;
    s.bind(&dia_chi.into())?;
    Ok(s.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chia_roi_ghep_lai() {
        let du_lieu: Vec<u8> = (0..5000u32).map(|i| (i * 7) as u8).collect();
        let goi = chia_manh(3, 9, 160, 120, &du_lieu);
        assert_eq!(goi.len(), 5); // 5000 / 1180 -> 5 mảnh
        let mut g = GhepKhung::new();
        let mut kq = None;
        for p in goi.iter().rev() {
            kq = g.them(&tach_header(p).unwrap(), 0.0).or(kq);
        }
        assert_eq!(kq.unwrap().2, du_lieu);
    }

    #[test]
    fn crc_va_zlib() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082); // CRC của khối IEND trong mọi tệp PNG
        let x = vec![7u8; 10000];
        assert_eq!(giai_nen(&nen(&x, 1)).unwrap(), x);
    }
}

