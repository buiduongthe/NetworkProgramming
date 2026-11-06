//! Bài mở rộng MR6.7 – Lõi máy chủ đặt xe: nghiệp vụ + sổ kiểm toán (khung ở chung.rs).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
//! m chiếc xe (mặc định 1), mỗi xe MỘT chỗ; n người cùng bấm "Đặt xe". Hai thao tác ghi:
//!     {"loai":"dat_xe","ma_yc":k[,"don":[x,y],"den":[x,y]][,"nguoi":"u17"]}
//!     {"loai":"huy","ma_yc":k[,"nguoi":"u17"]}
//! Luật: mỗi người chỉ giữ một chuyến; máy chủ giao xe TRỐNG gần điểm đón nhất; hết xe trống -> "hết chỗ".
//! Hủy chuyến trả xe về trạng thái trống.
//!
//! Bản đồ là lưới 5 × 5 đường (tọa độ 0–1, 1 đơn vị = 5 km); điểm đón/đến là ngã tư. Mỗi xe là MỘT ô
//! "xe1", "xe2"… của KhoTranhChap: Xe {ma, tai_xe, bien_so, mau, vi_tri, khach, don, den, gio, cuoc, phut}.
//! Vì phải biết người đặt đã có chuyến chưa và xe nào trống gần nhất, đặt xe đọc–ghi CẢ m ô trong một
//! thao tác (khoa: giữ khóa mọi xe theo thứ tự tên ô; lac_quan: so sánh phiên bản mọi xe).
//!
//! Sổ kiểm toán đếm một sai phạm:
//!   trung_cho – một lần đặt được chấp nhận cho một xe mà (theo sổ) đang chở người khác: hai người cùng
//!               đọc "xe trống", cùng được báo "Đặt thành công – tài xế Hùng", người ghi sau đè người ghi
//!               trước, người kia đứng đường chờ một chiếc xe không đến.

use crate::chung::*;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub const CONG: u16 = 44710; // cổng mặc định (dòng lệnh)
pub const CONG_GUI: u16 = 44712; // cổng mặc định (giao diện)
/// Kiểu nghiệp vụ của bài – server.rs, giao diện dùng tên chung này.
pub type NV = DatXe;

/// Tùy chọn dòng lệnh của máy chủ: [--so-xe 1]. Tham số đặt lại: so_xe – số xe, mỗi xe một chỗ, 1–50.
pub fn tham_so_dong_lenh(a: &Args) -> Obj {
    obj(json!({"so_xe": a.i64("so-xe", 1)}))
}

pub type Diem = (f64, f64);
pub const DUONG: [f64; 5] = [0.1, 0.3, 0.5, 0.7, 0.9]; // tọa độ 5 đường dọc / 5 đường ngang
pub const TEN_DOC: [&str; 5] = ["Lê Lợi", "Hai Bà Trưng", "Nguyễn Huệ", "Pasteur", "Đồng Khởi"];
pub const TEN_NGANG: [&str; 5] = ["Lý Tự Trọng", "Lê Thánh Tôn", "Nguyễn Du", "Võ Văn Tần", "Điện Biên Phủ"];
pub const KM_MOI_DON_VI: f64 = 5.0;
const TAI_XE: [(&str, &str, &str); 10] = [
    ("Hùng", "51A-123.45", "trắng"),
    ("Lan", "51G-678.90", "đỏ"),
    ("Minh", "51F-246.80", "xanh"),
    ("Tuấn", "51H-135.79", "bạc"),
    ("Hoa", "51K-864.20", "vàng"),
    ("Phúc", "51A-975.31", "đen"),
    ("Quang", "51G-112.23", "cam"),
    ("Thảo", "51F-445.56", "tím"),
    ("Sơn", "51H-778.89", "xám"),
    ("Vy", "51K-990.01", "hồng"),
];
const VI_TRI_XE: [Diem; 10] = [(0.9, 0.1), (0.1, 0.9), (0.5, 0.3), (0.3, 0.7), (0.7, 0.5), (0.1, 0.3), (0.9, 0.7), (0.5, 0.9), (0.3, 0.1), (0.7, 0.9)];

/// 25 ngã tư theo thứ tự hàng (y) rồi cột (x), như NGA_TU của bản Python.
pub fn nga_tu() -> Vec<Diem> {
    DUONG.iter().flat_map(|&y| DUONG.iter().map(move |&x| (x, y))).collect()
}

/// (0.1, 0.5) -> "Lê Lợi × Nguyễn Du".
pub fn ten_diem(p: Diem) -> String {
    let i = |v: f64| DUONG.iter().position(|&d| d == v).unwrap_or(0);
    format!("{} × {}", TEN_DOC[i(p.0)], TEN_NGANG[i(p.1)])
}

/// Đường đi trên lưới phố (Manhattan) giữa hai điểm, km.
pub fn km(a: Diem, b: Diem) -> f64 {
    ((a.0 - b.0).abs() + (a.1 - b.1).abs()) * KM_MOI_DON_VI
}

/// Giá cước: 12.000 đ mở cửa + 10.000 đ/km, làm tròn tới 1.000 đ.
pub fn cuoc(a: Diem, b: Diem) -> i64 {
    round_py((12_000.0 + 10_000.0 * km(a, b)) / 1000.0) * 1000
}

/// Thời gian xe tới điểm đón (30 km/h = 0,5 km/phút), tối thiểu 1 phút.
pub fn phut_den(xe: Diem, don: Diem) -> i64 {
    round_py(km(xe, don) / 0.5).max(1)
}

/// [x, y] trong JSON -> điểm (nếu là một ngã tư trên bản đồ).
pub fn diem_json(v: &Value) -> Option<Diem> {
    let a = v.as_array().filter(|a| a.len() == 2)?;
    let (x, y) = (a[0].as_f64()?, a[1].as_f64()?);
    let p = ((x * 100.0).round() / 100.0, (y * 100.0).round() / 100.0);
    nga_tu().contains(&p).then_some(p)
}

pub fn json_diem(p: Diem) -> Value {
    json!([p.0, p.1])
}

/// Ô "xe<i>": một chiếc xe một chỗ.
#[derive(Clone, Debug)]
pub struct Xe {
    pub ma: i64,
    pub tai_xe: String,
    pub bien_so: String,
    pub mau: String,
    pub vi_tri: Diem,
    pub khach: Option<String>,
    pub don: Option<Diem>,
    pub den: Option<Diem>,
    pub gio: Option<String>,
    pub cuoc: Option<i64>,
    pub phut: Option<i64>,
}

impl Xe {
    fn trong(&self) -> Xe {
        Xe { khach: None, don: None, den: None, gio: None, cuoc: None, phut: None, ..self.clone() }
    }

    fn json(&self) -> Value {
        json!({"ma": self.ma, "tai_xe": self.tai_xe, "bien_so": self.bien_so, "mau": self.mau, "vi_tri": json_diem(self.vi_tri),
               "khach": self.khach, "don": self.don.map(json_diem), "den": self.den.map(json_diem), "gio": self.gio,
               "cuoc": self.cuoc, "phut": self.phut})
    }
}

// ================================================================ sổ kiểm toán
pub struct GdDatXe {
    pub loai: &'static str, // "dat" | "huy"
    pub xe: i64,
    pub nguoi: String,
}

pub struct LuatDatXe {
    tren_xe: Vec<Vec<String>>, // theo sổ: ai đang "có" chỗ trên xe i
    pub so_dat: u64,           // số lần đặt được chấp nhận
}

impl LuatKiemToan for LuatDatXe {
    type GiaoDich = GdDatXe;
    type GiaTri = Xe;
    const TEN_SAI_PHAM: MaTen = &[("trung_cho", "trùng chỗ")];

    fn kiem_tra(&mut self, gd: &GdDatXe, sp: &mut SaiPham) {
        let Some(ds) = self.tren_xe.get_mut(gd.xe as usize - 1) else { return };
        if gd.loai == "dat" {
            self.so_dat += 1;
            if let Some(cu) = ds.last() {
                sp.bao("trung_cho", format!("xe {}: {} khi {cu} đang đặt", gd.xe, gd.nguoi));
            }
            ds.push(gd.nguoi.clone());
        } else if let Some(i) = ds.iter().position(|x| *x == gd.nguoi) {
            // hủy
            ds.remove(i);
        }
    }
}

// ================================================================ nghiệp vụ
pub struct DatXe {
    cac_o: RwLock<Vec<String>>,
    lich_su: Mutex<VecDeque<Obj>>,
    dem_ghi: AtomicU64, // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
    phien: AtomicU64,
}

fn tu_choi(ma_loi: &str, ly_do: &str) -> Obj {
    obj(json!({"ok": false, "ma_loi": ma_loi, "ly_do": ly_do}))
}

impl NghiepVu for DatXe {
    type GiaTri = Xe;
    type Luat = LuatDatXe;
    const TEN: &'static str = "Đặt xe MR6.7";
    const LOAI_YEU_CAU: &'static [&'static str] = &["dat_xe", "huy"];

    fn tham_so_mac_dinh() -> Obj {
        obj(json!({"so_xe": 1}))
    }

    fn moi() -> DatXe {
        DatXe { cac_o: RwLock::new(Vec::new()), lich_su: Mutex::new(VecDeque::new()), dem_ghi: AtomicU64::new(0), phien: AtomicU64::new(0) }
    }

    // ------------------------------------------------------------ tạo / đặt lại
    fn kiem_tham_so(ts: &Obj) -> Result<(), String> {
        if !(1..=50).contains(&lay_i64(ts, "so_xe").unwrap_or(1)) {
            return Err("số xe 1–50".into());
        }
        Ok(())
    }

    fn khoi_tao(mc: &MayChu<Self>) -> (Vec<(String, Xe)>, LuatDatXe) {
        let m = mc.ts_i64("so_xe").max(1) as usize;
        let nt = nga_tu();
        let o: Vec<(String, Xe)> = (0..m)
            .map(|i| {
                let (ten, bien, mau) = TAI_XE[i % TAI_XE.len()];
                let ten = if i >= TAI_XE.len() { format!("{ten} {}", i / TAI_XE.len() + 1) } else { ten.to_string() };
                let vi_tri = if i < VI_TRI_XE.len() { VI_TRI_XE[i] } else { nt[(i * 7) % nt.len()] };
                let xe = Xe { ma: i as i64 + 1, tai_xe: ten, bien_so: bien.into(), mau: mau.into(), vi_tri, khach: None, don: None, den: None, gio: None, cuoc: None, phut: None };
                (format!("xe{}", i + 1), xe)
            })
            .collect();
        *mc.nv.cac_o.write().unwrap() = o.iter().map(|(k, _)| k.clone()).collect();
        mc.nv.lich_su.lock().unwrap().clear();
        (o, LuatDatXe { tren_xe: vec![Vec::new(); m], so_dat: 0 })
    }

    // ------------------------------------------------------------ đặt xe / hủy
    fn xu_ly_yeu_cau(mc: &Arc<MayChu<Self>>, khach: &KhachKetNoi, nguoi: &str, tin: &Obj, tra_loi: TraLoi) {
        let that = khach.so_nguoi.is_none(); // người dùng thật (không phải bộ mô phỏng): ghi nhật ký
        let nguoi = nguoi.to_string();
        let cac_o = mc.nv.cac_o.read().unwrap().clone();
        let m2 = Arc::clone(mc);
        if lay_str(tin, "loai") == Some("dat_xe") {
            let lay = |k: &str, md: Diem| match tin.get(k) {
                None | Some(Value::Null) => Some(md),
                Some(v) => diem_json(v),
            };
            let (Some(don), Some(den)) = (lay("don", (0.5, 0.5)), lay("den", (0.9, 0.9))) else {
                tra_loi(tu_choi("sai_du_lieu", "điểm đón/đến phải là ngã tư [x, y]"));
                return;
            };
            let xu_ly: XuLy<Xe> = Box::new(move |doc| {
                // hàm THUẦN: đọc mọi xe, chọn xe trống gần nhất
                if let Some(xe) = doc.values().find(|x| x.khach.as_deref() == Some(nguoi.as_str())) {
                    return (tu_choi("da_dat", &format!("bạn đã đặt xe {}", xe.tai_xe)), None);
                }
                let Some(xe) = doc.values().filter(|x| x.khach.is_none()).min_by(|a, b| km(a.vi_tri, don).total_cmp(&km(b.vi_tri, don)).then(a.ma.cmp(&b.ma))) else {
                    return (tu_choi("het_cho", "hết chỗ – không còn xe trống"), None);
                };
                let moi = Xe { khach: Some(nguoi.clone()), don: Some(don), den: Some(den), gio: Some(gio_hien()), cuoc: Some(cuoc(don, den)), phut: Some(phut_den(xe.vi_tri, don)), ..xe.clone() };
                let kq = obj(json!({"ok": true, "xe": xe.ma, "khach": nguoi, "tai_xe": xe.tai_xe, "bien_so": xe.bien_so, "mau": xe.mau, "phut": moi.phut, "cuoc": moi.cuoc}));
                (kq, Some(vec![(format!("xe{}", xe.ma), moi)]))
            });
            mc.kho.thuc_hien(cac_o, xu_ly, tra_loi, Some(Box::new(move |kq: &mut Obj| da_ghi(&m2, "dat", kq, that))));
        } else {
            let xu_ly: XuLy<Xe> = Box::new(move |doc| match doc.values().find(|x| x.khach.as_deref() == Some(nguoi.as_str())) {
                Some(xe) => (obj(json!({"ok": true, "xe": xe.ma, "khach": nguoi, "tai_xe": xe.tai_xe})), Some(vec![(format!("xe{}", xe.ma), xe.trong())])),
                None => (tu_choi("chua_dat", "bạn chưa đặt xe"), None),
            });
            mc.kho.thuc_hien(cac_o, xu_ly, tra_loi, Some(Box::new(move |kq: &mut Obj| da_ghi(&m2, "huy", kq, that))));
        }
    }

    // ------------------------------------------------------------ trạng thái gửi client
    fn trang_thai(mc: &MayChu<Self>) -> Obj {
        let phien = mc.nv.phien.load(Ordering::SeqCst); // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
        let ds = mc.kho.anh_chup();
        let trong = ds.values().filter(|x| x.khach.is_none()).count();
        let ls: Vec<Obj> = mc.nv.lich_su.lock().unwrap().iter().take(8).cloned().collect();
        obj(json!({"phien": phien, "so_xe": ds.ds.len(), "trong": trong, "xe": ds.values().map(Xe::json).collect::<Vec<_>>(), "lich_su": ls}))
    }

    fn tom_tat(mc: &MayChu<Self>) -> String {
        let ds = mc.kho.anh_chup();
        let da = ds.values().filter(|x| x.khach.is_some()).count();
        format!("Đã đặt {da}/{} xe, {} lượt báo đặt được", ds.ds.len(), mc.so().xem(|l| l.so_dat))
    }
}

/// Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ tự lần
/// ghi; cap_nhat mang "phien" lúc chụp -> client biết một cap_nhat chụp trước hay sau lần ghi của mình.
fn da_ghi(mc: &MayChu<DatXe>, loai: &'static str, kq: &mut Obj, that: bool) {
    let phien = mc.nv.dem_ghi.fetch_add(1, Ordering::SeqCst) + 1;
    mc.nv.phien.store(phien, Ordering::SeqCst);
    kq.insert("phien".into(), json!(phien));
    let (xe, khach, tai_xe) = (lay_i64(kq, "xe").unwrap_or(0), lay_str(kq, "khach").unwrap_or("").to_string(), lay_str(kq, "tai_xe").unwrap_or("").to_string());
    mc.so().ghi(GdDatXe { loai, xe, nguoi: khach.clone() });
    {
        let mut ls = mc.nv.lich_su.lock().unwrap();
        ls.push_front(obj(json!({"gio": gio_hien(), "loai": loai, "xe": xe, "tai_xe": tai_xe, "nguoi": khach})));
        ls.truncate(30);
    }
    if that {
        if loai == "dat" {
            mc.thong_bao(&format!("Xe {xe} ({tai_xe}) nhận khách {khach}"));
        } else {
            mc.thong_bao(&format!("{khach} hủy chuyến, xe {xe} trống"));
        }
    }
    mc.bao_doi();
}

/// Điểm ([x, y]) của một trường JSON.
pub fn diem_truong(o: &Value, k: &str) -> Option<Diem> {
    let a = o.get(k)?.as_array()?;
    Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?))
}

// Ba chỗ nối với server.rs (bài này không dùng): cờ dòng lệnh không kèm giá trị, dòng báo sau khi lắng nghe,
// lệnh bàn phím riêng của bài (MR6.12, MR6.13 có dùng).
pub const CO_DONG_LENH: &[&str] = &[];

pub fn sau_khoi_dong(_a: &Args, _log: &dyn Fn(&str)) {}

pub fn lenh_rieng(_sv: &Arc<MayChu<NV>>, _p: &[&str]) -> Option<Result<(), String>> {
    None
}
