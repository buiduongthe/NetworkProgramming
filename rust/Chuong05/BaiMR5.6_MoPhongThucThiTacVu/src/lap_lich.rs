//! Bài mở rộng MR5.6 – Bộ lập lịch mô phỏng: đơn luồng/đa luồng × đồng bộ/bất đồng bộ.
//!
//! Thời gian rời rạc, 1 nhịp = 100 ms. Mỗi tác vụ T1, T2, T3 có thời lượng D giây, gồm ba bước:
//!   CPU (D/4) → chờ I/O (D/2, ví dụ đọc mạng hay đĩa) → CPU (phần còn lại).
//! Tùy chọn:
//!   - phụ thuộc: T3 chỉ bắt đầu khi T1 và T2 đã xong (`JoinHandle::join` / `tokio::join!`);
//!   - chia: bước CPU cuối của T1 được chia đôi, hai nửa chạy cùng lúc trên hai luồng rồi hợp lại
//!     (chỉ chia khi bước đó dài ít nhất 2 nhịp);
//!   - việc khẩn U: đến khi các luồng đang bận (mặc định lúc 0,3 s), cần 0,5 s CPU, ưu tiên cao;
//!   - số lõi CPU và lát thời gian (quantum) của bộ lập lịch hệ điều hành.
//!
//! Bốn chế độ:
//!   don_dong_bo      một luồng, lời gọi chặn: làm lần lượt theo thứ tự đến (T1, T2, T3 rồi U); khi chờ I/O
//!                    luồng đứng yên, không làm được việc khác;
//!   da_dong_bo       mỗi tác vụ một luồng hệ điều hành (`std::thread::spawn`), lời gọi chặn: các luồng tranh
//!                    nhau lõi CPU; hệ điều hành chia lát thời gian và cho luồng ưu tiên cao CHEN NGANG ngay
//!                    (preemption); T3 chờ T1, T2 bằng join; nửa sau phần CPU của T1 chạy trên luồng phụ T1b;
//!   don_bat_dong_bo  một luồng chạy vòng lặp sự kiện (như tokio `current_thread`): bước CPU chạy tới khi xong
//!                    (đa nhiệm hợp tác), chờ I/O không giữ luồng nên các I/O chồng lên nhau; hàng việc FIFO;
//!   da_bat_dong_bo   nhóm luồng thợ (số thợ = số lõi) + bất đồng bộ (như tokio `multi_thread`): mỗi bước CPU
//!                    giao cho thợ rảnh đầu tiên, nên sau một lần chờ I/O cùng một tác vụ có thể chạy tiếp trên
//!                    MỘT LUỒNG KHÁC; hai nửa của T1 chạy song song trên hai thợ.
//! Mỗi nhịp gồm bốn bước theo thứ tự cố định: (1) tác vụ đến, (2) các luồng/tác vụ tiến sang thao tác kế tiếp,
//! (3) lập lịch (cấp lõi, giao việc cho thợ), (4) chạy một nhịp và ghi lại. Kết quả tất định: cùng tham số →
//! cùng biểu đồ, cùng số liệu ở mọi ngôn ngữ (bản Rust in trùng từng ký tự với bản Python).
//!
//! Mô phỏng không tạo luồng thật: "luồng", "lõi", "thợ" chỉ là dữ liệu; mỗi luồng là một chỉ số trong
//! `Vec<Luong>` (thay cho tham chiếu đối tượng của bản Python).

use std::collections::{HashMap, VecDeque};

/// Mã chế độ, theo đúng thứ tự hiển thị.
pub const CHE_DO: [&str; 4] = ["don_dong_bo", "da_dong_bo", "don_bat_dong_bo", "da_bat_dong_bo"];

/// Tên đầy đủ của chế độ.
pub fn ten_che_do(m: &str) -> &'static str {
    match m {
        "don_dong_bo" => "Đơn luồng – đồng bộ (Single-threaded Sync)",
        "da_dong_bo" => "Đa luồng – đồng bộ (Multi-threaded Sync)",
        "don_bat_dong_bo" => "Đơn luồng – bất đồng bộ (Single-threaded Async)",
        _ => "Đa luồng – bất đồng bộ (Multi-threaded Async)",
    }
}

/// Tên ngắn (cột "Chế độ" của bảng so sánh).
pub fn ten_ngan(m: &str) -> &'static str {
    match m {
        "don_dong_bo" => "đơn, đồng bộ",
        "da_dong_bo" => "đa, đồng bộ",
        "don_bat_dong_bo" => "đơn, bất đ.bộ",
        _ => "đa, bất đ.bộ",
    }
}

/// Trạng thái một ô (một làn, một nhịp).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrangThai {
    Cpu,
    Io,
    SanSang,
    Cho,
    Ranh,
}

/// Một ô của biểu đồ: trạng thái và nhãn tác vụ ("T1", "U"…; rỗng khi rảnh).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct O {
    pub tt: TrangThai,
    pub nhan: String,
}

impl O {
    fn ranh() -> O {
        O { tt: TrangThai::Ranh, nhan: String::new() }
    }
}

/// Việc khẩn: 0,5 s CPU.
pub const U_CPU: i64 = 5;

/// Chia lấy phần nguyên làm tròn xuống (như `//` của Python, kể cả số âm).
fn chia(a: i64, b: i64) -> i64 {
    a.div_euclid(b)
}

/// Số nhịp (100 ms) → chuỗi giây, một chữ số thập phân.
pub fn giay(nhip: i64) -> String {
    format!("{}.{}", nhip.div_euclid(10), nhip.rem_euclid(10))
}

/// Giây → nhịp như `int(round(x * 10))` của Python: làm tròn nửa về số chẵn (banker's rounding),
/// ví dụ 0.25 s → 2,5 nhịp → 2; 0.35 s → 3,5000000000000004 → 4.
pub fn sang_nhip(giay: f64) -> i64 {
    (giay * 10.0).round_ties_even() as i64
}

#[derive(Clone, Debug)]
pub struct ThamSo {
    pub d: [i64; 3], // nhịp
    pub so_loi: usize,
    pub lat: i64,
    pub phu_thuoc: bool,
    pub chia: bool,
    pub khan: bool,
    pub t_khan: i64,
}

impl ThamSo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(t1: f64, t2: f64, t3: f64, so_loi: i64, lat: f64, phu_thuoc: bool, chia: bool, khan: bool, t_khan: f64) -> ThamSo {
        ThamSo {
            d: [sang_nhip(t1), sang_nhip(t2), sang_nhip(t3)],
            so_loi: so_loi.max(1) as usize,
            lat: sang_nhip(lat).max(1),
            phu_thuoc,
            chia,
            khan,
            t_khan: sang_nhip(t_khan),
        }
    }
}

impl Default for ThamSo {
    fn default() -> Self {
        ThamSo::new(4.0, 3.0, 2.0, 2, 0.5, true, true, true, 0.3)
    }
}

/// Loại bước của tác vụ: cpu | io | chia (CPU chia đôi chạy song song).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoaiBuoc {
    Cpu,
    Io,
    Chia,
}

/// Một tác vụ.
#[derive(Clone, Debug)]
pub struct Viec {
    pub ten: String,
    pub buoc: Vec<(LoaiBuoc, i64)>,
    pub uu_tien: i32,
    pub den: i64,
    pub phu_thuoc: Vec<String>,
    pub bat_dau: i64, // nhịp đầu tiên dùng CPU (-1: chưa)
    pub xong: i64,    // nhịp hoàn tất (-1: chưa)
    k: usize,         // (bất đồng bộ) bước đang làm
    con_cho: i32,     // (bất đồng bộ) số nửa của bước chia còn chưa xong
    da_chay: bool,    // (bất đồng bộ) đã bắt đầu
}

impl Viec {
    fn new(ten: String, buoc: Vec<(LoaiBuoc, i64)>, uu_tien: i32, den: i64, phu_thuoc: Vec<String>) -> Viec {
        Viec { ten, buoc, uu_tien, den, phu_thuoc, bat_dau: -1, xong: -1, k: 0, con_cho: 0, da_chay: false }
    }
}

pub fn tao_viec(ts: &ThamSo) -> Vec<Viec> {
    let mut ds = Vec::new();
    for i in 0..3 {
        let n = ts.d[i];
        let (c1, io) = (chia(n, 4), chia(n, 2));
        let c2 = n - c1 - io;
        // < 2 nhịp: không chia được (một nửa sẽ dài 0 nhịp)
        let cuoi = if ts.chia && i == 0 && c2 >= 2 { LoaiBuoc::Chia } else { LoaiBuoc::Cpu };
        let buoc: Vec<(LoaiBuoc, i64)> = [(LoaiBuoc::Cpu, c1), (LoaiBuoc::Io, io), (cuoi, c2)].into_iter().filter(|b| b.1 > 0).collect();
        let dep = if ts.phu_thuoc && i == 2 { vec!["T1".to_string(), "T2".to_string()] } else { vec![] };
        ds.push(Viec::new(format!("T{}", i + 1), buoc, 0, 0, dep));
    }
    if ts.khan {
        ds.push(Viec::new("U".into(), vec![(LoaiBuoc::Cpu, U_CPU)], 1, ts.t_khan, vec![]));
    }
    ds
}

/// Kết quả một lần chạy: các làn (tên, ô), số I/O đang chờ mỗi nhịp, nhật ký, tác vụ.
#[derive(Clone, Debug)]
pub struct KetQua {
    pub che_do: String,
    pub ts: ThamSo,
    pub lan: Vec<(String, Vec<O>)>,
    pub io: Vec<usize>,
    pub nhat_ky: Vec<(i64, String)>,
    pub viec: Vec<Viec>,
    pub chen_ngang: i64,
    pub het_lat: i64,
    pub tong: i64,
}

impl KetQua {
    fn new(che_do: &str, ts: &ThamSo) -> KetQua {
        KetQua { che_do: che_do.into(), ts: ts.clone(), lan: vec![], io: vec![], nhat_ky: vec![], viec: vec![], chen_ngang: 0, het_lat: 0, tong: 0 }
    }
}

fn noi_va(ds: &[String]) -> String {
    ds.join(" và ")
}

// ======================================================================== đồng bộ (luồng hệ điều hành)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LoaiOp {
    Cpu,
    Io,
    ChoViec,
    TaoPhu,
    ChoPhu,
    Ghi,
    Xong,
    Het,
}

/// Một thao tác trong chương trình của luồng.
#[derive(Clone, Debug)]
struct Op {
    loai: LoaiOp,
    n: i64,                // số nhịp còn lại (cpu, io) hoặc số nhịp của nửa sau (tao_phu)
    phu_thuoc: Vec<String>, // cho_viec: các tác vụ phải chờ
    phu: Option<usize>,    // cho_phu: luồng phụ phải chờ
    text: String,          // ghi: dòng nhật ký
    nhan: String,          // tên tác vụ
    viec: usize,           // chỉ số tác vụ
}

impl Op {
    fn moi(loai: LoaiOp, n: i64, v: &Viec, iv: usize) -> Op {
        Op { loai, n, phu_thuoc: vec![], phu: None, text: String::new(), nhan: v.ten.clone(), viec: iv }
    }
}

struct Luong {
    ten: String,
    uu_tien: i32,
    ct: VecDeque<Op>, // chương trình: các thao tác chưa làm
    op: Option<Op>,   // thao tác đang làm
    ket_thuc: bool,
    trang_thai: TrangThai,
    lat_da_chay: i64,
    o: Vec<O>,
}

struct DongBo {
    ts: ThamSo,
    nhieu: bool,
    kq: KetQua,
    viec: Vec<Viec>,
    luong: Vec<Luong>,
    san_sang: Vec<(usize, u64)>, // (luồng, số thứ tự vào hàng)
    dang_chay: Vec<usize>,
    stt: u64,
    hang: VecDeque<usize>, // đơn luồng: tác vụ đã đến, chờ luồng chính (FIFO)
    chinh: Option<usize>,
    t: i64,
}

impl DongBo {
    fn new(ts: &ThamSo, nhieu: bool) -> DongBo {
        DongBo {
            ts: ts.clone(),
            nhieu,
            kq: KetQua::new(if nhieu { "da_dong_bo" } else { "don_dong_bo" }, ts),
            viec: tao_viec(ts),
            luong: vec![],
            san_sang: vec![],
            dang_chay: vec![],
            stt: 0,
            hang: VecDeque::new(),
            chinh: None,
            t: 0,
        }
    }

    fn log(&mut self, text: String) {
        self.kq.nhat_ky.push((self.t, text));
    }

    /// Chương trình của tác vụ v: [chờ phụ thuộc], các bước, xong.
    fn ct_viec(&self, iv: usize, mot_luong: bool) -> VecDeque<Op> {
        let v = &self.viec[iv];
        let mut ct = VecDeque::new();
        if !v.phu_thuoc.is_empty() && !mot_luong {
            let mut op = Op::moi(LoaiOp::ChoViec, 0, v, iv);
            op.phu_thuoc = v.phu_thuoc.clone();
            ct.push_back(op);
        }
        for &(loai, n) in &v.buoc {
            match loai {
                LoaiBuoc::Chia => {
                    let (a, b) = (chia(n + 1, 2), chia(n, 2));
                    if mot_luong {
                        let mut g = Op::moi(LoaiOp::Ghi, 0, v, iv);
                        g.text = format!("{}: chia CPU cuối 2 nửa, một luồng nên làm lần lượt", v.ten);
                        ct.push_back(g);
                        ct.push_back(Op::moi(LoaiOp::Cpu, a, v, iv));
                        ct.push_back(Op::moi(LoaiOp::Cpu, b, v, iv));
                    } else {
                        ct.push_back(Op::moi(LoaiOp::TaoPhu, b, v, iv));
                        ct.push_back(Op::moi(LoaiOp::Cpu, a, v, iv));
                        ct.push_back(Op::moi(LoaiOp::ChoPhu, 0, v, iv));
                    }
                }
                LoaiBuoc::Cpu => ct.push_back(Op::moi(LoaiOp::Cpu, n, v, iv)),
                LoaiBuoc::Io => ct.push_back(Op::moi(LoaiOp::Io, n, v, iv)),
            }
        }
        ct.push_back(Op::moi(LoaiOp::Xong, 0, v, iv));
        ct
    }

    fn them_luong(&mut self, ten: String, uu_tien: i32, ct: VecDeque<Op>) -> usize {
        let o = (0..self.t).map(|_| O::ranh()).collect();
        self.luong.push(Luong { ten, uu_tien, ct, op: None, ket_thuc: false, trang_thai: TrangThai::Ranh, lat_da_chay: 0, o });
        self.luong.len() - 1
    }

    fn vao_san_sang(&mut self, l: usize) {
        self.stt += 1;
        self.san_sang.push((l, self.stt));
    }

    fn roi_loi(&mut self, l: usize) {
        if let Some(i) = self.dang_chay.iter().position(|&x| x == l) {
            self.dang_chay.remove(i);
        }
    }

    fn da_xong_het(&self, ds: &[String]) -> bool {
        ds.iter().all(|d| self.viec.iter().any(|v| &v.ten == d && v.xong >= 0))
    }

    /// Bước 2: luồng l làm các thao tác tức thời và sang thao tác kế tiếp nếu thao tác hiện tại đã xong.
    fn tien(&mut self, l: usize) {
        loop {
            if let Some(op) = self.luong[l].op.clone() {
                match op.loai {
                    LoaiOp::Cpu | LoaiOp::Io if op.n > 0 => return,
                    LoaiOp::ChoViec => {
                        if !self.da_xong_het(&op.phu_thuoc) {
                            return;
                        }
                        let ten = self.luong[l].ten.clone();
                        self.log(format!("{}: {} đã xong (join), {} chạy tiếp", ten, noi_va(&op.phu_thuoc), op.nhan));
                    }
                    LoaiOp::ChoPhu => {
                        let phu = op.phu.expect("cho_phu luôn có luồng phụ");
                        if !self.luong[phu].ket_thuc {
                            return;
                        }
                        let ten = self.luong[l].ten.clone();
                        let ten_phu = self.luong[phu].ten.clone();
                        self.log(format!("{}: hai nửa của {} đã xong (join {})", ten, op.nhan, ten_phu));
                    }
                    _ => {}
                }
            }
            if self.luong[l].ct.is_empty() {
                if !self.nhieu && Some(l) == self.chinh && !self.hang.is_empty() {
                    let iv = self.hang.pop_front().unwrap();
                    self.log(format!("Luồng chính bắt đầu {}", self.viec[iv].ten));
                    self.luong[l].ct = self.ct_viec(iv, true);
                } else {
                    self.luong[l].op = None;
                    self.roi_loi(l);
                    if self.nhieu || self.viec.iter().all(|v| v.den <= self.t) {
                        self.luong[l].ket_thuc = true;
                    }
                    self.luong[l].trang_thai = TrangThai::Ranh;
                    return;
                }
            }
            let op = self.luong[l].ct.pop_front().unwrap();
            self.luong[l].op = Some(op.clone());
            if op.loai == LoaiOp::Cpu {
                let dang = self.dang_chay.contains(&l);
                if !dang && self.san_sang.iter().all(|s| s.0 != l) {
                    self.vao_san_sang(l);
                }
                self.luong[l].trang_thai = if dang { TrangThai::Cpu } else { TrangThai::SanSang };
                return;
            }
            self.roi_loi(l);
            match op.loai {
                LoaiOp::Io => {
                    self.luong[l].trang_thai = TrangThai::Io;
                    return;
                }
                LoaiOp::ChoViec => {
                    self.luong[l].trang_thai = TrangThai::Cho;
                    if !self.da_xong_het(&op.phu_thuoc) {
                        let ten = self.luong[l].ten.clone();
                        self.log(format!("{}: {} chờ {} xong (join)", ten, op.nhan, noi_va(&op.phu_thuoc)));
                    }
                }
                LoaiOp::TaoPhu => {
                    let v = &self.viec[op.viec];
                    let mut ct = VecDeque::new();
                    ct.push_back(Op::moi(LoaiOp::Cpu, op.n, v, op.viec));
                    ct.push_back(Op::moi(LoaiOp::Het, 0, v, op.viec));
                    let uu_tien = self.luong[l].uu_tien;
                    let phu = self.them_luong(format!("Luồng {}b", op.nhan), uu_tien, ct);
                    let ten_phu = self.luong[phu].ten.clone();
                    self.log(format!("{}: chia CPU cuối 2 nửa, {} chạy nửa sau", op.nhan, ten_phu));
                    self.luong[l].ct[1].phu = Some(phu); // thao tác cho_phu chờ đúng luồng phụ này
                }
                LoaiOp::ChoPhu => self.luong[l].trang_thai = TrangThai::Cho,
                LoaiOp::Ghi => self.log(op.text.clone()),
                LoaiOp::Xong => {
                    self.viec[op.viec].xong = self.t;
                    let s = format!("{} xong lúc {} s", self.viec[op.viec].ten, giay(self.t));
                    self.log(s);
                }
                LoaiOp::Het | LoaiOp::Cpu => {}
            }
        }
    }

    /// Luồng sẵn sàng tốt nhất: ưu tiên cao, rồi đến trước.
    fn tot_nhat(&self) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (i, s) in self.san_sang.iter().enumerate() {
            let k = (-self.luong[s.0].uu_tien, s.1);
            match best {
                Some(b) if (-self.luong[self.san_sang[b].0].uu_tien, self.san_sang[b].1) <= k => {}
                _ => best = Some(i),
            }
        }
        best
    }

    /// Bước 3: bộ lập lịch hệ điều hành cấp lõi CPU cho các luồng sẵn sàng.
    fn lap_lich(&mut self) {
        let lat = self.ts.lat;
        // a. hết lát thời gian: nhường lõi nếu có luồng ưu tiên bằng hoặc cao hơn đang chờ
        for r in self.dang_chay.clone() {
            let ut = self.luong[r].uu_tien;
            if self.luong[r].lat_da_chay >= lat && self.san_sang.iter().any(|s| self.luong[s.0].uu_tien >= ut) {
                self.roi_loi(r);
                self.luong[r].lat_da_chay = 0;
                self.vao_san_sang(r);
                self.kq.het_lat += 1;
                let s = format!("{} hết lát thời gian {} s, nhường lõi", self.luong[r].ten, giay(lat));
                self.log(s);
            }
        }
        // b. lõi còn trống: cấp cho luồng sẵn sàng tốt nhất
        while self.dang_chay.len() < self.ts.so_loi && !self.san_sang.is_empty() {
            let i = self.tot_nhat().unwrap();
            let (l, _) = self.san_sang.remove(i);
            self.luong[l].lat_da_chay = 0;
            self.dang_chay.push(l);
        }
        // c. chen ngang: luồng sẵn sàng ưu tiên cao hơn luồng đang chạy thì lấy lõi ngay
        while !self.san_sang.is_empty() {
            let i = self.tot_nhat().unwrap();
            let s = self.san_sang[i].0;
            let ut = self.luong[s].uu_tien;
            // nạn nhân: ưu tiên thấp nhất, rồi lát đã chạy nhiều nhất, rồi vị trí đầu
            let mut nan: Option<usize> = None; // vị trí trong dang_chay
            for (j, &r) in self.dang_chay.iter().enumerate() {
                if self.luong[r].uu_tien >= ut {
                    continue;
                }
                let k = (self.luong[r].uu_tien, -self.luong[r].lat_da_chay, j);
                match nan {
                    Some(b) => {
                        let rb = self.dang_chay[b];
                        if k < (self.luong[rb].uu_tien, -self.luong[rb].lat_da_chay, b) {
                            nan = Some(j);
                        }
                    }
                    None => nan = Some(j),
                }
            }
            let Some(j) = nan else { break };
            let r = self.dang_chay[j];
            self.san_sang.remove(i);
            self.luong[s].lat_da_chay = 0;
            self.dang_chay[j] = s;
            self.luong[r].lat_da_chay = 0;
            self.vao_san_sang(r);
            self.kq.chen_ngang += 1;
            let txt = format!("{} CHEN NGANG: {} tạm dừng, nhường lõi", self.luong[s].ten, self.luong[r].ten);
            self.log(txt);
        }
        for l in 0..self.luong.len() {
            let la_cpu = matches!(&self.luong[l].op, Some(op) if op.loai == LoaiOp::Cpu && op.n > 0);
            if la_cpu {
                self.luong[l].trang_thai = if self.dang_chay.contains(&l) { TrangThai::Cpu } else { TrangThai::SanSang };
            }
        }
    }

    fn chay(mut self) -> KetQua {
        loop {
            // 1. tác vụ đến
            for iv in 0..self.viec.len() {
                if self.viec[iv].den != self.t {
                    continue;
                }
                let (ten, uu_tien) = (self.viec[iv].ten.clone(), self.viec[iv].uu_tien);
                if uu_tien != 0 {
                    self.log(format!("{ten} (việc khẩn, ưu tiên cao) đến"));
                }
                if self.nhieu {
                    let ct = self.ct_viec(iv, false);
                    let l = self.them_luong(format!("Luồng {ten}"), uu_tien, ct);
                    if uu_tien != 0 {
                        let s = format!("tạo {}", self.luong[l].ten);
                        self.log(s);
                    }
                } else {
                    self.hang.push_back(iv);
                    match self.chinh {
                        None => self.chinh = Some(self.them_luong("Luồng chính".into(), 0, VecDeque::new())),
                        Some(c) => {
                            if uu_tien != 0 && (self.luong[c].op.is_some() || !self.luong[c].ct.is_empty()) {
                                self.log(format!("{ten} xếp hàng: luồng chính bận, không chen ngang được"));
                            }
                        }
                    }
                }
            }
            // 2. tiến (theo thứ tự tạo luồng; luồng mới tạo trong bước này cũng được tiến). Làm hai lượt để
            //    luồng đang join thấy ngay luồng có số thứ tự lớn hơn vừa kết thúc trong cùng nhịp.
            for _ in 0..2 {
                let mut i = 0;
                while i < self.luong.len() {
                    if !self.luong[i].ket_thuc {
                        self.tien(i);
                    }
                    i += 1;
                }
            }
            if self.luong.iter().all(|l| l.ket_thuc) && self.viec.iter().all(|v| v.den <= self.t) {
                break;
            }
            // 3. lập lịch
            self.lap_lich();
            // 4. chạy một nhịp
            let t = self.t;
            for l in self.luong.iter_mut() {
                let o = match (&mut l.op, l.ket_thuc) {
                    (None, _) | (_, true) => O::ranh(),
                    (Some(op), false) => match l.trang_thai {
                        TrangThai::Cpu => {
                            op.n -= 1;
                            l.lat_da_chay += 1;
                            let v = &mut self.viec[op.viec];
                            if v.bat_dau < 0 {
                                v.bat_dau = t;
                            }
                            O { tt: TrangThai::Cpu, nhan: op.nhan.clone() }
                        }
                        TrangThai::Io => {
                            op.n -= 1;
                            O { tt: TrangThai::Io, nhan: op.nhan.clone() }
                        }
                        tt => O { tt, nhan: op.nhan.clone() },
                    },
                };
                l.o.push(o);
            }
            self.t += 1;
        }
        self.kq.tong = self.t;
        self.kq.lan = self.luong.iter().map(|l| (l.ten.clone(), l.o.clone())).collect();
        self.kq.viec = self.viec;
        self.kq
    }
}

// ======================================================================== bất đồng bộ (vòng lặp / nhóm thợ)
struct BatDongBo {
    nhieu: bool,
    kq: KetQua,
    viec: Vec<Viec>,
    ten_tho: Vec<String>,
    tho: Vec<Option<(usize, i64, bool)>>, // việc đang chạy trên mỗi thợ: (tác vụ, nhịp còn lại, là nửa?)
    o: Vec<Vec<O>>,
    hang: VecDeque<(usize, i64, bool)>, // các bước CPU sẵn sàng, FIFO
    cho_io: Vec<(usize, i64)>,          // (tác vụ, nhịp còn lại)
    cuoi_tho: HashMap<usize, usize>,    // tác vụ -> thợ chạy bước CPU gần nhất
    t: i64,
}

impl BatDongBo {
    fn new(ts: &ThamSo, nhieu: bool) -> BatDongBo {
        let so_tho = if nhieu { ts.so_loi } else { 1 };
        let ten_tho = if nhieu { (0..so_tho).map(|i| format!("Thợ {}", i + 1)).collect() } else { vec!["Vòng lặp".to_string()] };
        BatDongBo {
            nhieu,
            kq: KetQua::new(if nhieu { "da_bat_dong_bo" } else { "don_bat_dong_bo" }, ts),
            viec: tao_viec(ts),
            ten_tho,
            tho: vec![None; so_tho],
            o: vec![vec![]; so_tho],
            hang: VecDeque::new(),
            cho_io: vec![],
            cuoi_tho: HashMap::new(),
            t: 0,
        }
    }

    fn log(&mut self, text: String) {
        self.kq.nhat_ky.push((self.t, text));
    }

    /// Tác vụ v bắt đầu bước v.k (hoặc hoàn tất nếu hết bước).
    fn bat_dau_buoc(&mut self, iv: usize) {
        let k = self.viec[iv].k;
        if k >= self.viec[iv].buoc.len() {
            self.viec[iv].xong = self.t;
            let s = format!("{} xong lúc {} s", self.viec[iv].ten, giay(self.t));
            self.log(s);
            for w in 0..self.viec.len() {
                // tác vụ đang chờ (await) các tác vụ vừa xong
                let vw = &self.viec[w];
                if !vw.da_chay && vw.den <= self.t && !vw.phu_thuoc.is_empty() && self.du_phu_thuoc(w) {
                    let s = format!("{} đã xong → {} bắt đầu (await)", noi_va(&vw.phu_thuoc), vw.ten);
                    self.log(s);
                    self.khoi_dong(w);
                }
            }
            return;
        }
        let (loai, n) = self.viec[iv].buoc[k];
        match loai {
            LoaiBuoc::Cpu => self.hang.push_back((iv, n, false)),
            LoaiBuoc::Io => self.cho_io.push((iv, n)),
            LoaiBuoc::Chia => {
                // hai nửa vào hàng cùng lúc
                self.viec[iv].con_cho = 2;
                self.hang.push_back((iv, chia(n + 1, 2), true));
                self.hang.push_back((iv, chia(n, 2), true));
                let ten = self.viec[iv].ten.clone();
                if self.tho.len() > 1 {
                    self.log(format!("{ten}: chia CPU cuối 2 nửa, chạy song song trên 2 thợ"));
                } else {
                    self.log(format!("{ten}: chia CPU cuối 2 nửa, một luồng nên làm lần lượt"));
                }
            }
        }
    }

    fn du_phu_thuoc(&self, iv: usize) -> bool {
        self.viec[iv].phu_thuoc.iter().all(|d| self.viec.iter().any(|w| &w.ten == d && w.xong >= 0))
    }

    fn khoi_dong(&mut self, iv: usize) {
        self.viec[iv].da_chay = true;
        self.viec[iv].k = 0;
        self.bat_dau_buoc(iv);
    }

    fn chay(mut self) -> KetQua {
        let so_tho = self.tho.len();
        loop {
            // 1. tác vụ đến
            for iv in 0..self.viec.len() {
                if self.viec[iv].den != self.t {
                    continue;
                }
                let ten = self.viec[iv].ten.clone();
                if self.viec[iv].uu_tien != 0 {
                    self.log(format!("{ten} (việc khẩn) đến – xếp hàng, không chen ngang được"));
                }
                if !self.viec[iv].phu_thuoc.is_empty() && !self.du_phu_thuoc(iv) {
                    let s = format!("{ten} chờ {} xong (await), không giữ luồng nào", noi_va(&self.viec[iv].phu_thuoc));
                    self.log(s);
                } else {
                    self.khoi_dong(iv);
                }
            }
            // 2. tiến: bước CPU xong (theo thứ tự thợ), rồi I/O xong (theo thứ tự bắt đầu chờ)
            for i in 0..so_tho {
                if let Some((iv, 0, nua)) = self.tho[i] {
                    self.tho[i] = None;
                    if nua {
                        self.viec[iv].con_cho -= 1;
                        if self.viec[iv].con_cho == 0 {
                            let s = format!("{}: hai nửa đã xong, hợp kết quả", self.viec[iv].ten);
                            self.log(s);
                            self.viec[iv].k += 1;
                            self.bat_dau_buoc(iv);
                        }
                    } else {
                        self.viec[iv].k += 1;
                        self.bat_dau_buoc(iv);
                    }
                }
            }
            for x in self.cho_io.clone() {
                if x.1 == 0 {
                    if let Some(p) = self.cho_io.iter().position(|y| *y == x) {
                        self.cho_io.remove(p);
                    }
                    self.viec[x.0].k += 1;
                    self.bat_dau_buoc(x.0);
                }
            }
            if self.viec.iter().all(|v| v.xong >= 0) {
                break;
            }
            // 3. giao việc: thợ rảnh (theo thứ tự) lấy bước CPU đứng đầu hàng
            for i in 0..so_tho {
                if self.tho[i].is_none() && !self.hang.is_empty() {
                    let (iv, n, nua) = self.hang.pop_front().unwrap();
                    self.tho[i] = Some((iv, n, nua));
                    if let Some(&truoc) = self.cuoi_tho.get(&iv) {
                        if self.nhieu && truoc != i && !nua {
                            let s = format!("{} chạy tiếp trên {} (trước đó ở {})", self.viec[iv].ten, self.ten_tho[i], self.ten_tho[truoc]);
                            self.log(s);
                        }
                    }
                    self.cuoi_tho.insert(iv, i);
                }
            }
            // 4. chạy một nhịp
            for i in 0..so_tho {
                match &mut self.tho[i] {
                    None => self.o[i].push(O::ranh()),
                    Some(x) => {
                        x.1 -= 1;
                        let v = &mut self.viec[x.0];
                        if v.bat_dau < 0 {
                            v.bat_dau = self.t;
                        }
                        self.o[i].push(O { tt: TrangThai::Cpu, nhan: v.ten.clone() });
                    }
                }
            }
            self.kq.io.push(self.cho_io.len());
            for x in self.cho_io.iter_mut() {
                x.1 -= 1;
            }
            self.t += 1;
        }
        self.kq.tong = self.t;
        self.kq.lan = self.ten_tho.iter().cloned().zip(self.o).collect();
        self.kq.viec = self.viec;
        self.kq
    }
}

/// Chạy mô phỏng một chế độ.
pub fn chay(che_do: &str, ts: &ThamSo) -> KetQua {
    match che_do {
        "don_dong_bo" => DongBo::new(ts, false).chay(),
        "da_dong_bo" => DongBo::new(ts, true).chay(),
        "don_bat_dong_bo" => BatDongBo::new(ts, false).chay(),
        _ => BatDongBo::new(ts, true).chay(),
    }
}

// ======================================================================== chỉ số và biểu đồ văn bản
/// Hoàn tất (nhịp), độ trễ phản hồi của U (nhịp, -1 nếu không có U), số luồng, % lõi bận.
#[derive(Clone, Copy, Debug)]
pub struct ChiSo {
    pub tong: i64,
    pub u_cho: i64,
    pub luong: usize,
    pub ban: i64,
    pub chen: i64,
    pub lat: i64,
}

pub fn chi_so(kq: &KetQua) -> ChiSo {
    let ban = kq.lan.iter().flat_map(|(_, o)| o.iter()).filter(|c| c.tt == TrangThai::Cpu).count() as i64;
    let u = kq.viec.iter().find(|v| v.ten == "U");
    ChiSo {
        tong: kq.tong,
        u_cho: u.map(|u| u.bat_dau - u.den).unwrap_or(-1),
        luong: kq.lan.len(),
        ban: chia(ban * 100, (kq.ts.so_loi as i64 * kq.tong).max(1)),
        chen: kq.chen_ngang,
        lat: kq.het_lat,
    }
}

fn ky_tu(c: &O) -> char {
    match c.tt {
        TrangThai::Cpu => {
            if c.nhan == "U" {
                'U'
            } else {
                c.nhan.chars().nth(1).unwrap_or('?') // T1 → '1'
            }
        }
        TrangThai::Io => '~',
        TrangThai::SanSang => '-',
        TrangThai::Cho => '.',
        TrangThai::Ranh => ' ',
    }
}

/// Biểu đồ Gantt văn bản, 1 ô = 1 nhịp (0,1 s); dài hơn `rong` nhịp thì xuống dòng thành nhiều khối.
/// (Độ rộng `{:<11}` của Rust đếm theo ký tự Unicode như Python, nên "Luồng T1" căn lề đúng.)
pub fn gantt(kq: &KetQua, rong: i64) -> Vec<String> {
    let mut dong = Vec::new();
    let mut dau = 0;
    while dau < kq.tong {
        let cuoi = kq.tong.min(dau + rong);
        let (a, b) = (dau as usize, cuoi as usize);
        for (ten, o) in &kq.lan {
            let s: String = o[a.min(o.len())..b.min(o.len())].iter().map(ky_tu).collect();
            dong.push(format!("  {ten:<11}|{s}|"));
        }
        if !kq.io.is_empty() {
            let s: String = kq.io[a..b.min(kq.io.len())]
                .iter()
                .map(|&n| if n > 0 { char::from_digit(n.min(9) as u32, 10).unwrap() } else { ' ' })
                .collect();
            dong.push(format!("  {:<11}|{s}|", "I/O chờ"));
        }
        let nhan = format!("{} s", giay(cuoi));
        let gd = giay(dau);
        let w = (cuoi - dau - gd.chars().count() as i64 + 2).max(0) as usize;
        dong.push(format!("  {:<11} {gd}{nhan:>w$}", ""));
        dau += rong;
    }
    dong
}
