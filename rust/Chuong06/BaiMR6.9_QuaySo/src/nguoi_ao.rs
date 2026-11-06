//! Bài mở rộng MR6.9 – Phần NGHIỆP VỤ của bộ mô phỏng tải (khung ở mo_phong.rs):
//!   - NguCanh: điều một kết nối biết về trạng thái chung (ở đây: số quà còn lại), cập nhật từ cap_nhat;
//!   - NguoiAo: kịch bản của MỘT người dùng ảo: quay `lan` lần (mặc định 1). Người thứ 100, 200, 300…
//!     dùng TÊN của người liền trước (một người quay trên hai thiết bị cùng lúc) – máy chủ phải từ chối
//!     lượt thứ hai ("da_quay"); lan > 1 thì các lượt sau cũng phải bị từ chối.
//!   - tham_so_dat_lai(): {} – giữ số quà và xác suất p hiện tại của máy chủ (đặt bằng server --p,
//!     khach dat_lai p=…, hoặc ô tham số của server_gui).

use crate::chung::*;

pub const SO_LAN_MAC_DINH: u32 = 1; // mỗi người quay 1 lần
pub const HAI_THIET_BI: usize = 100; // người thứ 100, 200, … quay bằng tên người liền trước

/// Giữ số quà, p hiện tại của máy chủ.
pub fn tham_so_dat_lai() -> Obj {
    Obj::new()
}

/// Số quà còn lại mà một kết nối biết (chỉ để tham khảo – máy chủ mới quyết định).
pub struct NguCanh {
    pub con_lai: i64,
}

impl NguCanh {
    pub fn moi(trang_thai: &Obj) -> NguCanh {
        NguCanh { con_lai: lay_i64(trang_thai, "con_lai").unwrap_or(0) }
    }

    pub fn cap_nhat(&mut self, tin: &Obj) {
        if let Some(c) = lay_i64(tin, "con_lai") {
            self.con_lai = c;
        }
    }

    pub fn nhan_kq(&mut self, _tin: &Obj) {}
}

/// Quay `lan` lần; người thứ 100, 200, … mang tên người liền trước (thiết bị thứ hai của người đó).
pub struct NguoiAo {
    ten: String,
    con: u32,
}

impl NguoiAo {
    pub fn moi(ten: &str, lan: u32) -> NguoiAo {
        let i: usize = ten[1..].parse().unwrap_or(1);
        let ten = if i % HAI_THIET_BI == 0 { format!("u{}", i - 1) } else { ten.to_string() };
        NguoiAo { ten, con: lan }
    }

    fn quay(&mut self) -> Option<Obj> {
        if self.con == 0 {
            return None;
        }
        self.con -= 1;
        Some(obj(serde_json::json!({"loai": "quay", "nguoi": self.ten})))
    }

    pub fn yeu_cau_dau(&mut self, _nc: &NguCanh) -> Option<Obj> {
        self.quay()
    }

    pub fn sau_kq(&mut self, _nc: &NguCanh, _tin: &Obj) -> Option<Obj> {
        self.quay()
    }
}
