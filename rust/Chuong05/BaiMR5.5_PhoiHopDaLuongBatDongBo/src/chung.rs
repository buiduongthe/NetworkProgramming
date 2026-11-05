//! Phần dùng chung: hằng số, việc CPU (băm), hàm "thư viện cũ" kiểu chặn, nhật ký.
//!
//! Giao thức (mỗi kết nối một yêu cầu) giống bản Python/Go:
//!   BAM <mã> <chế_độ> <số_byte> <số_vòng> <hạn_ms>\n + dữ liệu    chế_độ: chan | tach | gioihan
//!       → "OK <mã> <16 hex đầu SHA-256>" hoặc "HETGIO <mã>"
//!   CAMBIEN <mã>   → "CAMBIEN <mã> <giá trị> <tên luồng đã gửi kết quả qua oneshot>"
//!   DATLAI         → "OK";   THONGKE <nhãn> → "TK <trễ nhịp ms> <số mẫu> <trễ giao mẫu ms> <nhóm tính tối đa>"

use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const N_YC: usize = 16;
pub const SO_BYTE: usize = 64 * 1024;
pub const SO_VONG: usize = 400;
pub const MS_CU: u64 = 20;
pub const NHOM_TINH: usize = 2;
pub const GIOI_HAN: usize = 4;
pub const NHIP_MS: f64 = 10.0;
pub const CAM_BIEN_MS: u64 = 50;

static T0: LazyLock<Instant> = LazyLock::new(Instant::now);
static KHOA_IN: Mutex<()> = Mutex::new(());

pub fn bay_gio_ms() -> f64 {
    T0.elapsed().as_secs_f64() * 1000.0
}

/// Dữ liệu thử: byte thứ i = i mod 251 (giống mọi bản ngôn ngữ).
pub fn du_lieu(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 251) as u8).collect()
}

/// Tên luồng hiện tại; luồng chính chạy vòng lặp tokio → "vòng-lặp".
pub fn ten_luong() -> String {
    match thread::current().name() {
        Some("main") | None => "vòng-lặp".to_string(),
        Some(t) => t.to_string(),
    }
}

/// Việc nặng CPU: SHA-256 của dữ liệu lặp so_vong lần; kiểm tra cờ hủy trước mỗi vòng.
/// Trả về Ok(hex16) hoặc Err(vòng đã dừng) khi bị hủy.
pub fn bam(data: &[u8], so_vong: usize, huy: &AtomicBool) -> Result<String, usize> {
    let mut h = Sha256::new();
    for r in 0..so_vong {
        if huy.load(Ordering::Relaxed) {
            return Err(r);
        }
        h.update(data);
    }
    Ok(h.finalize()
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Giả lập hàm của thư viện CŨ chỉ có API chặn: giữ luồng gọi MS_CU ms. Trả về tên luồng đã chạy nó.
pub fn tra_cuu_cu() -> String {
    thread::sleep(Duration::from_millis(MS_CU));
    ten_luong()
}

pub fn log(msg: &str) {
    let _g = KHOA_IN.lock().unwrap();
    println!(
        "[{:7.3} s] {:<16}| {}",
        bay_gio_ms() / 1000.0,
        ten_luong(),
        msg
    );
}

pub fn say(msg: &str) {
    let _g = KHOA_IN.lock().unwrap();
    println!("{msg}");
}
