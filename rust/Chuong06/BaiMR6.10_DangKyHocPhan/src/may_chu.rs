//! Bài mở rộng MR6.10 – Lõi máy chủ đăng ký học phần: nghiệp vụ + sổ kiểm toán (khung ở chung.rs).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
//! Cổng đăng ký học phần ngày mở đăng ký: vài lớp học phần (mặc định 3 lớp, sĩ số 50), hàng nghìn sinh viên
//! cùng bấm "Đăng ký". Hai thao tác ghi:
//!     {"loai":"dang_ky","ma_yc":k,"ma_lop":"123013-01"[,"nguoi":"u17"]}
//!     {"loai":"huy","ma_yc":k,"ma_lop":"123013-01"[,"nguoi":"u17"]}
//! Luật: lớp còn chỗ (số đã đăng ký < sĩ số) mới nhận; sinh viên đã có trong lớp thì không đăng ký lại
//! ("da_dang_ky"); chỉ hủy được lớp mình đã đăng ký ("chua_dang_ky").
//!
//! Mỗi lớp là MỘT ô của KhoTranhChap: Lop { sv: tập tên sinh viên (BTreeSet – sắp sẵn) }; ghi = tạo tập
//! mới (xu_ly thuần). Khóa theo tài nguyên: hai lớp khác nhau được xử lý song song; một lớp "hot" là
//! điểm nóng.
//!
//! Sổ kiểm toán (danh sách lớp theo sổ, ghi trong vùng găng) đếm hai sai phạm:
//!   vuot_si_so – một lượt đăng ký được chấp nhận khi lớp (theo sổ) đã đủ sĩ số;
//!   dk_trung   – một sinh viên được chấp nhận đăng ký một lớp mà sổ ghi là đã có tên (hai thiết bị cùng
//!                bấm, hoặc bấm đúp, cùng đọc "chưa có tên").

use crate::chung::*;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

pub const CONG: u16 = 44740; // cổng mặc định (dòng lệnh)
pub const CONG_GUI: u16 = 44742; // cổng mặc định (giao diện)
/// Kiểu nghiệp vụ của bài – server.rs, giao diện dùng tên chung này.
pub type NV = DangKy;

/// mã lớp, tên học phần, giảng viên, lịch học, số tín chỉ
pub const DANH_MUC: [(&str, &str, &str, &str, i64); 6] = [
    ("123013-01", "Lập trình mạng", "TS. Lê Hoàng Nam", "T2, tiết 1–3, E301", 3),
    ("123005-01", "Quản trị mạng", "ThS. Phạm Thu Hà", "T3, tiết 4–6, E302", 3),
    ("006124-01", "Tin học cơ bản", "ThS. Vũ Minh Đức", "T4, tiết 7–9, E303", 2),
    ("123013-02", "Lập trình mạng", "TS. Lê Hoàng Nam", "T5, tiết 1–3, E301", 3),
    ("123005-02", "Quản trị mạng", "ThS. Phạm Thu Hà", "T6, tiết 4–6, E302", 3),
    ("006124-02", "Tin học cơ bản", "ThS. Vũ Minh Đức", "T7, tiết 7–9, E303", 2),
];

/// Tùy chọn dòng lệnh của máy chủ: [--so-lop 3] [--si-so 50]. Tham số đặt lại: so_lop (1–6), si_so (1–500).
pub fn tham_so_dong_lenh(a: &Args) -> Obj {
    obj(json!({"so_lop": a.i64("so-lop", 3), "si_so": a.i64("si-so", 50)}))
}

/// Ô "lop:<mã>": danh sách sinh viên đã đăng ký.
#[derive(Clone, Debug, Default)]
pub struct Lop {
    pub sv: BTreeSet<String>,
}

// ================================================================ sổ kiểm toán
pub struct GdDangKy {
    pub thao_tac: String,
    pub nguoi: String,
    pub ma_lop: String,
}

pub struct LuatDangKy {
    si_so: usize,
    lop: HashMap<String, HashSet<String>>, // mã lớp -> sinh viên theo sổ
}

impl LuatDangKy {
    pub fn tong_so(&self) -> usize {
        self.lop.values().map(HashSet::len).sum()
    }
    pub fn so_lop(&self, ma: &str) -> usize {
        self.lop.get(ma).map_or(0, HashSet::len)
    }
}

impl LuatKiemToan for LuatDangKy {
    type GiaoDich = GdDangKy;
    type GiaTri = Lop;
    const TEN_SAI_PHAM: MaTen = &[("vuot_si_so", "vượt sĩ số"), ("dk_trung", "ĐK trùng")];

    fn kiem_tra(&mut self, gd: &GdDangKy, sp: &mut SaiPham) {
        let ds = self.lop.entry(gd.ma_lop.clone()).or_default();
        if gd.thao_tac == "huy" {
            ds.remove(&gd.nguoi);
            return;
        }
        if ds.contains(&gd.nguoi) {
            sp.bao("dk_trung", format!("{} ĐK lại {}", gd.nguoi, gd.ma_lop));
        } else if ds.len() >= self.si_so {
            sp.bao("vuot_si_so", format!("{}: {} là SV thứ {}", gd.ma_lop, gd.nguoi, ds.len() + 1));
        }
        ds.insert(gd.nguoi.clone());
    }
}

// ================================================================ nghiệp vụ
pub struct DangKy;

impl DangKy {
    fn so_lop(mc: &MayChu<Self>) -> usize {
        (mc.ts_i64("so_lop").max(0) as usize).min(DANH_MUC.len())
    }
}

impl NghiepVu for DangKy {
    type GiaTri = Lop;
    type Luat = LuatDangKy;
    const TEN: &'static str = "Đăng ký học phần MR6.10";
    const LOAI_YEU_CAU: &'static [&'static str] = &["dang_ky", "huy"];

    fn tham_so_mac_dinh() -> Obj {
        obj(json!({"so_lop": 3, "si_so": 50}))
    }

    fn moi() -> DangKy {
        DangKy
    }

    // ------------------------------------------------------------ tạo / đặt lại đợt đăng ký
    fn kiem_tham_so(ts: &Obj) -> Result<(), String> {
        if !(1..=DANH_MUC.len() as i64).contains(&lay_i64(ts, "so_lop").unwrap_or(1)) {
            return Err(format!("số lớp 1–{}", DANH_MUC.len()));
        }
        if !(1..=500).contains(&lay_i64(ts, "si_so").unwrap_or(1)) {
            return Err("sĩ số 1–500".into());
        }
        Ok(())
    }

    fn khoi_tao(mc: &MayChu<Self>) -> (Vec<(String, Lop)>, LuatDangKy) {
        let o = DANH_MUC[..Self::so_lop(mc)].iter().map(|x| (format!("lop:{}", x.0), Lop::default())).collect();
        (o, LuatDangKy { si_so: mc.ts_i64("si_so") as usize, lop: HashMap::new() })
    }

    // ------------------------------------------------------------ đăng ký / hủy
    fn xu_ly_yeu_cau(mc: &Arc<MayChu<Self>>, khach: &KhachKetNoi, nguoi: &str, tin: &Obj, tra_loi: TraLoi) {
        let ma = tin.get("ma_lop").cloned().unwrap_or(Value::Null);
        let Some(ma) = ma.as_str().filter(|m| DANH_MUC[..Self::so_lop(mc)].iter().any(|x| x.0 == *m)).map(str::to_string) else {
            tra_loi(obj(json!({"ok": false, "ma_loi": "sai_du_lieu", "ly_do": format!("không có lớp {}", py_str(&ma))})));
            return;
        };
        let (k, si_so, thao_tac, nguoi) = (format!("lop:{ma}"), mc.ts_i64("si_so") as usize, lay_str(tin, "loai").unwrap_or("").to_string(), nguoi.to_string());
        let k2 = k.clone();
        let xu_ly: XuLy<Lop> = Box::new(move |doc| {
            // hàm THUẦN: đọc doc, trả về (kq, ghi)
            let sv = &doc[k2.as_str()].sv;
            let mut moi = sv.clone();
            if thao_tac == "dang_ky" {
                if sv.contains(&nguoi) {
                    return (obj(json!({"ok": false, "ma_loi": "da_dang_ky", "ly_do": "đã đăng ký lớp này", "ma_lop": ma, "da_dk": sv.len()})), None);
                }
                if sv.len() >= si_so {
                    return (obj(json!({"ok": false, "ma_loi": "lop_day", "ly_do": "lớp đã đủ", "ma_lop": ma, "da_dk": sv.len()})), None);
                }
                moi.insert(nguoi.clone());
            } else {
                if !sv.contains(&nguoi) {
                    return (obj(json!({"ok": false, "ma_loi": "chua_dang_ky", "ly_do": "chưa đăng ký lớp này", "ma_lop": ma, "da_dk": sv.len()})), None);
                }
                moi.remove(&nguoi);
            }
            let n = moi.len();
            (obj(json!({"ok": true, "thao_tac": thao_tac, "ma_lop": ma, "nguoi": nguoi, "da_dk": n})), Some(vec![(k2.clone(), Lop { sv: moi })]))
        });
        let gop = khach.so_nguoi.is_some();
        let m2 = Arc::clone(mc);
        mc.kho.thuc_hien(vec![k], xu_ly, tra_loi, Some(Box::new(move |kq: &mut Obj| da_ghi(&m2, kq, gop))));
    }

    // ------------------------------------------------------------ trạng thái gửi client
    fn trang_thai(mc: &MayChu<Self>) -> Obj {
        let si_so = mc.ts_i64("si_so");
        let so = mc.so();
        let ds: Vec<Value> = DANH_MUC[..Self::so_lop(mc)]
            .iter()
            .map(|&(ma, ten, gv, lich, tc)| {
                let sv = mc.kho.doc(&format!("lop:{ma}"));
                json!({"ma": ma, "ten": ten, "gv": gv, "lich": lich, "tc": tc, "si_so": si_so,
                       "da_dk": sv.sv.len(), "theo_so": so.xem(|l| l.so_lop(ma)), "sv": sv.sv.iter().collect::<Vec<_>>()})
            })
            .collect();
        obj(json!({"si_so": si_so, "lop": ds}))
    }

    fn tom_tat(mc: &MayChu<Self>) -> String {
        let (so_lop, si_so) = (mc.ts_i64("so_lop"), mc.ts_i64("si_so"));
        format!("Đã ĐK {}/{} chỗ ({so_lop} lớp × {si_so})", mc.so().xem(|l| l.tong_so()), so_lop * si_so)
    }
}

/// Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.
fn da_ghi(mc: &MayChu<DangKy>, kq: &mut Obj, gop: bool) {
    let s = |k: &str| lay_str(kq, k).unwrap_or("").to_string();
    mc.so().ghi(GdDangKy { thao_tac: s("thao_tac"), nguoi: s("nguoi"), ma_lop: s("ma_lop") });
    if !gop {
        // người dùng thật: báo lên nhật ký
        let viec = if s("thao_tac") == "dang_ky" { "đăng ký" } else { "hủy" };
        mc.thong_bao(&format!("{} {viec} {} ({}/{})", s("nguoi"), s("ma_lop"), so_us(kq.get("da_dk")), mc.ts_i64("si_so")));
    }
    mc.bao_doi();
}

// Ba chỗ nối với server.rs (bài này không dùng): cờ dòng lệnh không kèm giá trị, dòng báo sau khi lắng nghe,
// lệnh bàn phím riêng của bài (MR6.12, MR6.13 có dùng).
pub const CO_DONG_LENH: &[&str] = &[];

pub fn sau_khoi_dong(_a: &Args, _log: &dyn Fn(&str)) {}

pub fn lenh_rieng(_sv: &Arc<MayChu<NV>>, _p: &[&str]) -> Option<Result<(), String>> {
    None
}
