//! Bài mở rộng MR6.12 – Phần NGHIỆP VỤ của bộ mô phỏng tải (khung ở mo_phong.rs):
//!   - NguCanh: điều một kết nối biết về trạng thái chung (ở đây: danh sách mã tài khoản), cập nhật từ
//!     cap_nhat đầu tiên;
//!   - NguoiAo: kịch bản của MỘT người dùng ảo: yeu_cau_dau() -> yêu cầu đầu tiên; sau_kq(tin) -> yêu cầu
//!     kế tiếp hoặc None (xong). Yêu cầu là đối tượng {"loai": ..., các trường...} (khung tự thêm ma_yc).
//!   - tham_so_dat_lai(): tham số gửi kèm dat_lai trước mỗi chế độ trong so_sanh.
//!
//! Ở bài này mỗi người ảo chọn ngẫu nhiên MỘT cặp tài khoản (A, B) rồi chuyển qua lại A→B, B→A, A→B…
//! (`lan` lần, số tiền ngẫu nhiên 100.000–2.000.000 đ) – rất nhiều cặp chuyển chéo chạy cùng lúc.
//! (--dat-lai và --so-sanh gửi khoa_ngay_tho=0: khóa theo thứ tự mã, không khóa chết)

use crate::chung::*;
use serde_json::{json, Value};

pub const SO_LAN_MAC_DINH: u32 = 5; // mỗi người chuyển 5 lần
pub const MUC_TIEN: [i64; 6] = [100_000, 200_000, 300_000, 500_000, 1_000_000, 2_000_000];

/// So sánh 4 chế độ: khóa theo thứ tự mã tài khoản.
pub fn tham_so_dat_lai() -> Obj {
    obj(json!({"khoa_ngay_tho": 0}))
}

/// Danh sách mã tài khoản mà một kết nối biết (đọc từ cap_nhat đầu tiên).
pub struct NguCanh {
    pub ds: Vec<String>,
}

impl NguCanh {
    pub fn moi(trang_thai: &Obj) -> NguCanh {
        let ds = trang_thai.get("tai_khoan").and_then(Value::as_array).map(|a| a.iter().filter_map(|t| t.get("ma").and_then(Value::as_str).map(str::to_string)).collect()).unwrap_or_default();
        NguCanh { ds }
    }
    pub fn cap_nhat(&mut self, _tin: &Obj) {}
    pub fn nhan_kq(&mut self, _tin: &Obj) {}
}

/// Chuyển `lan` lần qua lại giữa một cặp tài khoản: A→B, B→A, A→B…
pub struct NguoiAo {
    ten: String,
    con: u32,
    a: String,
    b: String,
}

impl NguoiAo {
    pub fn moi(ten: &str, lan: u32) -> NguoiAo {
        NguoiAo { ten: ten.to_string(), con: lan, a: String::new(), b: String::new() }
    }

    fn chuyen(&mut self) -> Option<Obj> {
        if self.con == 0 {
            return None;
        }
        self.con -= 1;
        std::mem::swap(&mut self.a, &mut self.b); // đổi chiều mỗi lần
        Some(obj(json!({"loai": "chuyen", "nguoi": self.ten, "tu": self.a, "den": self.b, "so_tien": *chon_ngau_nhien(&MUC_TIEN)})))
    }

    pub fn yeu_cau_dau(&mut self, nc: &NguCanh) -> Option<Obj> {
        if nc.ds.len() < 2 {
            return None;
        }
        // random.sample(ds, 2): hai tài khoản khác nhau
        let i = ngau_nhien_nguyen(0, nc.ds.len() as i64 - 1) as usize;
        let mut j = ngau_nhien_nguyen(0, nc.ds.len() as i64 - 2) as usize;
        if j >= i {
            j += 1;
        }
        self.a = nc.ds[i].clone();
        self.b = nc.ds[j].clone();
        self.chuyen()
    }

    pub fn sau_kq(&mut self, _nc: &NguCanh, _tin: &Obj) -> Option<Obj> {
        self.chuyen()
    }
}
