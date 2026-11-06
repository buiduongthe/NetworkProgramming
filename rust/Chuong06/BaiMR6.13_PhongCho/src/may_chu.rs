//! Bài mở rộng MR6.13 – Lõi máy chủ bán vé có phòng chờ ảo: nghiệp vụ + sổ kiểm toán (khung ở chung.rs).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
//! Bán 500 vé concert cho rất nhiều người cùng lúc. Ba thao tác của người dùng:
//!     {"loai":"xep_hang","ma_yc":k}            vào phòng chờ -> số thứ tự, số người phía trước, thời gian chờ ước tính
//!     {"loai":"cho_luot","ma_yc":k}            "chờ dài" (long polling): máy chủ CHỈ trả lời khi tới lượt (kèm
//!                                              han_ms – thời gian còn lại để mua) hoặc khi hết vé
//!     {"loai":"mua","ma_yc":k,"so_ve":2}       mua 1–4 vé; khi phòng chờ bật phải đang có lượt còn hạn
//! (kết nối gộp của bộ mô phỏng thêm "nguoi":"u17").
//!
//! Phòng chờ ảo (virtual waiting room): máy chủ cấp lượt theo THÙNG TOKEN (token bucket): token rót vào đều
//! toc_do token/giây, thùng chứa tối đa dung_luong token; mỗi 100 ms luồng nền lấy người đầu hàng (số thứ tự
//! nhỏ nhất) ra, mỗi người tốn một token -> lượt có hạn luot_s giây (mặc định 20 s) để mua. Nhờ vậy phía "mua"
//! (phần đắt: kiểm tra, ghi kho) chỉ nhận đều đặn vài chục người mỗi giây dù hàng chờ có hàng nghìn người.
//! Tắt phòng chờ (phong_cho = 0): ai cũng gọi mua ngay -> dồn cục; máy chủ chỉ cho tối đa gioi_han_xu_ly yêu
//! cầu mua đang xử lý cùng lúc, quá số đó từ chối ngay "qua_tai" (như HTTP 503) -> nhiều từ chối, độ trễ cao.
//!
//! Các ô của KhoTranhChap (đi qua 4 cách xử lý tranh chấp), kiểu giá trị OPhongCho:
//!   "hang" Hang {cuoi: số thứ tự cuối đã phát}  – xep_hang đọc, +1, ghi: phát số thứ tự;
//!   "kho"  Kho {con: vé còn, da_ban: đã bán}    – mua đọc, kiểm tra số vé, trừ kho;
//!   "luot:<tên>" Luot {con: còn lượt?, han: hạn} – tạo khi cấp lượt (kho.dat_o: thêm một ô); mua đọc cùng
//!                "kho" và ghi con = false trong CÙNG thao tác (tiêu lượt nguyên tử với trừ kho).
//! Sổ kiểm toán đếm ba sai phạm:
//!   ban_qua    "vượt vé"  – số vé bán vượt tổng số vé (theo sổ: cộng số vé mọi đơn được chấp nhận);
//!   khong_luot "sai lượt" – một đơn được chấp nhận khi người đó không còn lượt (lượt đã dùng ở tab khác);
//!   dao_thu_tu "đảo số"   – người vào hàng sau nhận số thứ tự không lớn hơn người vào trước (FIFO bị phá).

use crate::chung::*;
use serde_json::{json, Value};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const CONG: u16 = 44770; // cổng mặc định (dòng lệnh)
pub const CONG_GUI: u16 = 44772; // cổng mặc định (giao diện)
/// Kiểu nghiệp vụ của bài – server.rs, giao diện dùng tên chung này.
pub type NV = PhongCho;

/// Cờ dòng lệnh không kèm giá trị của server (server.rs truyền cho Args::parse).
pub const CO_DONG_LENH: &[&str] = &["tat-phong-cho"];

/// Tùy chọn dòng lệnh của máy chủ: [--tong-ve 500] [--toc-do 50] [--dung-luong 50] [--luot-s 20]
/// [--gioi-han 64] [--tat-phong-cho]. Tham số đặt lại: tong_ve, ve_toi_da, gia_ve, ten_su_kien, phong_cho
/// (0/1), toc_do, dung_luong, luot_s, gioi_han_xu_ly.
pub fn tham_so_dong_lenh(a: &Args) -> Obj {
    obj(json!({"tong_ve": a.i64("tong-ve", 500), "toc_do": a.f64("toc-do", 50.0), "dung_luong": a.i64("dung-luong", 50),
               "luot_s": a.f64("luot-s", 20.0), "gioi_han_xu_ly": a.i64("gioi-han", 64), "phong_cho": if a.co("tat-phong-cho") { 0 } else { 1 }}))
}

/// Gọi sau khi máy chủ dòng lệnh đã lắng nghe.
pub fn sau_khoi_dong(a: &Args, log: &dyn Fn(&str)) {
    log(&format!("Phòng chờ {}: {} vé, cấp lượt {} người/s", if a.co("tat-phong-cho") { "TẮT" } else { "BẬT" }, a.i64("tong-ve", 500), fmt_g(a.f64("toc-do", 50.0))));
}

/// Lệnh bàn phím riêng của bài cho server dòng lệnh: toc_do <người/s> (đổi tốc độ cấp lượt ngay).
pub fn lenh_rieng(sv: &Arc<MayChu<NV>>, p: &[&str]) -> Option<Result<(), String>> {
    match p {
        ["toc_do", x] => Some(x.parse::<f64>().map_err(|_| format!("could not convert string to float: '{x}'")).and_then(|v| PhongCho::dat_toc_do(sv, v))),
        _ => None,
    }
}

/// Giá trị một ô của kho.
#[derive(Clone, Debug)]
pub enum OPhongCho {
    Kho { con: i64, da_ban: i64 },
    Hang { cuoi: i64 },
    Luot { con: bool, han: f64 },
}

fn kq_het_ve() -> Obj {
    obj(json!({"ok": false, "ma_loi": "het_ve", "ly_do": "đã bán hết vé"}))
}

// ================================================================ sổ kiểm toán
pub enum GdPhongCho {
    Xep { nguoi: String, so: i64 },
    Luot { nguoi: String },
    Mua { nguoi: String, so_ve: i64, can_luot: bool },
}

pub struct LuatPhongCho {
    tong_ve: i64,
    pub da_ban: i64,         // tổng vé theo sổ
    so_max: i64,             // số thứ tự lớn nhất đã phát (theo thứ tự ghi sổ)
    luot: HashSet<String>,   // người đang có lượt chưa dùng (theo sổ)
    ban_qua_them: u64,       // "vượt vé" đếm theo SỐ VÉ: phần cộng thêm ngoài 1 lần báo mỗi đơn
}

impl LuatKiemToan for LuatPhongCho {
    type GiaoDich = GdPhongCho;
    type GiaTri = OPhongCho;
    const TEN_SAI_PHAM: MaTen = &[("ban_qua", "vượt vé"), ("khong_luot", "sai lượt"), ("dao_thu_tu", "đảo số")];

    fn kiem_tra(&mut self, gd: &GdPhongCho, sp: &mut SaiPham) {
        match gd {
            GdPhongCho::Xep { nguoi, so } => {
                if *so <= self.so_max {
                    sp.bao("dao_thu_tu", format!("{nguoi} vào sau nhưng nhận số {so} ≤ {}", self.so_max));
                } else {
                    self.so_max = *so;
                }
            }
            GdPhongCho::Luot { nguoi } => {
                self.luot.insert(nguoi.clone());
            }
            GdPhongCho::Mua { nguoi, so_ve, can_luot } => {
                if *can_luot && !self.luot.remove(nguoi) {
                    sp.bao("khong_luot", format!("{nguoi} mua {so_ve} vé khi lượt đã dùng"));
                }
                let truoc = self.da_ban;
                self.da_ban += so_ve;
                let vuot = self.da_ban - truoc.max(self.tong_ve);
                if vuot > 0 {
                    // đếm theo SỐ VÉ bán vượt
                    self.ban_qua_them += (vuot - 1) as u64;
                    sp.bao("ban_qua", format!("{nguoi} mua {so_ve} vé, sổ đã bán {}", self.da_ban));
                }
            }
        }
    }

    fn kiem_tra_cuoi(&self, _doc: &Doc<OPhongCho>) -> Vec<(&'static str, u64)> {
        if self.ban_qua_them > 0 {
            vec![("ban_qua", self.ban_qua_them)]
        } else {
            Vec::new()
        }
    }
}

// ================================================================ phòng chờ
struct Ng {
    so: i64,
    trang_thai: &'static str, // cho | luot | da_mua | het_han | het_ve
    han: Option<f64>,
    cho: Vec<TraLoi>,          // các yêu cầu cho_luot đang được giữ
}

struct Pc {
    nguoi: HashMap<String, Ng>,
    heap: BinaryHeap<Reverse<(i64, u64, String)>>, // (số thứ tự, thứ tự ghi, tên) của người đang chờ
    stt: u64,
    co_luot: VecDeque<(f64, String)>, // (hạn, tên) theo thứ tự cấp lượt – để thu hồi lượt quá hạn
    dang_phuc_vu: i64,                 // số thứ tự lớn nhất đã được cấp lượt
    dang_cho: i64,
    da_cap_luot: i64,
    het_han: i64,
    token: f64,
    t_token: f64,
    het_ve: bool,
    dang_xu_ly: i64,
    xu_ly_max: i64,
    qua_tai: i64,
    so_don: i64,
    lich_su: VecDeque<Obj>, // đơn mới nhất trước, tối đa 30
}

impl Pc {
    fn moi(dung_luong: f64) -> Pc {
        Pc {
            nguoi: HashMap::new(),
            heap: BinaryHeap::new(),
            stt: 0,
            co_luot: VecDeque::new(),
            dang_phuc_vu: 0,
            dang_cho: 0,
            da_cap_luot: 0,
            het_han: 0,
            token: dung_luong,
            t_token: bay_gio(),
            het_ve: false,
            dang_xu_ly: 0,
            xu_ly_max: 0,
            qua_tai: 0,
            so_don: 0,
            lich_su: VecDeque::new(),
        }
    }

    /// Số người phía trước và thời gian chờ ước tính.
    fn vi_tri(&self, ng: &Ng, toc_do: f64) -> Obj {
        let truoc = if ng.trang_thai == "cho" { (ng.so - self.dang_phuc_vu - 1).max(0) } else { 0 };
        obj(json!({"so": ng.so, "truoc": truoc, "cho_s": ((truoc + 1) as f64 / toc_do * 10.0).round() / 10.0, "trang_thai": ng.trang_thai}))
    }
}

pub struct PhongCho {
    pc: Mutex<Pc>,      // bảo vệ phòng chờ (danh sách người, hàng, thùng token)
    ma_don: AtomicU64,  // mã đơn tăng mãi, kể cả qua các lần đặt lại
}

impl PhongCho {
    /// Đổi tốc độ cấp lượt ngay khi đang chạy (không đặt lại phiên).
    pub fn dat_toc_do(mc: &MayChu<Self>, toc_do: f64) -> Result<(), String> {
        mc.dat_mot_tham_so("toc_do", json!(toc_do))?;
        mc.thong_bao(&format!("Tốc độ cấp lượt: {} người/s", fmt_g(toc_do)));
        mc.bao_doi();
        Ok(())
    }

    fn xep_hang(mc: &Arc<MayChu<Self>>, nguoi: &str, tra_loi: TraLoi) {
        let toc_do = mc.ts_f64("toc_do");
        let kq = {
            let pc = mc.nv.pc.lock().unwrap();
            if pc.het_ve {
                Some(kq_het_ve())
            } else {
                pc.nguoi.get(nguoi).map(|ng| {
                    // đã trong hàng (tải lại trang, tab thứ hai): giữ chỗ cũ
                    let mut k = obj(json!({"ok": true, "da_co": true}));
                    k.extend(pc.vi_tri(ng, toc_do));
                    k
                })
            }
        };
        if let Some(kq) = kq {
            tra_loi(kq);
            return;
        }
        let xu_ly: XuLy<OPhongCho> = Box::new(|doc| {
            // phát số thứ tự: đọc – cộng 1 – ghi
            let so = match &doc["hang"] {
                OPhongCho::Hang { cuoi } => cuoi + 1,
                _ => 1,
            };
            (obj(json!({"ok": true, "so": so})), Some(vec![("hang".into(), OPhongCho::Hang { cuoi: so })]))
        });
        let (m2, nguoi) = (Arc::clone(mc), nguoi.to_string());
        let khi_ghi: KhiGhi = Box::new(move |kq: &mut Obj| {
            // trong vùng găng: đưa vào hàng, ghi sổ
            let so = lay_i64(kq, "so").unwrap_or(0);
            let toc_do = m2.ts_f64("toc_do");
            let moi = {
                let mut g = m2.nv.pc.lock().unwrap();
                let pc = &mut *g;
                let moi = !pc.nguoi.contains_key(&nguoi);
                if moi && pc.het_ve {
                    // vừa hết vé trong lúc xếp hàng: cho_luot báo hết vé ngay
                    pc.nguoi.insert(nguoi.clone(), Ng { so, trang_thai: "het_ve", han: None, cho: Vec::new() });
                } else if moi {
                    pc.nguoi.insert(nguoi.clone(), Ng { so, trang_thai: "cho", han: None, cho: Vec::new() });
                    pc.stt += 1;
                    pc.heap.push(Reverse((so, pc.stt, nguoi.clone())));
                    pc.dang_cho += 1;
                } else {
                    kq.insert("da_co".into(), json!(true));
                }
                let vt = pc.vi_tri(&pc.nguoi[&nguoi], toc_do);
                kq.extend(vt);
                moi
            };
            if moi {
                m2.so().ghi(GdPhongCho::Xep { nguoi: nguoi.clone(), so });
            }
            m2.bao_doi();
        });
        mc.kho.thuc_hien(vec!["hang".into()], xu_ly, tra_loi, Some(khi_ghi));
    }

    /// Chờ dài: chưa tới lượt thì GIỮ tra_loi, luồng nền gọi khi cấp lượt / hết vé.
    fn cho_luot(mc: &MayChu<Self>, nguoi: &str, tra_loi: TraLoi) {
        let kq = {
            let mut pc = mc.nv.pc.lock().unwrap();
            match pc.nguoi.get_mut(nguoi) {
                None => obj(json!({"ok": false, "ma_loi": "chua_xep_hang", "ly_do": "chưa vào hàng chờ"})),
                Some(ng) if ng.trang_thai == "cho" => {
                    ng.cho.push(tra_loi);
                    return;
                }
                Some(ng) if ng.trang_thai == "luot" => {
                    let han_ms = round_py((ng.han.unwrap_or(0.0) - bay_gio()) * 1000.0).max(0);
                    obj(json!({"ok": true, "luot": true, "so": ng.so, "han_ms": han_ms}))
                }
                Some(ng) if ng.trang_thai == "da_mua" => obj(json!({"ok": false, "ma_loi": "da_mua", "ly_do": "lượt này đã dùng để mua"})),
                Some(ng) if ng.trang_thai == "het_han" => obj(json!({"ok": false, "ma_loi": "het_luot", "ly_do": "lượt mua đã hết hạn"})),
                Some(_) => kq_het_ve(),
            }
        };
        tra_loi(kq);
    }

    fn mua(mc: &Arc<MayChu<Self>>, nguoi: &str, tin: &Obj, tra_loi: TraLoi) {
        let ve_toi_da = mc.ts_i64("ve_toi_da");
        let Some(so_ve) = lay_i64(tin, "so_ve").filter(|s| (1..=ve_toi_da).contains(s)) else {
            tra_loi(obj(json!({"ok": false, "ma_loi": "sai_du_lieu", "ly_do": format!("số vé 1–{ve_toi_da}")})));
            return;
        };
        let qua_tai = {
            // giới hạn số yêu cầu mua đang xử lý cùng lúc
            let mut pc = mc.nv.pc.lock().unwrap();
            if pc.dang_xu_ly >= mc.ts_i64("gioi_han_xu_ly") {
                pc.qua_tai += 1;
                true
            } else {
                pc.dang_xu_ly += 1;
                pc.xu_ly_max = pc.xu_ly_max.max(pc.dang_xu_ly);
                false
            }
        };
        if qua_tai {
            tra_loi(obj(json!({"ok": false, "ma_loi": "qua_tai", "ly_do": "hệ thống quá tải, vui lòng thử lại"})));
            return;
        }
        let (pc_bat, gia) = (mc.ts_i64("phong_cho") != 0, mc.ts_i64("gia_ve"));
        let o_luot = format!("luot:{nguoi}"); // ô lượt của người này (tạo khi được cấp lượt)
        let o2 = o_luot.clone();
        let xu_ly: XuLy<OPhongCho> = Box::new(move |doc| {
            // kiểm tra lượt + số vé còn; trừ kho VÀ tiêu lượt
            let (con, da_ban) = match &doc["kho"] {
                OPhongCho::Kho { con, da_ban } => (*con, *da_ban),
                _ => (0, 0),
            };
            let mut han = 0.0;
            if pc_bat {
                if let Some(OPhongCho::Luot { con: c, han: h }) = doc.get(&o2) {
                    if !c {
                        return (obj(json!({"ok": false, "ma_loi": "da_mua", "ly_do": "lượt này đã dùng để mua"})), None);
                    }
                    if bay_gio() > *h {
                        return (obj(json!({"ok": false, "ma_loi": "het_luot", "ly_do": "lượt mua đã hết hạn"})), None);
                    }
                    han = *h;
                }
            }
            if con <= 0 {
                let mut k = kq_het_ve();
                k.insert("con".into(), json!(0));
                return (k, None);
            }
            if con < so_ve {
                return (obj(json!({"ok": false, "ma_loi": "het_ve", "ly_do": format!("chỉ còn {con} vé"), "con": con})), None);
            }
            let mut ghi = vec![("kho".to_string(), OPhongCho::Kho { con: con - so_ve, da_ban: da_ban + so_ve })];
            if pc_bat {
                ghi.push((o2.clone(), OPhongCho::Luot { con: false, han }));
            }
            (obj(json!({"ok": true, "so_ve": so_ve, "thanh_tien": so_ve * gia, "con": con - so_ve})), Some(ghi))
        });
        let (m1, m2, n1) = (Arc::clone(mc), Arc::clone(mc), nguoi.to_string());
        let khi_ghi: KhiGhi = Box::new(move |kq: &mut Obj| {
            // trong vùng găng: tiêu lượt, ghi sổ
            let ma = m1.nv.ma_don.fetch_add(1, Ordering::SeqCst);
            kq.insert("ma_don".into(), json!(ma));
            if pc_bat {
                if let Some(ng) = m1.nv.pc.lock().unwrap().nguoi.get_mut(&n1) {
                    ng.trang_thai = "da_mua";
                }
            }
            m1.so().ghi(GdPhongCho::Mua { nguoi: n1.clone(), so_ve, can_luot: pc_bat });
            {
                let mut pc = m1.nv.pc.lock().unwrap();
                pc.so_don += 1;
                pc.lich_su.push_front(obj(json!({"ma": ma, "gio": gio_hien(), "nguoi": n1, "so_ve": so_ve})));
                pc.lich_su.truncate(30);
            }
            m1.bao_doi();
        });
        let tra: TraLoi = Box::new(move |kq: Obj| {
            m2.nv.pc.lock().unwrap().dang_xu_ly -= 1;
            let kq = if lay_str(&kq, "ma_loi") == Some("khong_co") {
                // chưa có ô lượt: chưa được cấp lượt
                obj(json!({"ok": false, "ma_loi": "khong_luot", "ly_do": "chưa tới lượt – hãy vào hàng chờ"}))
            } else {
                kq
            };
            tra_loi(kq);
        });
        let cac_khoa = if pc_bat { vec!["kho".to_string(), o_luot] } else { vec!["kho".to_string()] };
        mc.kho.thuc_hien(cac_khoa, xu_ly, tra, Some(khi_ghi));
    }
}

impl NghiepVu for PhongCho {
    type GiaTri = OPhongCho;
    type Luat = LuatPhongCho;
    const TEN: &'static str = "Bán vé MR6.13";
    const LOAI_YEU_CAU: &'static [&'static str] = &["xep_hang", "cho_luot", "mua"];

    fn tham_so_mac_dinh() -> Obj {
        obj(json!({"ten_su_kien": "Đêm nhạc Mùa Đông 2026", "gia_ve": 1_500_000, "tong_ve": 500,
                   "ve_toi_da": 4, "phong_cho": 1, "toc_do": 50.0, "dung_luong": 50, "luot_s": 20.0,
                   "gioi_han_xu_ly": 64}))
    }

    fn moi() -> PhongCho {
        PhongCho { pc: Mutex::new(Pc::moi(50.0)), ma_don: AtomicU64::new(1) }
    }

    // ------------------------------------------------------------ tạo / đặt lại
    fn kiem_tham_so(ts: &Obj) -> Result<(), String> {
        let i = |k: &str, d: i64| lay_i64(ts, k).unwrap_or(d);
        let f = |k: &str, d: f64| lay_f64(ts, k).unwrap_or(d);
        if i("tong_ve", 1) < 1 || !(1..=10).contains(&i("ve_toi_da", 1)) || i("gia_ve", 0) < 0 {
            return Err("tổng vé ≥ 1, vé tối đa 1–10, giá ≥ 0".into());
        }
        if !matches!(i("phong_cho", 0), 0 | 1) {
            return Err("phong_cho = 0 hoặc 1".into());
        }
        let tdo = f("toc_do", 1.0);
        if !(tdo > 0.0 && tdo <= 100_000.0) || i("dung_luong", 1) < 1 {
            return Err("tốc độ cấp lượt 0–100000 người/s, dung lượng thùng ≥ 1".into());
        }
        if f("luot_s", 1.0) <= 0.0 || i("gioi_han_xu_ly", 1) < 1 {
            return Err("thời hạn lượt > 0, giới hạn xử lý ≥ 1".into());
        }
        if let Some(t) = ts.get("ten_su_kien") {
            if !(1..=40).contains(&py_str(t).chars().count()) {
                return Err("tên sự kiện 1–40 ký tự".into());
            }
        }
        Ok(())
    }

    fn khoi_tao(mc: &MayChu<Self>) -> (Vec<(String, OPhongCho)>, LuatPhongCho) {
        let tong_ve = mc.ts_i64("tong_ve");
        let cu: Vec<TraLoi> = {
            let mut pc = mc.nv.pc.lock().unwrap();
            let cu = pc.nguoi.values_mut().flat_map(|ng| std::mem::take(&mut ng.cho)).collect();
            *pc = Pc::moi(mc.ts_i64("dung_luong") as f64);
            cu
        };
        for cb in cu {
            // ai đang "chờ dài" từ phiên trước: trả lời ngay
            cb(obj(json!({"ok": false, "ma_loi": "dat_lai", "ly_do": "phiên bán vé được đặt lại"})));
        }
        (
            vec![("kho".into(), OPhongCho::Kho { con: tong_ve, da_ban: 0 }), ("hang".into(), OPhongCho::Hang { cuoi: 0 })],
            LuatPhongCho { tong_ve, da_ban: 0, so_max: 0, luot: HashSet::new(), ban_qua_them: 0 },
        )
    }

    // ------------------------------------------------------------ yêu cầu
    fn xu_ly_yeu_cau(mc: &Arc<MayChu<Self>>, _khach: &KhachKetNoi, nguoi: &str, tin: &Obj, tra_loi: TraLoi) {
        match lay_str(tin, "loai") {
            Some("mua") => Self::mua(mc, nguoi, tin, tra_loi),
            _ if mc.ts_i64("phong_cho") == 0 => tra_loi(obj(json!({"ok": false, "ma_loi": "pc_tat", "ly_do": "phòng chờ đang tắt – mua trực tiếp"}))),
            Some("xep_hang") => Self::xep_hang(mc, nguoi, tra_loi),
            _ => Self::cho_luot(mc, nguoi, tra_loi),
        }
    }

    // ------------------------------------------------------------ luồng nền (100 ms): thùng token
    fn dinh_ky(mc: &Arc<MayChu<Self>>, bay: f64) {
        let con = match &*mc.kho.doc("kho") {
            OPhongCho::Kho { con, .. } => *con,
            _ => 0,
        };
        let (toc_do, dung_luong, luot_s, pc_bat) = (mc.ts_f64("toc_do"), mc.ts_i64("dung_luong") as f64, mc.ts_f64("luot_s"), mc.ts_i64("phong_cho") != 0);
        let mut tha: Vec<(TraLoi, Obj)> = Vec::new();
        let mut bao_het = false;
        {
            let mut g = mc.nv.pc.lock().unwrap();
            let pc = &mut *g;
            while pc.co_luot.front().is_some_and(|x| x.0 <= bay) {
                // thu hồi lượt quá hạn
                let (han, ten) = pc.co_luot.pop_front().unwrap();
                if let Some(ng) = pc.nguoi.get_mut(&ten) {
                    if ng.trang_thai == "luot" && ng.han == Some(han) {
                        ng.trang_thai = "het_han";
                        pc.het_han += 1;
                    }
                }
            }
            if con <= 0 && !pc.het_ve {
                // hết vé: báo mọi người đang chờ, đóng hàng
                pc.het_ve = true;
                bao_het = true;
                for ng in pc.nguoi.values_mut() {
                    if ng.trang_thai == "cho" {
                        ng.trang_thai = "het_ve";
                        tha.extend(std::mem::take(&mut ng.cho).into_iter().map(|cb| (cb, kq_het_ve())));
                    }
                }
                pc.heap.clear();
                pc.dang_cho = 0;
            } else if pc_bat && !pc.het_ve {
                pc.token = dung_luong.min(pc.token + toc_do * (bay - pc.t_token));
                while pc.token >= 1.0 {
                    let Some(Reverse((so, _, ten))) = pc.heap.pop() else { break };
                    let Some(ng) = pc.nguoi.get_mut(&ten).filter(|ng| ng.trang_thai == "cho" && ng.so == so) else { continue };
                    pc.token -= 1.0;
                    pc.dang_cho -= 1;
                    mc.so().ghi(GdPhongCho::Luot { nguoi: ten.clone() }); // sổ biết lượt TRƯỚC khi người đó thấy lượt
                    let han = bay + luot_s;
                    ng.trang_thai = "luot";
                    ng.han = Some(han);
                    let cho = std::mem::take(&mut ng.cho);
                    mc.kho.dat_o(&format!("luot:{ten}"), OPhongCho::Luot { con: true, han }); // ô mới: chưa ai chạm tới
                    pc.co_luot.push_back((han, ten));
                    pc.dang_phuc_vu = pc.dang_phuc_vu.max(so);
                    pc.da_cap_luot += 1;
                    let kq = obj(json!({"ok": true, "luot": true, "so": so, "han_ms": round_py(luot_s * 1000.0)}));
                    tha.extend(cho.into_iter().map(|cb| (cb, kq.clone())));
                }
            }
            pc.t_token = bay;
        }
        if bao_het {
            mc.thong_bao(&format!("Đã bán hết {} vé", mc.ts_i64("tong_ve")));
        }
        let co = !tha.is_empty();
        for (cb, kq) in tha {
            cb(kq);
        }
        if co {
            mc.bao_doi();
        }
    }

    // ------------------------------------------------------------ trạng thái gửi client
    fn trang_thai(mc: &MayChu<Self>) -> Obj {
        let (con, da_ban) = match &*mc.kho.doc("kho") {
            OPhongCho::Kho { con, da_ban } => (*con, *da_ban),
            _ => (0, 0),
        };
        let so_cap = match &*mc.kho.doc("hang") {
            OPhongCho::Hang { cuoi } => *cuoi,
            _ => 0,
        };
        let ban_so = mc.so().xem(|l| l.da_ban);
        let pc = mc.nv.pc.lock().unwrap();
        obj(json!({"su_kien": {"ten": mc.ts_str("ten_su_kien"), "gia_ve": mc.ts_i64("gia_ve"), "tong_ve": mc.ts_i64("tong_ve"),
                               "ve_toi_da": mc.ts_i64("ve_toi_da"), "luot_s": mc.ts_f64("luot_s")},
                   "phong_cho": mc.ts_i64("phong_cho") != 0, "con_lai": con, "da_ban": da_ban,
                   "toc_do": mc.ts_f64("toc_do"), "dung_luong": mc.ts_i64("dung_luong"), "token": pc.token as i64,
                   "dang_phuc_vu": pc.dang_phuc_vu, "so_cap": so_cap,
                   "dang_cho": pc.dang_cho, "da_cap_luot": pc.da_cap_luot, "het_han": pc.het_han,
                   "het_ve": pc.het_ve, "dang_xu_ly": pc.dang_xu_ly, "xu_ly_max": pc.xu_ly_max,
                   "gioi_han": mc.ts_i64("gioi_han_xu_ly"), "qua_tai": pc.qua_tai, "so_don": pc.so_don,
                   "ban_so": ban_so, "lich_su": pc.lich_su.iter().take(10).cloned().collect::<Vec<_>>()}))
    }

    fn tom_tat(mc: &MayChu<Self>) -> String {
        let con = match &*mc.kho.doc("kho") {
            OPhongCho::Kho { con, .. } => *con,
            _ => 0,
        };
        let so_don = mc.nv.pc.lock().unwrap().so_don;
        format!("Sổ: bán {}/{} vé, {so_don} đơn; kho còn {con}", mc.so().xem(|l| l.da_ban), mc.ts_i64("tong_ve"))
    }
}

/// Số nguyên của một trường trong đối tượng con (ví dụ tt["su_kien"]["tong_ve"]).
pub fn so_con(tt: &Obj, cha: &str, k: &str) -> i64 {
    tt.get(cha).and_then(|c| c.get(k)).and_then(Value::as_i64).unwrap_or(0)
}
