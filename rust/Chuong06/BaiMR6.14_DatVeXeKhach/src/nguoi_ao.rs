//! Bài mở rộng MR6.14 – Phần NGHIỆP VỤ của bộ mô phỏng tải (khung ở mo_phong.rs):
//!   - NguCanh: điều một kết nối biết về trạng thái chung (ở đây: chỗ trống theo từng chặng, ghế nóng), cập
//!     nhật từ cap_nhat và từ trả lời kq;
//!   - NguoiAo: kịch bản của MỘT người dùng ảo: yeu_cau_dau() -> yêu cầu đầu tiên; sau_kq(tin) -> yêu cầu
//!     kế tiếp hoặc None (xong). Yêu cầu là đối tượng {"loai": ..., các trường...} (khung tự thêm ma_yc).
//!   - tham_so_dat_lai(): tham số gửi kèm dat_lai trước mỗi chế độ trong so_sanh ({} = giữ tham số hiện
//!     tại của máy chủ, ví dụ loại xe/ghế nóng/TTL đã đặt bằng "khach … dat_lai ghe_nong=").

use crate::chung::*;
use serde_json::{json, Value};
use std::collections::HashSet;

pub const SO_LAN_MAC_DINH: u32 = 1; // số lần thử giữ chỗ của mỗi người

/// Giữ tham số hiện tại của máy chủ (loại xe, ghế nóng, TTL).
pub fn tham_so_dat_lai() -> Obj {
    Obj::new()
}

/// Chỗ nào bận ở chặng nào mà một kết nối biết – đọc từ cap_nhat (có thể đã cũ khi tới máy chủ).
pub struct NguCanh {
    ghe: Vec<String>,
    so_tram: usize,
    ghe_nong: String,
    ban: HashSet<String>, // ô "chỗ#chặng" không trống (đang giữ / đã bán)
}

fn o_cua(ghe: &[String], tu: i64, den: i64) -> Vec<String> {
    ghe.iter().flat_map(|g| (tu..den).map(move |k| format!("{g}#{k}"))).collect()
}

impl NguCanh {
    pub fn moi(trang_thai: &Obj) -> NguCanh {
        let ds = |k: &str| -> Vec<String> { trang_thai.get(k).and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default() };
        let mut nc = NguCanh { ghe: ds("ghe"), so_tram: ds("tram").len(), ghe_nong: lay_str(trang_thai, "ghe_nong").unwrap_or("").to_string(), ban: HashSet::new() };
        nc.cap_nhat(trang_thai);
        nc
    }

    pub fn cap_nhat(&mut self, tin: &Obj) {
        self.ban.clear();
        for (g, dong) in self.ghe.iter().zip(tin.get("so_do").and_then(Value::as_array).into_iter().flatten()) {
            for (k, x) in dong.as_str().unwrap_or("").chars().enumerate() {
                if x != '.' {
                    self.ban.insert(format!("{g}#{k}"));
                }
            }
        }
    }

    pub fn nhan_kq(&mut self, tin: &Obj) {
        if !dung(tin, "ok") {
            if let (Some(tu), Some(den)) = (lay_i64(tin, "tu"), lay_i64(tin, "den")) {
                let g: Vec<String> = tin.get("ghe_ban").and_then(Value::as_array).map(|a| a.iter().map(py_str).collect()).unwrap_or_default();
                self.ban.extend(o_cua(&g, tu, den));
            }
        }
    }

    /// Một đoạn đường ngẫu nhiên [tu, den).
    fn doan(&self) -> (i64, i64) {
        let tu = ngau_nhien_nguyen(0, self.so_tram as i64 - 2);
        (tu, ngau_nhien_nguyen(tu + 1, self.so_tram as i64 - 1))
    }

    /// 1–2 chỗ liền nhau (theo thứ tự mã) còn trống cả đoạn [tu, den) theo những gì kết nối này biết.
    fn chon(&self, tu: i64, den: i64) -> Vec<String> {
        let so = ngau_nhien_nguyen(1, 2) as usize;
        let nhom: Vec<Vec<String>> = (0..(self.ghe.len() + 1).saturating_sub(so))
            .map(|i| self.ghe[i..i + so].to_vec())
            .filter(|x| !o_cua(x, tu, den).iter().any(|o| self.ban.contains(o)))
            .collect();
        if nhom.is_empty() {
            vec![chon_ngau_nhien(&self.ghe).clone()]
        } else {
            chon_ngau_nhien(&nhom).clone()
        }
    }
}

/// Lần đầu giữ ghế nóng (ví dụ D01) cho một đoạn ngẫu nhiên (nếu máy chủ không đặt ghế nóng thì chọn chỗ trống);
/// giữ được thì thanh toán ngay; bị từ chối thì đổi đoạn và chọn chỗ trống khác, tối đa `lan` lần giữ chỗ. Vì
/// nhiều đoạn không chồng nhau, ghế nóng có thể bán đúng cho 2–3 người.
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
        let (tu, den) = nc.doan();
        let ghe = if self.lan_dau && !nc.ghe_nong.is_empty() { vec![nc.ghe_nong.clone()] } else { nc.chon(tu, den) };
        self.lan_dau = false;
        Some(obj(json!({"loai": "giu_cho", "nguoi": self.ten, "ghe": ghe, "tu": tu, "den": den})))
    }

    pub fn yeu_cau_dau(&mut self, nc: &NguCanh) -> Option<Obj> {
        self.giu(nc)
    }

    pub fn sau_kq(&mut self, nc: &NguCanh, tin: &Obj) -> Option<Obj> {
        if dung(tin, "ok") && tin.contains_key("het_han_ms") {
            // giữ được: thanh toán ngay
            let lay = |k: &str| tin.get(k).cloned().unwrap_or(Value::Null);
            return Some(obj(json!({"loai": "thanh_toan", "nguoi": self.ten, "ghe": lay("ghe"), "tu": lay("tu"), "den": lay("den")})));
        }
        if dung(tin, "ok") {
            return None; // đã thanh toán: xong
        }
        self.giu(nc)
    }
}
