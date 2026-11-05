//! Bài mở rộng MR1.2 – Bên trong máy chủ: đồng bộ, đa luồng, nhóm luồng, bất đồng bộ và phối hợp.
//!
//! Sáu yêu cầu từ các client (H1, H2, H3 ở LAN A; H4, H6 ở LAN B) đi qua mạng mô phỏng tới máy chủ H5.
//! Mỗi yêu cầu cần đọc CSDL 30 ms (chờ I/O: luồng chờ nhưng không tốn CPU) rồi xử lý 10 ms CPU;
//! máy chủ có 2 lõi CPU. Cùng khối việc đó, máy chủ được tổ chức theo năm mô hình:
//!
//!   dong_bo      một luồng, xử lý lần lượt: yêu cầu sau phải chờ yêu cầu trước xong hẳn;
//!   da_luong     mỗi yêu cầu một luồng (tạo luồng 1 ms); luồng nhiều hơn lõi thì phải chờ lõi;
//!                cập nhật bộ đếm chung trong đoạn găng có khóa (3 ms) – luồng khác phải chờ khóa;
//!   nhom_luong   nhóm 2 luồng thợ + hàng đợi tối đa 2 yêu cầu; hàng đợi đầy thì từ chối ngay (503);
//!   bat_dong_bo  một luồng chạy vòng lặp sự kiện: I/O không chặn (nhiều I/O chờ cùng lúc), nhưng phần
//!                CPU chạy ngay trên vòng lặp nên vòng lặp bị chặn – nhịp tim 10 ms chạy muộn;
//!   phoi_hop     vòng lặp sự kiện lo I/O, đẩy phần CPU sang nhóm 2 luồng (tương đương run_in_executor
//!                của Python, tokio::task::spawn_blocking trong Rust); luồng thợ trả kết quả về vòng
//!                lặp (call_soon_threadsafe; trong Rust: gửi qua kênh mpsc/oneshot) – vòng lặp luôn rảnh,
//!                nhịp tim đúng giờ.
//!
//! Mô hình giảng dạy đã đơn giản hóa: lõi CPU cấp theo thứ tự đến, không chia lát thời gian (không
//! preemption); đoạn găng chỉ tính thời gian giữ khóa; thời gian chuyển ngữ cảnh bằng 0.
//!
//! Khác Python: Rust không lưu được hàm đóng mượn cả máy chủ lẫn Net trong hàng đợi, nên mỗi "việc"
//! (việc hẹn giờ, việc làm tiếp khi xong CPU/khóa, việc trên vòng lặp) là một giá trị của enum Viec;
//! hàm `lam` thực hiện việc đó. Thứ tự lập lịch giữ đúng như bản Python để kết quả trùng khớp.

use crate::mang::{fmt_t, App, Ev, Net, PData, Packet};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, VecDeque};

const MS: i64 = 1000;
const IO_US: i64 = 30 * MS; // đọc CSDL: chờ I/O
const CPU_US: i64 = 10 * MS; // xử lý: dùng một lõi CPU
const KHOA_US: i64 = 3 * MS; // đoạn găng (mô hình đa luồng)
const TAO_US: i64 = MS; // tạo một luồng mới (mô hình đa luồng)
const NHIP_US: i64 = 10 * MS; // chu kỳ nhịp tim của vòng lặp sự kiện
const SO_LOI: i64 = 2; // số lõi CPU của máy chủ
const SO_THO: usize = 2; // số luồng của nhóm luồng
const HANG_TOI_DA: usize = 2; // sức chứa hàng đợi của nhóm luồng
const SERVER: &str = "H5";
/// (mã yêu cầu, client, thời điểm gửi µs)
const YEU_CAU: [(&str, &str, i64); 6] = [
    ("A", "H4", 0),
    ("B", "H6", 0),
    ("C", "H4", 2 * MS),
    ("D", "H1", 0),
    ("E", "H2", 0),
    ("F", "H3", 0),
];

pub const TITLES: [(&str, &str); 5] = [
    ("dong_bo", "Đồng bộ, một luồng – xử lý lần lượt, yêu cầu sau phải chờ"),
    ("da_luong", "Đa luồng – mỗi yêu cầu một luồng, chờ lõi CPU, khóa"),
    ("nhom_luong", "Nhóm luồng – 2 luồng thợ + hàng đợi 2, đầy thì từ chối (503)"),
    ("bat_dong_bo", "Bất đồng bộ – một vòng lặp sự kiện, việc CPU chặn vòng lặp"),
    ("phoi_hop", "Phối hợp – vòng lặp sự kiện lo I/O, nhóm luồng lo việc CPU"),
];

fn ten_ngan(mode: &str) -> &'static str {
    match mode {
        "dong_bo" => "đồng bộ",
        "da_luong" => "đa luồng",
        "nhom_luong" => "nhóm luồng",
        "bat_dong_bo" => "bất đồng bộ",
        _ => "phối hợp",
    }
}

fn nhan_xet(mode: &str) -> &'static [&'static str] {
    match mode {
        "dong_bo" => &[
            "Nhận xét: một luồng làm lần lượt; khi chờ CSDL luồng bị chặn,",
            "  CPU rảnh mà không phục vụ được ai; yêu cầu sau chờ cộng dồn.",
        ],
        "da_luong" => &[
            "Nhận xét: mỗi yêu cầu một luồng nên các lần chờ I/O chồng lên",
            "  nhau; nhưng 6 luồng chỉ có 2 lõi (-: chờ lõi), đoạn găng chỉ",
            "  cho một luồng vào mỗi lúc (~: chờ khóa); mỗi luồng tốn bộ",
            "  nhớ và thời gian tạo (+).",
        ],
        "nhom_luong" => &[
            "Nhận xét: số luồng cố định nên tài nguyên được giới hạn; khi",
            "  2 thợ đều bận và hàng đợi đầy, yêu cầu bị từ chối ngay (503)",
            "  thay vì làm máy chủ quá tải.",
        ],
        "bat_dong_bo" => &[
            "Nhận xét: một luồng vẫn phục vụ nhiều yêu cầu vì I/O không",
            "  chặn (hàng I/O đang chờ có tới 6 lệnh cùng lúc); nhưng việc",
            "  CPU chạy trên vòng lặp làm mọi việc khác, kể cả nhịp tim,",
            "  phải chờ.",
        ],
        _ => &[
            "Nhận xét: vòng lặp chỉ điều phối (nhận, gửi lệnh I/O, trả lời);",
            "  việc CPU chạy trên nhóm luồng, song song trên 2 lõi, rồi báo",
            "  về vòng lặp: vòng lặp luôn rảnh, nhịp tim đúng giờ.",
        ],
    }
}

/// Ký hiệu trên biểu đồ cho các trạng thái không phải "io"/"cpu".
fn ky_hieu(kind: &str) -> char {
    match kind {
        "khoa" => '#',
        "cho_khoa" => '~',
        "cho_cpu" => '-',
        _ => '+', // "tao": tạo luồng
    }
}

/// Một luồng của máy chủ và các đoạn hoạt động của nó: (t0, t1, trạng thái, mã yêu cầu).
struct Lane {
    name: String,
    segs: Vec<(i64, i64, &'static str, &'static str)>,
    marks: Vec<i64>, // thời điểm vòng lặp chạy một việc rất ngắn (nhận, chuyển giao, nhịp tim)
}

impl Lane {
    fn seg(&mut self, t0: i64, t1: i64, kind: &'static str, label: &'static str) {
        if t1 > t0 {
            self.segs.push((t0, t1, kind, label));
        }
    }
}

/// Theo dõi một yêu cầu: thời điểm gửi, tới máy chủ, client nhận trả lời; mã trả lời.
struct Req {
    client: &'static str,
    gui: i64,
    den: i64,
    nhan: i64,
    ma: i64,
}

/// Một "việc" của máy chủ – thay cho các hàm đóng (lambda) của bản Python.
/// Tham số usize đầu tiên là chỉ số luồng (lane) hoặc chỉ số thợ, &str là mã yêu cầu.
enum Viec {
    Gui(usize),                           // client gửi yêu cầu YEU_CAU[i]
    Cpu(usize, &'static str, Box<Viec>),  // xin lõi CPU cho 10 ms xử lý, xong thì làm việc kèm theo
    CpuEnd(Box<Viec>),                    // hết 10 ms CPU
    Khoa(usize, &'static str, Box<Viec>), // vào đoạn găng
    Tha(usize, &'static str, Box<Viec>),  // hết 3 ms giữ khóa: nhả khóa
    DongBoXong(&'static str),
    DaDoc(usize, &'static str),  // đa luồng: luồng tạo xong, bắt đầu đọc CSDL
    DaXong(usize, &'static str), // đa luồng: gửi trả lời, luồng kết thúc
    NhomXong(usize, &'static str),
    Nhip(i64),                    // nhịp tim dự kiến lúc p
    Chay,                         // vòng lặp lấy việc trong hàng đợi
    VlTiep(Box<Viec>),            // việc CPU trên vòng lặp xong: làm nốt rồi chạy tiếp vòng lặp
    IoXong(&'static str),         // lệnh đọc CSDL không chặn đã xong
    Beat(i64),                    // (trên vòng lặp) nhịp tim
    VlNhan(&'static str),         // (trên vòng lặp) nhận yêu cầu, gửi lệnh đọc CSDL
    BatXong(&'static str),        // (trên vòng lặp) tính xong, trả lời
    Giao(&'static str),           // (trên vòng lặp) giao việc CPU cho nhóm luồng
    PhoiXong(usize, &'static str), // thợ tính xong
    Tra(&'static str),            // (trên vòng lặp) nhận kết quả từ nhóm luồng, trả lời
}

/// Chỉ số so sánh của một mô hình.
pub struct ChiSo {
    hoan_tat: i64,
    tb: i64,
    lon: i64,
    luong: i64,
    tu_choi: i64,
    nhip: i64, // trễ nhịp tim lớn nhất; -1 nếu không có nhịp tim
}

struct MayChu {
    mode: &'static str,
    lanes: Vec<Lane>,
    free_core: i64,                                         // số lõi CPU rảnh
    cpu_wait: VecDeque<(usize, &'static str, Viec, i64)>,   // hàng chờ lõi (FIFO)
    lock_busy: bool,
    lock_wait: VecDeque<(usize, &'static str, Viec, i64)>,  // hàng chờ khóa
    dem: i64,                                               // bộ đếm chung (đoạn găng)
    io: Vec<(i64, i64, &'static str)>,                      // I/O bất đồng bộ đang chờ
    beats: Vec<(i64, i64)>,                                 // nhịp tim [dự kiến, thực tế]
    req: Vec<Req>,
    cho_xu_ly: i64, // số yêu cầu máy chủ chưa trả lời
    so_luong: i64,
    max_luong: i64,
    tu_choi: i64,
    // đồng bộ
    chinh: usize,
    cho_ket_noi: VecDeque<&'static str>,
    ban: bool,
    // nhóm luồng / phối hợp
    tho: Vec<usize>,
    ranh: Vec<bool>,
    hang: VecDeque<(&'static str, i64)>,
    // vòng lặp sự kiện
    vl: usize,
    ready: VecDeque<(Viec, i64, &'static str)>,
    vl_ban: bool,
    // việc đã hẹn giờ: số hiệu trong Ev::App -> việc
    hen_so: u64,
    hen: HashMap<u64, Viec>,
}

impl MayChu {
    fn new(mode: &'static str) -> MayChu {
        MayChu {
            mode,
            lanes: Vec::new(),
            free_core: SO_LOI,
            cpu_wait: VecDeque::new(),
            lock_busy: false,
            lock_wait: VecDeque::new(),
            dem: 0,
            io: Vec::new(),
            beats: Vec::new(),
            req: Vec::new(),
            cho_xu_ly: YEU_CAU.len() as i64,
            so_luong: 0,
            max_luong: 0,
            tu_choi: 0,
            chinh: 0,
            cho_ket_noi: VecDeque::new(),
            ban: false,
            tho: Vec::new(),
            ranh: Vec::new(),
            hang: VecDeque::new(),
            vl: 0,
            ready: VecDeque::new(),
            vl_ban: false,
            hen_so: 0,
            hen: HashMap::new(),
        }
    }

    fn log(&self, net: &mut Net, text: &str) {
        net.log(text, SERVER);
    }

    fn lane(&mut self, name: String) -> usize {
        self.lanes.push(Lane { name, segs: Vec::new(), marks: Vec::new() });
        self.lanes.len() - 1
    }

    /// Hẹn làm việc `v` lúc t (net.at với số hiệu riêng).
    fn hen_gio(&mut self, net: &mut Net, t: i64, v: Viec) {
        self.hen_so += 1;
        self.hen.insert(self.hen_so, v);
        net.at(t, Ev::App(self.hen_so));
    }

    fn ri(label: &str) -> usize {
        YEU_CAU.iter().position(|r| r.0 == label).unwrap()
    }

    // ---------- tài nguyên dùng chung: lõi CPU và khóa ----------
    /// Chạy 10 ms CPU trên `lane`; hết lõi thì chờ theo thứ tự đến.
    fn cpu(&mut self, net: &mut Net, lane: usize, label: &'static str, done: Viec) {
        if self.free_core > 0 {
            self.free_core -= 1;
            self.cpu_start(net, lane, label, done);
        } else {
            let name = self.lanes[lane].name.clone();
            self.log(net, &format!("{name}: {label} chờ lõi CPU (cả {SO_LOI} lõi đang bận)"));
            self.cpu_wait.push_back((lane, label, done, net.now));
        }
    }

    fn cpu_start(&mut self, net: &mut Net, lane: usize, label: &'static str, done: Viec) {
        let t = net.now;
        self.lanes[lane].seg(t, t + CPU_US, "cpu", label);
        self.hen_gio(net, t + CPU_US, Viec::CpuEnd(Box::new(done)));
    }

    fn cpu_end(&mut self, net: &mut Net, done: Viec) {
        if let Some((lane, label, d2, tw)) = self.cpu_wait.pop_front() {
            // trao lõi vừa rảnh cho luồng chờ lâu nhất
            self.lanes[lane].seg(tw, net.now, "cho_cpu", label);
            self.cpu_start(net, lane, label, d2);
        } else {
            self.free_core += 1;
        }
        self.lam(net, done);
    }

    /// Vào đoạn găng: giữ khóa 3 ms để cập nhật bộ đếm chung; khóa đang bị giữ thì chờ.
    fn khoa(&mut self, net: &mut Net, lane: usize, label: &'static str, done: Viec) {
        if !self.lock_busy {
            self.lock_busy = true;
            self.khoa_giu(net, lane, label, done);
        } else {
            let name = self.lanes[lane].name.clone();
            self.log(net, &format!("{name}: {label} chờ khóa (luồng khác đang trong đoạn găng)"));
            self.lock_wait.push_back((lane, label, done, net.now));
        }
    }

    fn khoa_giu(&mut self, net: &mut Net, lane: usize, label: &'static str, done: Viec) {
        let t = net.now;
        self.lanes[lane].seg(t, t + KHOA_US, "khoa", label);
        self.hen_gio(net, t + KHOA_US, Viec::Tha(lane, label, Box::new(done)));
    }

    fn tha(&mut self, net: &mut Net, lane: usize, label: &'static str, done: Viec) {
        self.dem += 1;
        let name = self.lanes[lane].name.clone();
        self.log(net, &format!("{name}: {label} vào đoạn găng, bộ đếm chung = {}, nhả khóa", self.dem));
        if let Some((l2, lb2, d2, tw)) = self.lock_wait.pop_front() {
            self.lanes[l2].seg(tw, net.now, "cho_khoa", lb2);
            self.khoa_giu(net, l2, lb2, d2);
        } else {
            self.lock_busy = false;
        }
        self.lam(net, done);
    }

    // ---------- mạng: gửi yêu cầu, nhận yêu cầu, trả lời ----------
    fn bat_dau(&mut self, net: &mut Net) {
        for (i, &(_, client, t)) in YEU_CAU.iter().enumerate() {
            self.req.push(Req { client, gui: t, den: 0, nhan: 0, ma: 0 });
            self.hen_gio(net, t, Viec::Gui(i));
        }
        match self.mode {
            "dong_bo" => self.khoi_tao_dong_bo(),
            "da_luong" => {}
            "nhom_luong" => self.khoi_tao_nhom_luong(),
            "bat_dong_bo" => self.khoi_tao_bat_dong_bo(net),
            _ => self.khoi_tao_phoi_hop(net),
        }
    }

    fn gui(&mut self, net: &mut Net, i: usize) {
        let (label, client, _) = YEU_CAU[i];
        net.log(&format!("{client} gửi yêu cầu {label} tới máy chủ {SERVER}"), client);
        let data = PData { id: label, ..Default::default() };
        net.send(client, Packet::new("uni", "YC", client, SERVER, &format!("YC {label} {client}→{SERVER}"), 200, data));
    }

    fn tra_loi(&mut self, net: &mut Net, label: &'static str, code: i64) {
        let c = self.req[Self::ri(label)].client;
        self.cho_xu_ly -= 1;
        let text = if code == 200 { format!("TL {label} {SERVER}→{c}") } else { format!("503 {label} {SERVER}→{c}") };
        let data = PData { id: label, code, ..Default::default() };
        net.send(SERVER, Packet::new("uni", "TL", SERVER, c, &text, if code == 200 { 1000 } else { 100 }, data));
    }

    fn den(&mut self, net: &mut Net, label: &'static str) {
        match self.mode {
            "dong_bo" => self.den_dong_bo(net, label),
            "da_luong" => self.den_da_luong(net, label),
            "nhom_luong" => self.den_nhom_luong(net, label),
            _ => self.vl_nhan(net, label), // bat_dong_bo, phoi_hop
        }
    }

    // ---------- 1. đồng bộ, một luồng ----------
    fn khoi_tao_dong_bo(&mut self) {
        self.chinh = self.lane("luồng chính".to_string());
        self.so_luong = 1;
        self.max_luong = 1;
    }

    fn den_dong_bo(&mut self, net: &mut Net, label: &'static str) {
        if self.ban {
            self.cho_ket_noi.push_back(label);
            let n = self.cho_ket_noi.len();
            self.log(net, &format!("luồng chính bận → {label} chờ trong hàng đợi kết nối ({n})"));
        } else {
            self.dong_bo_xu_ly(net, label);
        }
    }

    fn dong_bo_xu_ly(&mut self, net: &mut Net, label: &'static str) {
        self.ban = true;
        let t = net.now;
        self.log(net, &format!("luồng chính xử lý {label}: đọc CSDL, luồng bị chặn {} ms", IO_US / MS));
        self.lanes[self.chinh].seg(t, t + IO_US, "io", label);
        let v = Viec::Cpu(self.chinh, label, Box::new(Viec::DongBoXong(label)));
        self.hen_gio(net, t + IO_US, v);
    }

    fn dong_bo_xong(&mut self, net: &mut Net, label: &'static str) {
        self.log(net, &format!("luồng chính xong {label} (CPU {} ms), gửi trả lời", CPU_US / MS));
        self.tra_loi(net, label, 200);
        if let Some(lb) = self.cho_ket_noi.pop_front() {
            self.dong_bo_xu_ly(net, lb);
        } else {
            self.ban = false;
        }
    }

    // ---------- 2. đa luồng: mỗi yêu cầu một luồng ----------
    fn den_da_luong(&mut self, net: &mut Net, label: &'static str) {
        let l = self.lane(format!("T{}", self.lanes.len() + 1));
        self.so_luong += 1;
        self.max_luong = self.max_luong.max(self.so_luong);
        let name = self.lanes[l].name.clone();
        self.log(net, &format!("tạo luồng {name} cho {label} ({} luồng đang chạy)", self.so_luong));
        let t = net.now;
        self.lanes[l].seg(t, t + TAO_US, "tao", label);
        self.hen_gio(net, t + TAO_US, Viec::DaDoc(l, label));
    }

    fn da_doc(&mut self, net: &mut Net, l: usize, label: &'static str) {
        let u = net.now;
        let name = self.lanes[l].name.clone();
        self.log(net, &format!("{name}: {label} đọc CSDL ({name} bị chặn, các luồng khác vẫn chạy)"));
        self.lanes[l].seg(u, u + IO_US, "io", label);
        let v = Viec::Cpu(l, label, Box::new(Viec::Khoa(l, label, Box::new(Viec::DaXong(l, label)))));
        self.hen_gio(net, u + IO_US, v);
    }

    fn da_xong(&mut self, net: &mut Net, l: usize, label: &'static str) {
        self.so_luong -= 1;
        let name = self.lanes[l].name.clone();
        self.log(net, &format!("{name}: gửi trả lời {label}, luồng kết thúc ({} luồng còn chạy)", self.so_luong));
        self.tra_loi(net, label, 200);
    }

    // ---------- 3. nhóm luồng + hàng đợi giới hạn ----------
    fn khoi_tao_nhom_luong(&mut self) {
        self.tho = (0..SO_THO).map(|i| self.lane(format!("W{}", i + 1))).collect();
        self.ranh = vec![true; SO_THO];
        self.so_luong = SO_THO as i64;
        self.max_luong = SO_THO as i64;
    }

    fn den_nhom_luong(&mut self, net: &mut Net, label: &'static str) {
        if let Some(i) = (0..SO_THO).find(|&i| self.ranh[i]) {
            self.nhom_giao(net, i, label);
            return;
        }
        if self.hang.len() < HANG_TOI_DA {
            self.hang.push_back((label, net.now));
            let n = self.hang.len();
            self.log(net, &format!("W1, W2 đều bận → {label} vào hàng đợi ({n}/{HANG_TOI_DA})"));
        } else {
            self.tu_choi += 1;
            self.log(net, &format!("hàng đợi đầy → từ chối {label} ngay (503)"));
            self.tra_loi(net, label, 503);
        }
    }

    fn nhom_giao(&mut self, net: &mut Net, i: usize, label: &'static str) {
        self.ranh[i] = false;
        let l = self.tho[i];
        let t = net.now;
        let name = self.lanes[l].name.clone();
        self.log(net, &format!("{name} nhận {label}: đọc CSDL ({name} bị chặn {} ms)", IO_US / MS));
        self.lanes[l].seg(t, t + IO_US, "io", label);
        self.hen_gio(net, t + IO_US, Viec::Cpu(l, label, Box::new(Viec::NhomXong(i, label))));
    }

    fn nhom_xong(&mut self, net: &mut Net, i: usize, label: &'static str) {
        self.log(net, &format!("W{} xong {label}, gửi trả lời", i + 1));
        self.tra_loi(net, label, 200);
        if let Some((lb, tw)) = self.hang.pop_front() {
            self.log(net, &format!("W{} lấy {lb} từ hàng đợi (đã chờ {} ms)", i + 1, fmt_t(net.now - tw)));
            self.nhom_giao(net, i, lb);
        } else {
            self.ranh[i] = true;
        }
    }

    // ---------- vòng lặp sự kiện (dùng cho 4 và 5) ----------
    fn khoi_tao_vong_lap(&mut self, net: &mut Net) {
        self.vl = self.lane("vòng lặp".to_string());
        self.hen_gio(net, 0, Viec::Nhip(0));
    }

    /// Đưa một việc vào hàng đợi của vòng lặp (dur > 0: việc CPU chạy ngay trên vòng lặp).
    fn post(&mut self, net: &mut Net, v: Viec, dur: i64, label: &'static str) {
        self.ready.push_back((v, dur, label));
        if !self.vl_ban {
            self.vl_ban = true;
            self.hen_gio(net, net.now, Viec::Chay);
        }
    }

    /// Vòng lặp lấy lần lượt từng việc; việc CPU giữ vòng lặp tới khi xong.
    fn chay(&mut self, net: &mut Net) {
        while let Some((v, dur, label)) = self.ready.pop_front() {
            if dur != 0 {
                self.cpu(net, self.vl, label, Viec::VlTiep(Box::new(v)));
                return;
            }
            self.lanes[self.vl].marks.push(net.now);
            self.lam(net, v);
        }
        self.vl_ban = false;
    }

    /// Nhịp tim mỗi 10 ms: một việc rất ngắn trên vòng lặp; đo nó chạy muộn bao nhiêu.
    fn nhip(&mut self, net: &mut Net, p: i64) {
        if self.cho_xu_ly == 0 {
            return;
        }
        self.post(net, Viec::Beat(p), 0, "");
        self.hen_gio(net, p + NHIP_US, Viec::Nhip(p + NHIP_US));
    }

    fn beat(&mut self, net: &mut Net, p: i64) {
        self.beats.push((p, net.now));
        if net.now - p >= MS {
            self.log(net, &format!("nhịp tim {} ms chạy muộn {} ms: vòng lặp bận", fmt_t(p), fmt_t(net.now - p)));
        }
    }

    fn vl_nhan(&mut self, net: &mut Net, label: &'static str) {
        self.post(net, Viec::VlNhan(label), 0, "");
    }

    fn vl_nhan_go(&mut self, net: &mut Net, label: &'static str) {
        let t = net.now;
        self.log(net, &format!("vòng lặp nhận {label}, gửi lệnh đọc CSDL không chặn"));
        self.io.push((t, t + IO_US, label));
        self.hen_gio(net, t + IO_US, Viec::IoXong(label));
    }

    // ---------- 4. bất đồng bộ: một vòng lặp, CPU trên vòng lặp ----------
    fn khoi_tao_bat_dong_bo(&mut self, net: &mut Net) {
        self.so_luong = 1;
        self.max_luong = 1;
        self.khoi_tao_vong_lap(net);
    }

    fn bat_io_xong(&mut self, net: &mut Net, label: &'static str) {
        self.log(net, &format!("CSDL xong {label} → việc CPU {} ms vào hàng đợi vòng lặp", CPU_US / MS));
        self.post(net, Viec::BatXong(label), CPU_US, label);
    }

    fn bat_xong(&mut self, net: &mut Net, label: &'static str) {
        self.log(net, &format!("vòng lặp tính xong {label} (CPU trên vòng lặp), trả lời"));
        self.tra_loi(net, label, 200);
    }

    // ---------- 5. phối hợp: vòng lặp + nhóm luồng cho việc CPU ----------
    fn khoi_tao_phoi_hop(&mut self, net: &mut Net) {
        self.so_luong = 1 + SO_THO as i64;
        self.max_luong = self.so_luong;
        self.khoi_tao_vong_lap(net);
        self.tho = (0..SO_THO).map(|i| self.lane(format!("P{}", i + 1))).collect();
        self.ranh = vec![true; SO_THO];
    }

    fn phoi_io_xong(&mut self, net: &mut Net, label: &'static str) {
        self.post(net, Viec::Giao(label), 0, "");
    }

    fn giao(&mut self, net: &mut Net, label: &'static str) {
        self.log(net, &format!("vòng lặp giao việc CPU của {label} cho nhóm luồng"));
        if let Some(i) = (0..SO_THO).find(|&i| self.ranh[i]) {
            self.phoi_chay(net, i, label);
            return;
        }
        self.hang.push_back((label, net.now));
        self.log(net, &format!("P1, P2 đều bận → việc CPU của {label} chờ trong hàng đợi"));
    }

    fn phoi_chay(&mut self, net: &mut Net, i: usize, label: &'static str) {
        self.ranh[i] = false;
        self.cpu(net, self.tho[i], label, Viec::PhoiXong(i, label));
    }

    fn phoi_xong(&mut self, net: &mut Net, i: usize, label: &'static str) {
        self.log(net, &format!("P{} tính xong {label} → gửi kết quả về vòng lặp", i + 1));
        self.post(net, Viec::Tra(label), 0, "");
        if let Some((lb, _)) = self.hang.pop_front() {
            self.phoi_chay(net, i, lb);
        } else {
            self.ranh[i] = true;
        }
    }

    fn tra(&mut self, net: &mut Net, label: &'static str) {
        self.log(net, &format!("vòng lặp nhận kết quả {label} từ nhóm luồng, trả lời"));
        self.tra_loi(net, label, 200);
    }

    // ---------- thực hiện một việc ----------
    fn lam(&mut self, net: &mut Net, v: Viec) {
        match v {
            Viec::Gui(i) => self.gui(net, i),
            Viec::Cpu(l, label, done) => self.cpu(net, l, label, *done),
            Viec::CpuEnd(done) => self.cpu_end(net, *done),
            Viec::Khoa(l, label, done) => self.khoa(net, l, label, *done),
            Viec::Tha(l, label, done) => self.tha(net, l, label, *done),
            Viec::DongBoXong(label) => self.dong_bo_xong(net, label),
            Viec::DaDoc(l, label) => self.da_doc(net, l, label),
            Viec::DaXong(l, label) => self.da_xong(net, l, label),
            Viec::NhomXong(i, label) => self.nhom_xong(net, i, label),
            Viec::Nhip(p) => self.nhip(net, p),
            Viec::Chay => self.chay(net),
            Viec::VlTiep(v) => {
                self.lam(net, *v);
                self.chay(net);
            }
            Viec::IoXong(label) => {
                if self.mode == "bat_dong_bo" {
                    self.bat_io_xong(net, label)
                } else {
                    self.phoi_io_xong(net, label)
                }
            }
            Viec::Beat(p) => self.beat(net, p),
            Viec::VlNhan(label) => self.vl_nhan_go(net, label),
            Viec::BatXong(label) => self.bat_xong(net, label),
            Viec::Giao(label) => self.giao(net, label),
            Viec::PhoiXong(i, label) => self.phoi_xong(net, i, label),
            Viec::Tra(label) => self.tra(net, label),
        }
    }

    // ---------- báo cáo ----------
    fn gantt(&self) {
        let mut end = 0;
        for l in &self.lanes {
            for s in &l.segs {
                end = end.max(s.1);
            }
        }
        for s in &self.io {
            end = end.max(s.1);
        }
        let step = MS.max((end + 46 * MS - 1) / (46 * MS) * MS); // tối đa 46 ô
        let width = (end + step - 1) / step;
        println!("Bên trong máy chủ {SERVER}, 1 ô = {} ms:", step / MS);
        println!("  CHỮ HOA dùng CPU, chữ thường bị chặn chờ I/O, + tạo luồng,");
        println!("  - chờ lõi CPU, # giữ khóa, ~ chờ khóa, · việc rất ngắn");
        for l in &self.lanes {
            let mut row = String::new();
            for i in 0..width {
                let mid = i * step + step / 2;
                let mut ch = ' ';
                for &(t0, t1, kind, label) in &l.segs {
                    if t0 <= mid && mid < t1 {
                        let c = label.chars().next().unwrap();
                        ch = match kind {
                            "cpu" => c,
                            "io" => c.to_ascii_lowercase(),
                            _ => ky_hieu(kind),
                        };
                        break;
                    }
                }
                if ch == ' ' && l.marks.iter().any(|&m| i * step <= m && m < (i + 1) * step) {
                    ch = '·';
                }
                row.push(ch);
            }
            println!("  {:<12}|{row}|", l.name);
        }
        if !self.io.is_empty() {
            let mut row = String::new();
            for i in 0..width {
                let mid = i * step + step / 2;
                let n = self.io.iter().filter(|&&(t0, t1, _)| t0 <= mid && mid < t1).count();
                if n > 0 {
                    row.push_str(&n.min(9).to_string());
                } else {
                    row.push(' ');
                }
            }
            println!("  {:<12}|{row}|", "I/O đang chờ");
        }
        let right = format!("{} ms", width * step / MS);
        println!("  {:<12} 0{right:>w$}", "", w = (width - 1).max(0) as usize);
    }

    /// Chỉ số so sánh: hoàn tất, phản hồi trung bình/lớn nhất, số luồng, từ chối, nhịp tim trễ.
    fn chi_so(&self) -> ChiSo {
        let (mut tong, mut lon, mut hoan_tat) = (0, 0, 0);
        for r in &self.req {
            let ph = r.nhan - r.gui;
            tong += ph;
            lon = lon.max(ph);
            hoan_tat = hoan_tat.max(r.nhan);
        }
        let nhip = self.beats.iter().map(|b| b.1 - b.0).max().unwrap_or(-1);
        ChiSo {
            hoan_tat,
            tb: tong / YEU_CAU.len() as i64,
            lon,
            luong: self.max_luong,
            tu_choi: self.tu_choi,
            nhip,
        }
    }

    fn report(&self) {
        println!();
        self.gantt();
        println!();
        println!(
            "  {:<3} {:<6} {:>6} {:>7} {:>8} {:>9}  Kết quả",
            "Mã", "Client", "Gửi", "Tới H5", "Nhận TL", "Phản hồi"
        );
        for (r, &(label, client, _)) in self.req.iter().zip(YEU_CAU.iter()) {
            let kq = if r.ma == 200 { "200 OK" } else { "503 từ chối" };
            println!(
                "  {label:<3} {client:<6} {:>6} {:>7} {:>8} {:>9}  {kq}",
                fmt_t(r.gui),
                fmt_t(r.den),
                fmt_t(r.nhan),
                fmt_t(r.nhan - r.gui)
            );
        }
        let s = self.chi_so();
        println!("  Hoàn tất lúc {} ms; phản hồi TB {} ms, lớn nhất {} ms", fmt_t(s.hoan_tat), fmt_t(s.tb), fmt_t(s.lon));
        println!("  Luồng tối đa {}; từ chối {}", s.luong, s.tu_choi);
        if !self.beats.is_empty() {
            let tre: Vec<i64> = self.beats.iter().map(|b| b.1 - b.0).collect();
            let tb = tre.iter().sum::<i64>() / tre.len() as i64;
            println!(
                "  Nhịp tim {} ms: {} nhịp, trễ lớn nhất {} ms, TB {} ms",
                NHIP_US / MS,
                tre.len(),
                fmt_t(s.nhip),
                fmt_t(tb)
            );
        }
        println!();
        for line in nhan_xet(self.mode) {
            println!("{line}");
        }
    }

    /// Dữ liệu cho hoạt hình HTML: các luồng, I/O đang chờ, nhịp tim.
    fn data(&self) -> Map<String, Value> {
        let mut d = Map::new();
        let lanes: Vec<Value> = self
            .lanes
            .iter()
            .map(|l| {
                let segs: Vec<Value> = l.segs.iter().map(|s| json!([s.0, s.1, s.2, s.3])).collect();
                json!([l.name, segs, l.marks])
            })
            .collect();
        d.insert("lanes".into(), Value::Array(lanes));
        if !self.io.is_empty() {
            d.insert("io".into(), Value::Array(self.io.iter().map(|s| json!([s.0, s.1, s.2])).collect()));
        }
        if !self.beats.is_empty() {
            d.insert("beats".into(), Value::Array(self.beats.iter().map(|b| json!([b.0, b.1])).collect()));
        }
        d
    }
}

impl App for MayChu {
    fn deliver(&mut self, net: &mut Net, node: &str, pkt: &Packet) {
        let label = pkt.data.id;
        let i = Self::ri(label);
        if node == SERVER {
            self.req[i].den = net.now;
            let client = self.req[i].client;
            self.log(net, &format!("yêu cầu {label} từ {client} tới {SERVER}"));
            self.den(net, label);
        } else {
            self.req[i].nhan = net.now;
            self.req[i].ma = pkt.data.code;
            let sau = fmt_t(net.now - self.req[i].gui);
            net.log(&format!("{node} nhận trả lời {label} ({}) sau {sau} ms", pkt.data.code), node);
        }
    }

    fn event(&mut self, net: &mut Net, code: u64) {
        let v = self.hen.remove(&code).expect("việc hẹn giờ không tồn tại");
        self.lam(net, v);
    }
}

/// Chạy một mô hình máy chủ trên `net`, in nhật ký và báo cáo; trả về dữ liệu hoạt hình.
pub fn chay(net: &mut Net, mode: &'static str) -> Map<String, Value> {
    net.hop_trace = false;
    println!("6 yêu cầu tới {SERVER}, mỗi yêu cầu: đọc CSDL {} ms (chờ I/O)", IO_US / MS);
    println!("  + xử lý {} ms CPU; máy chủ có {SO_LOI} lõi CPU", CPU_US / MS);
    let mut m = MayChu::new(mode);
    m.bat_dau(net);
    net.run(&mut m);
    m.report();
    m.data()
}

/// Chạy một mô hình máy chủ không in gì (net.quiet); trả về chỉ số so sánh.
pub fn chi_so(net: &mut Net, mode: &'static str) -> ChiSo {
    net.hop_trace = false;
    net.quiet = true;
    let mut m = MayChu::new(mode);
    m.bat_dau(net);
    net.run(&mut m);
    m.chi_so()
}

/// In bảng so sánh năm mô hình từ chỉ số đã đo: danh sách (mô hình, chỉ số).
pub fn so_sanh(cac_ket_qua: &[(&str, ChiSo)]) {
    println!("So sánh năm mô hình máy chủ: cùng 6 yêu cầu, mỗi yêu cầu");
    println!("  {} ms I/O + {} ms CPU, máy chủ {SO_LOI} lõi. TB, Lớn nhất: thời gian", IO_US / MS, CPU_US / MS);
    println!("  phản hồi; 503: số yêu cầu bị từ chối; Nhịp tim: trễ lớn nhất.");
    println!(
        "  {:<11} {:>5} {:>9} {:>9} {:>9} {:>3} {:>9}",
        "Mô hình", "Luồng", "Hoàn tất", "TB", "Lớn nhất", "503", "Nhịp tim"
    );
    for (mode, s) in cac_ket_qua {
        let nhip = if s.nhip >= 0 { format!("{} ms", fmt_t(s.nhip)) } else { "–".to_string() };
        println!(
            "  {:<11} {:>5} {:>9} {:>9} {:>9} {:>3} {nhip:>9}",
            ten_ngan(mode),
            s.luong,
            format!("{} ms", fmt_t(s.hoan_tat)),
            format!("{} ms", fmt_t(s.tb)),
            format!("{} ms", fmt_t(s.lon)),
            s.tu_choi
        );
    }
    println!();
    println!("Ghi nhớ: luồng giúp chồng thời gian chờ I/O nhưng tốn tài nguyên");
    println!("  và cần khóa khi dùng chung dữ liệu; bất đồng bộ phục vụ nhiều");
    println!("  I/O bằng một luồng nhưng không được làm việc CPU trên vòng");
    println!("  lặp; phối hợp vòng lặp + nhóm luồng có được cả hai ưu điểm.");
}
