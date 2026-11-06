//! Bài mở rộng MR6.7 – Phần NGHIỆP VỤ của bộ mô phỏng tải (khung ở mo_phong.rs):
//!   - NguCanh: điều một kết nối biết về trạng thái chung (ở đây: số xe, số xe còn trống), cập nhật từ
//!     cap_nhat và từ trả lời kq;
//!   - NguoiAo: kịch bản của MỘT người dùng ảo: yeu_cau_dau() -> yêu cầu đầu tiên; sau_kq(tin) -> yêu cầu
//!     kế tiếp hoặc None (xong). Yêu cầu là đối tượng {"loai": ..., các trường...} (khung tự thêm ma_yc).
//!   - tham_so_dat_lai(): tham số gửi kèm dat_lai trước mỗi chế độ trong so_sanh ({} = giữ tham số hiện
//!     tại của máy chủ, ví dụ số xe đã đặt bằng "khach … dat_lai so_xe=10").

use crate::chung::*;
use crate::may_chu::{json_diem, nga_tu};
use serde_json::json;

pub const SO_LAN_MAC_DINH: u32 = 1; // mỗi người bấm "Đặt xe" một lần

/// Giữ số xe hiện tại của máy chủ.
pub fn tham_so_dat_lai() -> Obj {
    Obj::new()
}

/// Số xe còn trống mà một kết nối biết – đọc từ cap_nhat (có thể đã cũ khi tới máy chủ).
pub struct NguCanh {
    pub so_xe: i64,
    pub trong: i64,
}

impl NguCanh {
    pub fn moi(trang_thai: &Obj) -> NguCanh {
        NguCanh { so_xe: lay_i64(trang_thai, "so_xe").unwrap_or(0), trong: lay_i64(trang_thai, "trong").unwrap_or(0) }
    }

    pub fn cap_nhat(&mut self, tin: &Obj) {
        self.so_xe = lay_i64(tin, "so_xe").unwrap_or(self.so_xe);
        self.trong = lay_i64(tin, "trong").unwrap_or(self.trong);
    }

    pub fn nhan_kq(&mut self, tin: &Obj) {
        if lay_str(tin, "ma_loi") == Some("het_cho") {
            self.trong = 0;
        }
    }
}

/// Bấm "Đặt xe" từ một ngã tư ngẫu nhiên (máy chủ chọn xe trống gần nhất). Bị từ chối vì xung đột
/// (lac_quan hết lượt thử) thì bấm lại, tối đa `lan` lần; "hết chỗ" hay đặt được thì thôi.
pub struct NguoiAo {
    ten: String,
    con: u32,
}

impl NguoiAo {
    pub fn moi(ten: &str, lan: u32) -> NguoiAo {
        NguoiAo { ten: ten.to_string(), con: lan }
    }

    fn dat(&mut self) -> Option<Obj> {
        if self.con == 0 {
            return None;
        }
        self.con -= 1;
        let nt = nga_tu();
        let i = ngau_nhien_nguyen(0, nt.len() as i64 - 1) as usize; // random.sample(NGA_TU, 2): hai ngã tư khác nhau
        let j = (i + 1 + ngau_nhien_nguyen(0, nt.len() as i64 - 2) as usize) % nt.len();
        Some(obj(json!({"loai": "dat_xe", "nguoi": self.ten, "don": json_diem(nt[i]), "den": json_diem(nt[j])})))
    }

    pub fn yeu_cau_dau(&mut self, _nc: &NguCanh) -> Option<Obj> {
        self.dat()
    }

    pub fn sau_kq(&mut self, _nc: &NguCanh, tin: &Obj) -> Option<Obj> {
        if dung(tin, "ok") || matches!(lay_str(tin, "ma_loi"), Some("het_cho" | "da_dat")) {
            return None;
        }
        self.dat()
    }
}
