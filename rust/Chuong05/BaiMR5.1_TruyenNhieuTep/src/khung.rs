//! Khung giao thức dùng chung: định khung JSON, thùng token, băm SHA-256.
//!
//! Giao thức (một kết nối TCP cho mỗi lượt truyền):
//!   - Một khung điều khiển JSON có tiền tố độ dài 4 byte (big-endian) mở đầu.
//!   - Sau đó là luồng byte nhị phân của tệp (từ offset tới hết) để hỗ trợ tải tiếp (resume).
//! Chép gọn ý tưởng định khung từ Mã giả 5.1 (Bài 5.1) và băm SHA-256 từ Bài 3.9/MR7.1.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

pub const CHUNK: usize = 32 * 1024; // cỡ mỗi lần đọc/ghi
pub const KHONG_GIOI_HAN: f64 = (1u64 << 40) as f64; // "không giới hạn" tốc độ (1 TiB/s) – dùng cho mặc định

/// Gửi một khung điều khiển JSON: 4 byte độ dài + nội dung UTF-8.
pub fn gui_khung(mut s: &TcpStream, obj: &Value) -> io::Result<()> {
    let data = serde_json::to_vec(obj).unwrap();
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(&data);
    s.write_all(&out)
}

/// Đọc đúng n byte (lỗi UnexpectedEof nếu kết nối đóng giữa chừng).
pub fn doc_du(mut s: &TcpStream, n: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; n];
    s.read_exact(&mut buf)?;
    Ok(buf)
}

/// Nhận một khung điều khiển JSON.
pub fn nhan_khung(s: &TcpStream) -> io::Result<Value> {
    let h = doc_du(s, 4)?;
    let n = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as usize;
    serde_json::from_slice(&doc_du(s, n)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Băm SHA-256 của cả tệp (dạng chữ hex thường).
pub fn sha256_tep(path: &Path) -> io::Result<String> {
    let mut h = Sha256::new();
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(0))?;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

struct TrangThaiThung {
    rate: f64,
    suc_chua: f64,
    token: f64,
    moc: Instant,
}

/// Thùng token giới hạn tốc độ (byte/giây), đổ đầy liên tục; có thể đổi tốc độ lúc chạy.
/// Dùng chung cho mức phiên (session) và mức từng tệp. `tieu_thu(n)` chặn tới khi đủ n token.
pub struct ThungToken {
    khoa: Mutex<TrangThaiThung>,
}

impl TrangThaiThung {
    fn do_day(&mut self) {
        let now = Instant::now();
        self.token = self.suc_chua.min(self.token + (now - self.moc).as_secs_f64() * self.rate);
        self.moc = now;
    }
}

impl ThungToken {
    pub fn new(rate: f64) -> ThungToken {
        ThungToken {
            khoa: Mutex::new(TrangThaiThung {
                rate,
                suc_chua: rate,                         // cho phép bùng một giây
                token: rate.min((CHUNK * 4) as f64),    // bùng khởi động nhỏ -> tốc độ đo sát giới hạn
                moc: Instant::now(),
            }),
        }
    }

    pub fn rate(&self) -> f64 {
        self.khoa.lock().unwrap().rate
    }

    pub fn dat_rate(&self, rate: f64) {
        let mut t = self.khoa.lock().unwrap();
        t.do_day();
        t.rate = rate;
        t.suc_chua = rate; // sức chứa = 1 giây ở tốc độ mới
        // Giới hạn "bùng" còn 0,25 giây để nhiều tệp chia sẻ đều hơn, tốc độ đo sát giới hạn phiên.
        t.token = t.token.min(rate * 0.25);
    }

    /// Chặn tới khi lấy được n token. `dung_khi()` trả true thì bỏ dở (khi pause/stop).
    pub fn tieu_thu(&self, n: usize, dung_khi: impl Fn() -> bool) -> bool {
        while !dung_khi() {
            let thieu = {
                let mut t = self.khoa.lock().unwrap();
                t.do_day();
                if t.token >= n as f64 {
                    t.token -= n as f64;
                    return true;
                }
                (n as f64 - t.token) / t.rate
            };
            thread::sleep(Duration::from_secs_f64(thieu.min(0.05)));
        }
        false
    }
}

/// Phân tích chuỗi tốc độ: '512K', '1M', '2MiB', '1048576' -> số byte/giây.
pub fn doc_so_byte(s: &str) -> Option<f64> {
    let mut s = s.trim().to_uppercase().replace("IB", "").replace('B', "");
    let mut nhan = 1.0;
    for (hau_to, he_so) in [("K", 1024.0), ("M", 1048576.0), ("G", 1073741824.0)] {
        if let Some(r) = s.strip_suffix(hau_to) {
            nhan = he_so;
            s = r.to_string();
            break;
        }
    }
    s.parse::<f64>().ok().map(|v| (v * nhan).trunc())
}

/// Số byte -> chuỗi dễ đọc (KiB/MiB).
pub fn dinh_dang_byte(n: u64) -> String {
    for (don_vi, chia) in [("MiB", 1048576u64), ("KiB", 1024)] {
        if n >= chia {
            return format!("{:.2} {don_vi}", n as f64 / chia as f64);
        }
    }
    format!("{n} B")
}
