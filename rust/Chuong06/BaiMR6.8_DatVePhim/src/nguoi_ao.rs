//! Bài mở rộng MR6.8 – Phần NGHIỆP VỤ của bộ mô phỏng tải (khung ở mo_phong.rs):
//!   - NguCanh: điều một kết nối biết về trạng thái chung (ở đây: sơ đồ ghế, ghế nóng), cập nhật từ
//!     cap_nhat và từ trả lời kq;
//!   - NguoiAo: kịch bản của MỘT người dùng ảo: yeu_cau_dau() -> yêu cầu đầu tiên; sau_kq(tin) -> yêu cầu
//!     kế tiếp hoặc None (xong). Yêu cầu là đối tượng {"loai": ..., các trường...} (khung tự thêm ma_yc).
//!   - tham_so_dat_lai(): tham số gửi kèm dat_lai trước mỗi chế độ trong so_sanh ({} = giữ tham số hiện
//!     tại của máy chủ, ví dụ ghế nóng/TTL đã đặt bằng "khach … dat_lai ghe_nong=").

use crate::chung::*;
use serde_json::{json, Value};
use std::collections::HashSet;

pub const SO_LAN_MAC_DINH: u32 = 1; // số lần thử giữ chỗ của mỗi người

/// Giữ tham số hiện tại của máy chủ (ghế nóng, TTL).
pub fn tham_so_dat_lai() -> Obj {
    Obj::new()
}

/// Sơ đồ ghế mà một kết nối biết – đọc từ cap_nhat (có thể đã cũ khi tới máy chủ).
pub struct NguCanh {
    hang: Vec<char>,
    cot: usize,
    ghe_nong: String,
    ban: HashSet<String>, // ghế không trống (đang giữ / đã bán)
}

impl NguCanh {
    pub fn moi(trang_thai: &Obj) -> NguCanh {
        let mut nc = NguCanh {
            hang: lay_str(trang_thai, "hang").unwrap_or("ABCDEFGH").chars().collect(),
            cot: lay_i64(trang_thai, "cot").unwrap_or(12) as usize,
            ghe_nong: lay_str(trang_thai, "ghe_nong").unwrap_or("").to_string(),
            ban: HashSet::new(),
        };
        nc.cap_nhat(trang_thai);
        nc
    }

    pub fn cap_nhat(&mut self, tin: &Obj) {
        self.ban.clear();
        for (r, dong) in self.hang.iter().zip(tin.get("so_do").and_then(Value::as_array).into_iter().flatten()) {
            for (c, x) in dong.as_str().unwrap_or("").chars().enumerate() {
                if x != '.' {
                    self.ban.insert(format!("{r}{}", c + 1));
                }
            }
        }
    }

    pub fn nhan_kq(&mut self, tin: &Obj) {
        if !dung(tin, "ok") {
            for g in tin.get("ghe_ban").and_then(Value::as_array).into_iter().flatten() {
                self.ban.insert(py_str(g));
            }
        }
    }

    /// Một nhóm 1–2 ghế liền nhau ngẫu nhiên, còn trống theo những gì kết nối này biết.
    fn chon(&self) -> Vec<String> {
        let so = ngau_nhien_nguyen(1, 2) as usize;
        let mut nhom: Vec<Vec<String>> = Vec::new();
        for r in &self.hang {
            for c in 1..=(self.cot + 1).saturating_sub(so) {
                let x: Vec<String> = (0..so).map(|i| format!("{r}{}", c + i)).collect();
                if !x.iter().any(|g| self.ban.contains(g)) {
                    nhom.push(x);
                }
            }
        }
        if nhom.is_empty() {
            vec![format!("{}{}", chon_ngau_nhien(&self.hang), ngau_nhien_nguyen(1, self.cot as i64))]
        } else {
            chon_ngau_nhien(&nhom).clone()
        }
    }
}

/// Lần đầu giữ ghế nóng (ví dụ E7; nếu máy chủ không đặt ghế nóng thì chọn nhóm ghế trống); giữ được
/// thì thanh toán ngay; bị từ chối thì chọn nhóm ghế trống khác, tối đa `lan` lần giữ chỗ.
pub struct NguoiAo {
    ten: String,
    con: u32,
    lan_dau: bool,
}

impl NguoiAo {
    pub fn moi(ten: &str, lan: u32) -> NguoiAo {
        NguoiAo { ten: ten.to_string(), con: lan, lan_dau: true }
    }

    fn giu(&mut self, nc: &NguCanh) -> Option<Obj> {
        if self.con == 0 {
            return None;
        }
        self.con -= 1;
        let ghe = if self.lan_dau && !nc.ghe_nong.is_empty() { vec![nc.ghe_nong.clone()] } else { nc.chon() };
        self.lan_dau = false;
        Some(obj(json!({"loai": "giu_cho", "nguoi": self.ten, "ghe": ghe})))
    }

    pub fn yeu_cau_dau(&mut self, nc: &NguCanh) -> Option<Obj> {
        self.giu(nc)
    }

    pub fn sau_kq(&mut self, nc: &NguCanh, tin: &Obj) -> Option<Obj> {
        if dung(tin, "ok") && tin.contains_key("het_han_ms") {
            // giữ được: thanh toán ngay
            return Some(obj(json!({"loai": "thanh_toan", "nguoi": self.ten, "ghe": tin.get("ghe").cloned().unwrap_or(Value::Null)})));
        }
        if dung(tin, "ok") {
            return None; // đã thanh toán: xong
        }
        self.giu(nc)
    }
}
