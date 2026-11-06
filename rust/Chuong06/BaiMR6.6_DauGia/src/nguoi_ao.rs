//! Bài mở rộng MR6.6 – Phần NGHIỆP VỤ của bộ mô phỏng tải (khung ở mo_phong.rs):
//!   - NguCanh: điều một kết nối biết về trạng thái chung (ở đây: giá hiện tại, bước giá), cập nhật từ
//!     cap_nhat và từ trả lời kq;
//!   - NguoiAo: kịch bản của MỘT người dùng ảo: yeu_cau_dau() -> yêu cầu đầu tiên; sau_kq(tin) -> yêu cầu
//!     kế tiếp hoặc None (xong). Yêu cầu là đối tượng {"loai": ..., các trường...} (khung tự thêm ma_yc).
//!   - tham_so_dat_lai(): tham số gửi kèm dat_lai trước mỗi chế độ trong so_sanh.

use crate::chung::*;
use serde_json::{json, Value};

pub const SO_LAN_MAC_DINH: u32 = 5; // mỗi người trả giá 5 lần

/// Mô phỏng: phiên không giới hạn thời gian.
pub fn tham_so_dat_lai() -> Obj {
    obj(json!({"thoi_gian_s": 0}))
}

/// Giá hiện tại mà một kết nối biết – đọc từ cap_nhat và từ kq (có thể đã cũ khi tới máy chủ).
pub struct NguCanh {
    gia: i64,
    buoc: i64,
}

impl NguCanh {
    pub fn moi(trang_thai: &Obj) -> NguCanh {
        let buoc = trang_thai.get("mon").and_then(|m| m.get("buoc_gia")).and_then(Value::as_i64).unwrap_or(100_000);
        NguCanh { gia: lay_i64(trang_thai, "gia").unwrap_or(0), buoc }
    }

    pub fn cap_nhat(&mut self, tin: &Obj) {
        if let Some(g) = lay_i64(tin, "gia") {
            self.gia = g;
        }
    }

    pub fn nhan_kq(&mut self, tin: &Obj) {
        if let Some(g) = lay_i64(tin, "gia") {
            self.gia = g;
        }
    }
}

/// Trả giá `lan` lần; mỗi lần: giá hiện tại đã biết + bước giá × ngẫu nhiên 1–3.
pub struct NguoiAo {
    ten: String,
    con: u32,
}

impl NguoiAo {
    pub fn moi(ten: &str, lan: u32) -> NguoiAo {
        NguoiAo { ten: ten.to_string(), con: lan }
    }

    fn tra(&mut self, nc: &NguCanh) -> Option<Obj> {
        if self.con == 0 {
            return None;
        }
        self.con -= 1;
        Some(obj(json!({"loai": "tra_gia", "nguoi": self.ten, "gia": nc.gia + nc.buoc * ngau_nhien_nguyen(1, 3)})))
    }

    pub fn yeu_cau_dau(&mut self, nc: &NguCanh) -> Option<Obj> {
        self.tra(nc)
    }

    pub fn sau_kq(&mut self, nc: &NguCanh, _tin: &Obj) -> Option<Obj> {
        self.tra(nc)
    }
}
