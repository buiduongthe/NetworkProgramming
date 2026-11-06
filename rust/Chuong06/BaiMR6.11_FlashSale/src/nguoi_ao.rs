//! Bài mở rộng MR6.11 – Phần NGHIỆP VỤ của bộ mô phỏng tải (khung ở mo_phong.rs):
//!   - NguCanh: không cần gì (máy chủ quyết định tồn kho) – chỉ nhớ số tối đa mỗi đơn;
//!   - NguoiAo: kịch bản của MỘT người mua ảo: đặt `lan` đơn (mặc định 1), mỗi đơn 1–2 sản phẩm (ngẫu
//!     nhiên); "chỉ còn 1" thì đặt lại với số lượng còn lại; hết hàng thì thôi.
//!   - tham_so_dat_lai(): {"mo_sau_s": 0} – mở bán ngay lúc đặt lại: rào xuất phát chính là "giờ G",
//!     mọi người cùng bấm MUA NGAY đúng lúc mở.

use crate::chung::*;
use serde_json::{json, Value};

pub const SO_LAN_MAC_DINH: u32 = 1; // mỗi người đặt 1 đơn

/// Mô phỏng: mở bán ngay khi đặt lại (rào xuất phát = giờ G).
pub fn tham_so_dat_lai() -> Obj {
    obj(json!({"mo_sau_s": 0}))
}

/// Không cần trạng thái chung: tồn kho do máy chủ quyết định.
pub struct NguCanh {
    pub toi_da: i64,
}

impl NguCanh {
    pub fn moi(trang_thai: &Obj) -> NguCanh {
        NguCanh { toi_da: trang_thai.get("sp").and_then(|s| s.get("toi_da")).and_then(Value::as_i64).unwrap_or(2) }
    }
    pub fn cap_nhat(&mut self, _tin: &Obj) {}
    pub fn nhan_kq(&mut self, _tin: &Obj) {}
}

/// Đặt `lan` đơn, mỗi đơn 1–2 sản phẩm; "chỉ còn k" thì mua đúng k; hết hàng thì thôi.
pub struct NguoiAo {
    ten: String,
    con: u32,
    toi_da: i64,
}

impl NguoiAo {
    pub fn moi(ten: &str, lan: u32) -> NguoiAo {
        NguoiAo { ten: ten.to_string(), con: lan, toi_da: 2 }
    }

    fn mua(&mut self, sl: Option<i64>) -> Option<Obj> {
        if self.con == 0 {
            return None;
        }
        self.con -= 1;
        let sl = sl.filter(|&x| x > 0).unwrap_or_else(|| ngau_nhien_nguyen(1, self.toi_da));
        Some(obj(json!({"loai": "mua", "nguoi": self.ten, "so_luong": sl})))
    }

    pub fn yeu_cau_dau(&mut self, nc: &NguCanh) -> Option<Obj> {
        self.toi_da = nc.toi_da.clamp(1, 2);
        self.mua(None)
    }

    pub fn sau_kq(&mut self, _nc: &NguCanh, tin: &Obj) -> Option<Obj> {
        match lay_str(tin, "ma_loi") {
            Some("het_hang") => None,
            Some("khong_du") if lay_i64(tin, "ton").unwrap_or(0) > 0 => {
                self.con += 1; // đơn hỏng không tính: mua lại phần còn lại
                self.mua(lay_i64(tin, "ton"))
            }
            _ => self.mua(None),
        }
    }
}
