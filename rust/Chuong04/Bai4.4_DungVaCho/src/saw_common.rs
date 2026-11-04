//! Hằng số và tiện ích dùng chung cho bên gửi, bên nhận dừng và chờ (Mã giả 4.6, 4.7).

use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::File;
use std::hash::{BuildHasher, RandomState};
use std::io::{self, Read};

// Byte loại trong tiêu đề 5 byte (Mã giả 4.6, dòng 1)
pub const DATA: u8 = 0;
pub const ACK: u8 = 1;
pub const FIN: u8 = 2;
pub const MAX_TRIES: u32 = 8;
pub const MAX_DGRAM: usize = 1200;
pub const CHUNK: usize = MAX_DGRAM - 5; // khối dữ liệu ≤ 1195 byte
// ms. RTO ban đầu và RTO tối thiểu chọn cho mạng nội bộ (Mục 4.4.2), không phải 1 s của TCP;
// MAX_RTO = 60 000 ms là trần RFC 6298 cho phép (Mã giả 4.6)
pub const RTO_INIT: f64 = 200.0;
pub const RTO_MIN: f64 = 100.0;
pub const MAX_RTO: f64 = 60000.0;

/// Tiêu đề 5 byte: UINT32_BE(seq) + BYTE(type).
pub fn header(seq: u32, typ: u8) -> Vec<u8> {
    let mut h = seq.to_be_bytes().to_vec();
    h.push(typ);
    h
}

/// Số ngẫu nhiên trong [0, 1). Thư viện chuẩn Rust chưa có hàm số ngẫu nhiên; mỗi RandomState mới
/// mang khóa ngẫu nhiên do hệ điều hành cấp, nên giá trị băm của nó dùng được cho mô phỏng (không dùng cho mật mã).
fn random_unit() -> f64 {
    let x = RandomState::new().hash_one(0u8);
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// Mô phỏng mất gói ở chiều gửi đi (thêm cho bài thực hành, không có trong mã giả).
/// spec = ""      : không mất gói;
/// spec = "0.1"   : mỗi datagram gửi đi mất ngẫu nhiên với xác suất 0,1;
/// spec = "3,7"   : mất đúng datagram gửi đi thứ 3 và thứ 7 (đếm từ 1, kể cả lần gửi lại).
pub struct LossSim {
    pub spec: String,
    count: u64,
    rate: f64,
    picks: HashSet<u64>,
}

impl LossSim {
    pub fn new(spec: &str) -> LossSim {
        let mut sim = LossSim { spec: spec.to_string(), count: 0, rate: 0.0, picks: HashSet::new() };
        if spec.contains('.') {
            sim.rate = spec.parse().unwrap_or(0.0);
        } else {
            sim.picks = spec.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        }
        sim
    }

    /// Có "làm mất" datagram gửi đi lần này không.
    pub fn drop_next(&mut self) -> bool {
        self.count += 1;
        self.picks.contains(&self.count) || random_unit() < self.rate
    }

    pub fn describe(&self) -> &str {
        if self.spec.is_empty() {
            "không"
        } else {
            &self.spec
        }
    }
}

/// SHA-256 của một tệp, dạng hex chữ thường.
pub fn sha256_file(path: &str) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
