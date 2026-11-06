//! Bài mở rộng MR6.13 – Phần NGHIỆP VỤ của bộ mô phỏng tải (khung ở mo_phong.rs):
//!   - NguCanh: điều một kết nối biết về trạng thái chung (ở đây: phòng chờ bật hay tắt), cập nhật từ
//!     cap_nhat đầu tiên;
//!   - NguoiAo: kịch bản của MỘT người dùng ảo: yeu_cau_dau() -> yêu cầu đầu tiên; sau_kq(tin) -> yêu cầu
//!     kế tiếp hoặc None (xong). Yêu cầu là đối tượng {"loai": ..., các trường...} (khung tự thêm ma_yc).
//!   - tham_so_dat_lai(): tham số gửi kèm dat_lai trước mỗi chế độ trong so_sanh.
//!
//! Người ảo ở bài này (mỗi người mua 1–4 vé, `lan` không dùng):
//!   - phòng chờ bật:  xep_hang -> cho_luot (máy chủ giữ yêu cầu tới khi tới lượt) -> mua;
//!   - phòng chờ tắt:  mua ngay; bị "qua_tai" thì bấm lại ngay (tối đa 3 lần) – như người dùng nhấn F5;
//!   - 1/10 người là "TAB THỨ HAI" của người liền trước (u10 đóng vai u9…): cùng xếp hàng, cùng được lượt,
//!     cùng bấm mua -> chế độ an toàn chỉ nhận một đơn, khong_khoa nhận cả hai (sai phạm "sai lượt").
//! DO_MUA ghi thời gian từ lần bấm "mua" đầu tiên tới kết quả cuối của từng người (gồm cả các lần bấm lại)
//! – so_sanh_phong_cho() dùng để so sánh bật/tắt phòng chờ.
//! Khứ hồi gồm cả cho_luot – tức là cả thời gian đứng trong phòng chờ.
//! (--dat-lai, --so-sanh: phòng chờ bật, cấp lượt 200 người/s, thùng 20 token)
//! So sánh bật/tắt phòng chờ:  so_sanh_phong_cho <host> <cổng> [--n 1000] [--k 200] [--che-do khoa]

use crate::chung::*;
use crate::mo_phong;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

pub const SO_LAN_MAC_DINH: u32 = 1; // mỗi người một lần mua (1–4 vé)
pub const BAM_LAI: u32 = 3; // bị quá tải / xung đột: bấm lại tối đa 3 lần

fn tham_so_goc() -> Obj {
    obj(json!({"phong_cho": 1, "toc_do": 200.0, "dung_luong": 20}))
}

/// Thay THAM_SO_DAT_LAI tạm thời (so_sanh_phong_cho đặt phong_cho = 0 / 1).
static GHI_DE: Mutex<Option<Obj>> = Mutex::new(None);

/// Phòng chờ bật, cấp lượt 200 người/s, thùng 20 token.
pub fn tham_so_dat_lai() -> Obj {
    GHI_DE.lock().unwrap().clone().unwrap_or_else(tham_so_goc)
}

/// Kết quả cuối của từng người, thời gian mua (ms), số lần bị từ chối vì quá tải – an toàn đa luồng.
#[derive(Default)]
pub struct DoMua {
    pub tre: Vec<f64>,
    pub kq: HashMap<String, u64>,
    pub qua_tai: u64,
}

pub static DO_MUA: LazyLock<Mutex<DoMua>> = LazyLock::new(|| Mutex::new(DoMua::default()));

fn ket_thuc(ma: Option<&str>, tre_ms: Option<f64>) {
    let mut d = DO_MUA.lock().unwrap();
    *d.kq.entry(ma.unwrap_or("None").to_string()).or_default() += 1;
    if let Some(t) = tre_ms {
        d.tre.push(t);
    }
}

/// Phòng chờ bật hay tắt (đọc từ cap_nhat đầu tiên).
pub struct NguCanh {
    pub phong_cho: bool,
}

impl NguCanh {
    pub fn moi(trang_thai: &Obj) -> NguCanh {
        NguCanh { phong_cho: dung(trang_thai, "phong_cho") }
    }
    pub fn cap_nhat(&mut self, _tin: &Obj) {}
    pub fn nhan_kq(&mut self, _tin: &Obj) {}
}

pub struct NguoiAo {
    ai: String,
    so_ve: i64,
    dang: &'static str,
    bam: u32,
    t_mua: Option<Instant>,
}

impl NguoiAo {
    pub fn moi(ten: &str, _lan: u32) -> NguoiAo {
        let i: usize = ten[1..].parse().unwrap_or(1);
        let la_tab2 = i % 10 == 0; // tab thứ hai của người u(i-1)
        let ai = if la_tab2 { format!("u{}", i - 1) } else { ten.to_string() };
        NguoiAo { ai, so_ve: ngau_nhien_nguyen(1, 4), dang: "", bam: 0, t_mua: None }
    }

    fn gui(&mut self, loai: &'static str, them: Obj) -> Option<Obj> {
        self.dang = loai;
        let mut o = obj(json!({"loai": loai, "nguoi": self.ai}));
        o.extend(them);
        Some(o)
    }

    fn mua(&mut self) -> Option<Obj> {
        if self.t_mua.is_none() {
            self.t_mua = Some(Instant::now());
        }
        self.gui("mua", obj(json!({"so_ve": self.so_ve})))
    }

    pub fn yeu_cau_dau(&mut self, nc: &NguCanh) -> Option<Obj> {
        if !nc.phong_cho {
            return self.mua();
        }
        self.gui("xep_hang", Obj::new())
    }

    pub fn sau_kq(&mut self, _nc: &NguCanh, tin: &Obj) -> Option<Obj> {
        let ma = lay_str(tin, "ma_loi");
        if self.dang == "xep_hang" {
            if dung(tin, "ok") {
                return self.gui("cho_luot", Obj::new());
            }
            if ma == Some("xung_dot") && self.bam < BAM_LAI {
                self.bam += 1;
                return self.gui("xep_hang", Obj::new());
            }
            ket_thuc(ma, None);
            return None;
        }
        if self.dang == "cho_luot" {
            if dung(tin, "ok") {
                return self.mua();
            }
            ket_thuc(ma, None); // hết vé khi đang đứng trong hàng
            return None;
        }
        if ma == Some("qua_tai") {
            DO_MUA.lock().unwrap().qua_tai += 1;
        }
        if matches!(ma, Some("qua_tai") | Some("xung_dot")) && self.bam < BAM_LAI {
            self.bam += 1;
            return self.mua();
        }
        let tre = self.t_mua.map(|t| t.elapsed().as_secs_f64() * 1000.0);
        ket_thuc(if dung(tin, "ok") { Some("ok") } else { ma }, tre);
        None
    }
}

// ===================================================================== so sánh bật / tắt phòng chờ
pub const TIEU_DE_PHONG_CHO: &str = "  Phòng chờ Mua được Hết vé Quá tải Bỏ cuộc p50 ms p99 ms";

/// Một hàng của bảng bật/tắt phòng chờ.
#[derive(Clone, Debug)]
pub struct HangPhongCho {
    pub phong_cho: &'static str,
    pub mua_duoc: u64,
    pub het_ve: u64,
    pub qua_tai: u64,
    pub bo_cuoc: u64,
    pub p50_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub giay: f64,
    #[allow(dead_code)] // sai phạm của lượt (theo thong_ke máy chủ) – giữ như bản Python, bảng không in
    pub sai_pham: i64,
}

/// Các dòng bảng so sánh bật/tắt phòng chờ (≤ 64 ký tự). Mua được, Hết vé, Bỏ cuộc: số NGƯỜI (kết quả
/// cuối); Quá tải: số LẦN bị từ chối "qua_tai"; p50/p99: thời gian mua của từng người.
pub fn bang_phong_cho(hang: &[HangPhongCho]) -> Vec<String> {
    let mut ds = vec![TIEU_DE_PHONG_CHO.to_string()];
    for h in hang {
        ds.push(format!("  {:<9}{:>9} {:>6} {:>7} {:>7} {:>6} {:>6}", h.phong_cho, h.mua_duoc, h.het_ve, h.qua_tai, h.bo_cuoc, chu_p99(h.p50_ms), chu_p99(h.p99_ms)));
    }
    if !hang.is_empty() {
        ds.push(format!("  Thời gian cả lượt: {}", hang.iter().map(|h| format!("{} {:.2} s", h.phong_cho, h.giay)).collect::<Vec<_>>().join(", ")));
    }
    ds
}

/// Hai lượt ở cùng chế độ (mặc định khoa): phòng chờ TẮT rồi BẬT; đo thời gian mua của từng người.
pub fn so_sanh_phong_cho(host: &str, cong: u16, n: usize, k: usize, bao: &dyn Fn(&str), che_do: &str) -> Result<Vec<HangPhongCho>, String> {
    let mut hang = Vec::new();
    let kq = (|| {
        for bat in [0, 1] {
            let mut ts = tham_so_goc();
            ts.insert("phong_cho".into(), json!(bat));
            *GHI_DE.lock().unwrap() = Some(ts);
            *DO_MUA.lock().unwrap() = DoMua::default();
            let t0 = Instant::now();
            let Some(h) = mo_phong::chay_luot(host, cong, n, k, 1, bao, Some(che_do), true, None)? else { break };
            let d = DO_MUA.lock().unwrap();
            let mut tre = d.tre.clone();
            tre.sort_by(f64::total_cmp);
            let lay = |m: &str| d.kq.get(m).copied().unwrap_or(0);
            hang.push(HangPhongCho {
                phong_cho: if bat == 1 { "bật" } else { "tắt" },
                mua_duoc: lay("ok"),
                het_ve: lay("het_ve"),
                qua_tai: d.qua_tai,
                bo_cuoc: lay("qua_tai"),
                p50_ms: phan_vi(&tre, 50.0),
                p99_ms: phan_vi(&tre, 99.0),
                giay: t0.elapsed().as_secs_f64(),
                sai_pham: h.sai_pham,
            });
        }
        Ok::<(), String>(())
    })();
    *GHI_DE.lock().unwrap() = None;
    kq.map(|_| hang)
}

/// so_sanh_phong_cho [host] [cổng] [--n 1000] [--k 200] [--che-do khoa]
pub fn run(args: &[String]) {
    let a = Args::parse(args, &[]);
    let (host, cong) = a.host_cong(crate::may_chu::CONG);
    let n = a.i64("n", 1000).max(1) as usize;
    let k = match a.i64("k", 0) {
        x if x > 0 => x as usize,
        _ => n.min(200),
    };
    let che_do = a.str("che-do", "khoa");
    let bao = |text: &str| in_dong("mo_phong", text);
    match so_sanh_phong_cho(&host, cong, n, k, &bao, &che_do) {
        Ok(hang) => {
            bao(&format!("Bật/tắt phòng chờ ({n} người, {che_do}, {} kết nối):", k.min(n)));
            for d in bang_phong_cho(&hang) {
                in_tho(&d);
            }
        }
        Err(e) => {
            bao(&format!("lỗi: {e}"));
            std::process::exit(1);
        }
    }
}
