//! Bài mở rộng MR6.9 – Lõi máy chủ quay số trúng thưởng: nghiệp vụ + sổ kiểm toán (khung ở chung.rs).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
//! Chương trình khuyến mãi "Vòng quay may mắn": 10 món quà, hàng chục nghìn lượt quay. Thao tác ghi duy nhất:
//!     {"loai":"quay","ma_yc":k[,"nguoi":"u17"]}
//! Luật: mỗi người quay ĐÚNG MỘT lần (quay lần nữa – dù bằng ma_yc khác, từ thiết bị khác – bị từ chối
//! "da_quay" kèm kết quả cũ: idempotency theo tên); mỗi lượt trúng với xác suất p (mặc định 1/2000) nếu
//! còn quà; trúng thì nhận ngẫu nhiên một món còn lại.
//!
//! Tài nguyên trong KhoTranhChap (kiểu giá trị ô: OQuay):
//!   - ô "qua"      : Qua(mã các món còn lại) – ô "nóng" mà mọi lượt TRÚNG cùng tranh;
//!   - ô "ng:<tên>" : ChuaQuay | DaQuay(Some(mã) | None) – một ô cho mỗi người, tạo (kho.dat_o – thêm một
//!                    ô, không chép lại cả bảng) khi người đó quay lần đầu. Lượt không trúng chỉ chạm ô của
//!                    chính mình nên các lượt chạy song song được (khóa theo tài nguyên); chỉ lượt trúng mới
//!                    khóa thêm ô "qua".
//! Rút thăm (ngau_nhien() < p) làm TRƯỚC, ngoài vùng găng: kết quả rút thăm không phụ thuộc trạng thái.
//!
//! Sổ kiểm toán đếm hai sai phạm:
//!   thua_qua    – một món quà đã trao lại được trao lần nữa (tổng số quà trao > số quà có);
//!   quay_2_lan  – một người có hai lượt quay được chấp nhận (hai thiết bị cùng bấm, ô của người đó bị
//!                 ghi đè).

use crate::chung::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const CONG: u16 = 44730; // cổng mặc định (dòng lệnh)
pub const CONG_GUI: u16 = 44732; // cổng mặc định (giao diện)
/// Kiểu nghiệp vụ của bài – server.rs, giao diện dùng tên chung này.
pub type NV = QuaySo;

pub const TEN_QUA: [&str; 10] = ["Xe đạp", "Điện thoại", "Laptop", "Tai nghe", "Đồng hồ", "Máy ảnh", "Ba lô", "Nồi cơm điện", "Loa", "Phiếu 500k"];

/// Tùy chọn dòng lệnh của máy chủ: [--so-qua 10] [--p 0.0005]. Tham số đặt lại: so_qua (1–10), p.
pub fn tham_so_dong_lenh(a: &Args) -> Obj {
    obj(json!({"so_qua": a.i64("so-qua", 10), "p": a.f64("p", 0.0005)}))
}

/// 0.0005 -> "1/2000"; 0.3 -> "0.3".
pub fn chu_p(p: f64) -> String {
    if p > 0.0 {
        let n = round_py(1.0 / p);
        if (1.0 / p - n as f64).abs() < 1e-6 && n > 1 {
            return format!("1/{n}");
        }
    }
    fmt_g(p)
}

/// Giá trị một ô của kho.
#[derive(Clone, Debug)]
pub enum OQuay {
    Qua(Vec<usize>),       // ô "qua": mã các món còn lại
    ChuaQuay,              // ô "ng:<tên>" vừa tạo
    DaQuay(Option<usize>), // ô "ng:<tên>": đã quay, trúng món nào (None: không trúng)
}

fn con_qua(o: &OQuay) -> &[usize] {
    match o {
        OQuay::Qua(v) => v,
        _ => &[],
    }
}

// ================================================================ sổ kiểm toán
pub struct GdQuay {
    pub nguoi: String,
    pub qua: Option<usize>,
}

pub struct LuatQuaySo {
    da_quay: HashSet<String>,         // người đã có lượt quay được chấp nhận (theo sổ)
    da_trao: HashMap<usize, String>,  // mã quà -> người nhận đầu tiên
    pub so_trao: u64,                 // số lượt trúng được chấp nhận
    pub so_gd: u64,                   // số giao dịch trong sổ (= len(giao_dich))
}

impl LuatKiemToan for LuatQuaySo {
    type GiaoDich = GdQuay;
    type GiaTri = OQuay;
    const TEN_SAI_PHAM: MaTen = &[("thua_qua", "thừa quà"), ("quay_2_lan", "quay 2 lần")];

    fn kiem_tra(&mut self, gd: &GdQuay, sp: &mut SaiPham) {
        self.so_gd += 1;
        if !self.da_quay.insert(gd.nguoi.clone()) {
            sp.bao("quay_2_lan", format!("{} quay lần 2", gd.nguoi));
        }
        if let Some(q) = gd.qua {
            self.so_trao += 1;
            match self.da_trao.get(&q) {
                Some(truoc) => sp.bao("thua_qua", format!("{}: {truoc} rồi {}", TEN_QUA[q], gd.nguoi)),
                None => {
                    self.da_trao.insert(q, gd.nguoi.clone());
                }
            }
        }
    }
}

// ================================================================ nghiệp vụ
pub struct QuaySo {
    khoa_tao: Mutex<()>,                 // tạo ô "ng:<tên>" đúng một lần
    nguoi_trung: Mutex<VecDeque<Obj>>,   // mới nhất trước (để hiển thị), tối đa 50
    bao_het: AtomicBool,
}

impl QuaySo {
    fn o_nguoi(mc: &MayChu<Self>, nguoi: &str) -> String {
        let k = format!("ng:{nguoi}");
        let _g = mc.nv.khoa_tao.lock().unwrap();
        if !mc.kho.co_o(&k) {
            mc.kho.dat_o(&k, OQuay::ChuaQuay); // thêm MỘT ô (O(1)), không chép cả bảng
        }
        k
    }
}

impl NghiepVu for QuaySo {
    type GiaTri = OQuay;
    type Luat = LuatQuaySo;
    const TEN: &'static str = "Quay số MR6.9";
    const LOAI_YEU_CAU: &'static [&'static str] = &["quay"];

    fn tham_so_mac_dinh() -> Obj {
        obj(json!({"so_qua": 10, "p": 0.0005}))
    }

    fn moi() -> QuaySo {
        QuaySo { khoa_tao: Mutex::new(()), nguoi_trung: Mutex::new(VecDeque::new()), bao_het: AtomicBool::new(false) }
    }

    // ------------------------------------------------------------ tạo / đặt lại chương trình
    fn kiem_tham_so(ts: &Obj) -> Result<(), String> {
        if !(1..=TEN_QUA.len() as i64).contains(&lay_i64(ts, "so_qua").unwrap_or(1)) {
            return Err(format!("số quà 1–{}", TEN_QUA.len()));
        }
        if !(0.0..=1.0).contains(&lay_f64(ts, "p").unwrap_or(0.0)) {
            return Err("xác suất p trong [0, 1]".into());
        }
        Ok(())
    }

    fn khoi_tao(mc: &MayChu<Self>) -> (Vec<(String, OQuay)>, LuatQuaySo) {
        let so_qua = mc.ts_i64("so_qua") as usize;
        mc.nv.nguoi_trung.lock().unwrap().clear();
        mc.nv.bao_het.store(false, Ordering::SeqCst);
        // ô "ng:<tên>" cũ mất theo thay_o (bảng ô mới chỉ có ô "qua")
        (vec![("qua".into(), OQuay::Qua((0..so_qua).collect()))], LuatQuaySo { da_quay: HashSet::new(), da_trao: HashMap::new(), so_trao: 0, so_gd: 0 })
    }

    // ------------------------------------------------------------ quay
    fn xu_ly_yeu_cau(mc: &Arc<MayChu<Self>>, khach: &KhachKetNoi, nguoi: &str, _tin: &Obj, tra_loi: TraLoi) {
        let k_ng = Self::o_nguoi(mc, nguoi);
        let rut_trung = ngau_nhien() < mc.ts_f64("p"); // kim dừng ở ô quà với xác suất p
        let mut cac_khoa = vec![k_ng.clone()];
        if rut_trung && !con_qua(&mc.kho.doc("qua")).is_empty() {
            cac_khoa.push("qua".into()); // hết quà thì khỏi chạm ô "qua" (quà không tăng lại)
        }
        let nguoi = nguoi.to_string();
        let k = k_ng.clone();
        let xu_ly: XuLy<OQuay> = Box::new(move |doc| {
            // hàm THUẦN: đọc doc, trả về (kq, ghi)
            if let Some(OQuay::DaQuay(cu)) = doc.get(&k) {
                let truoc = match cu {
                    None => "không trúng".to_string(),
                    Some(q) => format!("trúng {}", TEN_QUA[*q]),
                };
                return (obj(json!({"ok": false, "ma_loi": "da_quay", "ly_do": format!("đã quay rồi ({truoc})"), "qua": cu})), None);
            }
            if let Some(q) = doc.get("qua") {
                let con = con_qua(q);
                if !con.is_empty() {
                    let chon = *chon_ngau_nhien(con);
                    let con_moi: Vec<usize> = con.iter().copied().filter(|&x| x != chon).collect();
                    let n = con_moi.len();
                    return (
                        obj(json!({"ok": true, "trung": true, "qua": chon, "ten_qua": TEN_QUA[chon], "nguoi": nguoi, "con_lai": n})),
                        Some(vec![(k.clone(), OQuay::DaQuay(Some(chon))), ("qua".into(), OQuay::Qua(con_moi))]),
                    );
                }
            }
            (obj(json!({"ok": true, "trung": false, "qua": null, "nguoi": nguoi})), Some(vec![(k.clone(), OQuay::DaQuay(None))]))
        });
        let gop = khach.so_nguoi.is_some(); // người ảo của bộ mô phỏng: không báo từng người
        let m2 = Arc::clone(mc);
        mc.kho.thuc_hien(cac_khoa, xu_ly, tra_loi, Some(Box::new(move |kq: &mut Obj| da_quay(&m2, kq, gop))));
    }

    // ------------------------------------------------------------ trạng thái gửi client
    fn trang_thai(mc: &MayChu<Self>) -> Obj {
        let con: HashSet<usize> = con_qua(&mc.kho.doc("qua")).iter().copied().collect();
        let trung: Vec<Obj> = mc.nv.nguoi_trung.lock().unwrap().iter().cloned().collect();
        let mut nhan: HashMap<i64, Value> = HashMap::new();
        for x in trung.iter().rev() {
            // người nhận gần nhất của mỗi món
            nhan.insert(lay_i64(x, "qua").unwrap_or(-1), x.get("nguoi").cloned().unwrap_or(Value::Null));
        }
        let so_qua = mc.ts_i64("so_qua") as usize;
        let qua: Vec<Value> = (0..so_qua)
            .map(|i| {
                let c = con.contains(&i);
                json!({"ma": i, "ten": TEN_QUA[i], "con": c, "nguoi": if c { Value::Null } else { nhan.get(&(i as i64)).cloned().unwrap_or(Value::Null) }})
            })
            .collect();
        let (so_gd, so_trao) = mc.so().xem(|l| (l.so_gd, l.so_trao));
        obj(json!({"so_qua": so_qua, "p": mc.ts_f64("p"), "con_lai": con.len(), "qua": qua,
                   "da_quay": so_gd, "so_trung": so_trao, "nguoi_trung": trung.into_iter().take(20).collect::<Vec<_>>()}))
    }

    fn tom_tat(mc: &MayChu<Self>) -> String {
        let (so_gd, so_trao) = mc.so().xem(|l| (l.so_gd, l.so_trao));
        format!("Trao {so_trao}/{} quà, {so_gd} lượt, p {}", mc.ts_i64("so_qua"), chu_p(mc.ts_f64("p")))
    }
}

/// Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.
fn da_quay(mc: &MayChu<QuaySo>, kq: &mut Obj, gop: bool) {
    let nguoi = lay_str(kq, "nguoi").unwrap_or("").to_string();
    let qua = lay_i64(kq, "qua").map(|q| q as usize);
    let so = mc.so();
    so.ghi(GdQuay { nguoi: nguoi.clone(), qua });
    if dung(kq, "trung") {
        {
            let mut ds = mc.nv.nguoi_trung.lock().unwrap();
            ds.push_front(obj(json!({"nguoi": nguoi, "qua": qua, "gio": gio_hien()})));
            ds.truncate(50);
        }
        if !gop {
            mc.thong_bao(&format!("{nguoi} trúng {}", lay_str(kq, "ten_qua").unwrap_or("")));
        }
        if lay_i64(kq, "con_lai") == Some(0) && !mc.nv.bao_het.swap(true, Ordering::SeqCst) {
            mc.thong_bao(&format!("Đã trao hết quà sau {} lượt", so.xem(|l| l.so_gd)));
        }
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
