//! Bài mở rộng MR6.10 – Phần NGHIỆP VỤ của bộ mô phỏng tải (khung ở mo_phong.rs):
//!   - NguCanh: danh sách mã lớp (từ cap_nhat đầu tiên);
//!   - NguoiAo: kịch bản của MỘT sinh viên ảo: muốn `lan` lớp (mặc định 1). Thứ tự ưu tiên các lớp xáo
//!     ngẫu nhiên theo tên (cùng tên -> cùng thứ tự); đăng ký lớp đầu, "lớp đã đủ" thì thử lớp kế tiếp;
//!     được nhận thì 10 % "đổi ý" hủy ngay lớp đó (nhả chỗ cho người khác). Sinh viên thứ 100, 200, …
//!     dùng TÊN của người liền trước (một sinh viên bấm trên hai thiết bị cùng lúc) – máy chủ phải từ
//!     chối lượt thứ hai ("da_dang_ky").
//!   - tham_so_dat_lai(): {} – giữ số lớp, sĩ số hiện tại của máy chủ.

use crate::chung::*;
use serde_json::{json, Value};

pub const SO_LAN_MAC_DINH: u32 = 1; // mỗi sinh viên muốn 1 lớp
pub const HAI_THIET_BI: usize = 100; // sinh viên thứ 100, 200, … dùng tên người liền trước
pub const TI_LE_HUY: f64 = 0.1; // được nhận rồi hủy ngay ("đổi ý")

/// Giữ số lớp, sĩ số hiện tại của máy chủ.
pub fn tham_so_dat_lai() -> Obj {
    Obj::new()
}

/// Danh sách mã lớp mà một kết nối biết.
pub struct NguCanh {
    pub ma_lop: Vec<String>,
}

impl NguCanh {
    pub fn moi(trang_thai: &Obj) -> NguCanh {
        let ma_lop = trang_thai.get("lop").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.get("ma").and_then(Value::as_str).map(str::to_string)).collect()).unwrap_or_default();
        NguCanh { ma_lop }
    }
    pub fn cap_nhat(&mut self, _tin: &Obj) {}
    pub fn nhan_kq(&mut self, _tin: &Obj) {}
}

/// Bộ sinh số giả ngẫu nhiên gieo theo tên (như random.Random(ten) của Python: cùng tên -> cùng dãy).
struct NgauNhienTheoTen(u64);

impl NgauNhienTheoTen {
    fn moi(ten: &str) -> Self {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a
        for b in ten.bytes() {
            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        NgauNhienTheoTen(h | 1)
    }
    fn so(&mut self) -> f64 {
        let mut x = self.0; // xorshift64*
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Xáo Fisher–Yates.
    fn xao<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = ((self.so() * (i + 1) as f64) as usize).min(i);
            v.swap(i, j);
        }
    }
}

/// Đăng ký đủ `lan` lớp theo thứ tự ưu tiên; lớp đủ thì thử lớp sau; 10 % hủy ngay lớp vừa được.
pub struct NguoiAo {
    ten: String,
    thu_tu: Vec<String>,
    can: u32,
    doi_y: bool,
    dang_huy: bool,
}

impl NguoiAo {
    pub fn moi(ten: &str, lan: u32) -> NguoiAo {
        let i: usize = ten[1..].parse().unwrap_or(1);
        let ten = if i % HAI_THIET_BI == 0 { format!("u{}", i - 1) } else { ten.to_string() };
        NguoiAo { ten, thu_tu: Vec::new(), can: lan, doi_y: false, dang_huy: false }
    }

    fn tiep(&mut self) -> Option<Obj> {
        if self.can == 0 || self.thu_tu.is_empty() {
            return None;
        }
        let ma = self.thu_tu.remove(0);
        Some(obj(json!({"loai": "dang_ky", "nguoi": self.ten, "ma_lop": ma})))
    }

    pub fn yeu_cau_dau(&mut self, nc: &NguCanh) -> Option<Obj> {
        let mut nn = NgauNhienTheoTen::moi(&self.ten); // cùng tên -> cùng thứ tự ưu tiên
        self.thu_tu = nc.ma_lop.clone();
        nn.xao(&mut self.thu_tu);
        self.doi_y = nn.so() < TI_LE_HUY;
        self.tiep()
    }

    pub fn sau_kq(&mut self, _nc: &NguCanh, tin: &Obj) -> Option<Obj> {
        if self.dang_huy {
            return None; // vừa hủy xong: thôi
        }
        if dung(tin, "ok") {
            if self.doi_y {
                self.dang_huy = true;
                return Some(obj(json!({"loai": "huy", "nguoi": self.ten, "ma_lop": tin.get("ma_lop").cloned().unwrap_or(Value::Null)})));
            }
            self.can = self.can.saturating_sub(1);
        }
        self.tiep()
    }
}
