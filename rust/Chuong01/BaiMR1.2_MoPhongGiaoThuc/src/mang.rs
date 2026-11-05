//! Bài mở rộng MR1.2 – Bộ mô phỏng mạng theo sự kiện rời rạc (discrete-event simulation).
//!
//! Mô-đun này là "lõi" của bộ mô phỏng: topo mạng, hàng đợi sự kiện, định tuyến theo chi phí
//! (Dijkstra) và quy tắc chuyển tiếp của từng loại nút. Không dùng socket thật: mỗi gói tin là một
//! giá trị Rust (struct Packet), "thời gian" là đồng hồ mô phỏng tính bằng micro giây (µs).
//!
//! Topo (hai đoạn LAN nối qua ba router):
//!
//!     H1 ─┐                                   ┌─ H4
//!     H2 ─┼─ SW1 ── R1 ── R2 ── R3 ── SW2 ─┼─ H5      (R1 ── R3: đường dự phòng, chi phí 3)
//!     H3 ─┘   LAN A 192.168.1.0/24   LAN B 192.168.2.0/24  └─ H6
//!
//! Quy tắc chuyển tiếp:
//! - host: nhận gói nếu gói dành cho mình (unicast đúng tên, quảng bá trong LAN, nhóm đã tham gia,
//!   địa chỉ anycast mình phục vụ); host không chuyển tiếp gói của người khác;
//! - switch: quảng bá → gửi tràn (flood) mọi cổng trừ cổng vào; multicast → chỉ cổng có thành viên
//!   (IGMP snooping) và cổng router; unicast → đúng một cổng theo bảng định tuyến;
//! - router: chặn quảng bá (biên của miền quảng bá); unicast/anycast → giảm TTL, chọn chặng kế theo
//!   chi phí nhỏ nhất; multicast → chỉ chuyển tới các LAN có thành viên.
//!
//! Khác Python/Go: Rust không cho lưu hàm đóng (closure) mượn cả Net lẫn kịch bản trong hàng đợi,
//! nên sự kiện là một enum (Ev): gói tới nút, gói mất, hoặc "việc của kịch bản" mang một số hiệu;
//! kịch bản cài trait App để nhận gói và xử lý việc của mình.

use serde_json::{json, Value};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Định dạng thời gian mô phỏng (µs) thành mili giây, một chữ số thập phân.
pub fn fmt_t(us: i64) -> String {
    format!("{}.{}", us / 1000, us % 1000 / 100)
}

/// Bộ sinh số giả ngẫu nhiên xorshift32: cùng hạt giống (seed) → cùng dãy số ở Python, Go và Rust.
pub struct Rng {
    x: u32,
}

impl Rng {
    pub fn new(seed: i64) -> Rng {
        let x = (seed as u64).wrapping_mul(2654435761).wrapping_add(1) as u32;
        Rng { x: if x == 0 { 1 } else { x } }
    }
    pub fn next(&mut self) -> u32 {
        let mut x = self.x;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.x = x;
        x
    }
    /// Số nguyên ngẫu nhiên trong [0, n).
    pub fn below(&mut self, n: i64) -> i64 {
        self.next() as i64 % n
    }
}

pub struct Node {
    pub name: &'static str,
    pub kind: &'static str,
    pub ip: &'static str,
    pub lan: &'static str,
    pub up: bool,
    pub sent: i64,
    pub recv: i64,
    pub deliv: i64,
    pub fwd: i64,
    pub drop: i64,
}

pub struct Link {
    pub a: usize,
    #[allow(dead_code)] // giữ cho đủ mô tả liên kết; mã chỉ cần a để biết chiều
    pub b: usize,
    pub cost: i64,  // chi phí định tuyến (số bước, có trọng số)
    pub delay: i64, // trễ lan truyền (µs)
    pub bw: i64,    // băng thông: byte mỗi mili giây
    pub up: bool,
    pub loss: i64,      // xác suất mất gói, phần nghìn
    pub jitter: i64,    // trễ ngẫu nhiên thêm tối đa (µs)
    free: [i64; 2],     // thời điểm mỗi chiều (a→b, b→a) rảnh (hàng đợi FIFO)
}

/// Dữ liệu ứng dụng mang theo gói (Python dùng dict; ở đây gom các trường mà các kịch bản cần).
#[derive(Clone, Default)]
pub struct PData {
    pub flags: &'static str, // TCP: SYN, SYNACK, ACK, DATA, FIN, FINACK, FIN2, LASTACK
    pub seq: i64,
    pub ack: i64,
    #[allow(dead_code)] // độ dài dữ liệu, chỉ để mô tả gói như bản Python
    pub len: i64,
    pub t: i64, // thời điểm gửi (TCP)
    pub n: i64, // số thứ tự datagram (UDP)
    pub msg: String,
    pub reply: bool,
    pub join: bool,
    pub id: &'static str, // mã yêu cầu (mô phỏng bên trong máy chủ)
    pub code: i64,        // mã trả lời: 200 hoặc 503 (mô phỏng bên trong máy chủ)
}

#[derive(Clone)]
pub struct Packet {
    pub cast: &'static str, // uni | bc | mc | igmp | any
    pub kind: &'static str, // nhãn hiển thị: TCP, UDP, UNI, BC, MC, IGMP, ANY
    pub src: String,
    pub dst: String,
    pub label: String,
    pub size: i64,
    pub ttl: i64,
    pub data: PData,
    pub drop_on: Option<(String, String)>, // kịch bản ép mất gói trên liên kết a→b
}

impl Packet {
    pub fn new(cast: &'static str, kind: &'static str, src: &str, dst: &str, label: &str, size: i64, data: PData) -> Packet {
        Packet {
            cast,
            kind,
            src: src.to_string(),
            dst: dst.to_string(),
            label: label.to_string(),
            size,
            ttl: 64,
            data,
            drop_on: None,
        }
    }
}

/// Topo: (tên, loại, địa chỉ IP, LAN, x, y)
pub const NODES: [(&str, &str, &str, &str, i64, i64); 11] = [
    ("H1", "host", "192.168.1.11", "A", 60, 80),
    ("H2", "host", "192.168.1.12", "A", 60, 210),
    ("H3", "host", "192.168.1.13", "A", 60, 340),
    ("SW1", "switch", "-", "A", 190, 210),
    ("R1", "router", "192.168.1.1", "A", 330, 210),
    ("R2", "router", "10.0.0.2", "-", 450, 80),
    ("R3", "router", "192.168.2.1", "B", 570, 210),
    ("SW2", "switch", "-", "B", 710, 210),
    ("H4", "host", "192.168.2.14", "B", 840, 80),
    ("H5", "host", "192.168.2.15", "B", 840, 210),
    ("H6", "host", "192.168.2.16", "B", 840, 340),
];
const LAN_US: i64 = 500; // trễ lan truyền trong LAN 0,5 ms
const WAN_US: i64 = 5000; // giữa router 5 ms
const LAN_BW: i64 = 12500; // 100 Mbit/s (byte/ms)
const WAN_BW: i64 = 1250; // 10 Mbit/s
/// Liên kết: (a, b, chi phí, trễ µs, băng thông)
pub const LINKS: [(&str, &str, i64, i64, i64); 11] = [
    ("H1", "SW1", 1, LAN_US, LAN_BW),
    ("H2", "SW1", 1, LAN_US, LAN_BW),
    ("H3", "SW1", 1, LAN_US, LAN_BW),
    ("SW1", "R1", 1, LAN_US, LAN_BW),
    ("R1", "R2", 1, WAN_US, WAN_BW),
    ("R2", "R3", 1, WAN_US, WAN_BW),
    ("R1", "R3", 3, 8000, WAN_BW),
    ("R3", "SW2", 1, LAN_US, LAN_BW),
    ("SW2", "H4", 1, LAN_US, LAN_BW),
    ("SW2", "H5", 1, LAN_US, LAN_BW),
    ("SW2", "H6", 1, LAN_US, LAN_BW),
];

/// Sự kiện trong hàng đợi.
pub enum Ev {
    Arrive { node: usize, pkt: Packet, frm: usize },
    Lost { pkt: Packet, a: usize, b: usize },
    App(u64), // việc của kịch bản (số hiệu do kịch bản tự quy ước)
}

/// Tầng ứng dụng của một kịch bản.
pub trait App {
    /// Gói tới host và dành cho host đó.
    fn deliver(&mut self, net: &mut Net, node: &str, pkt: &Packet);
    /// Gói mất trên liên kết a→b; trả về true nếu kịch bản tự ghi nhận (TCP), false để Net in dòng mặc định.
    fn loss(&mut self, _net: &mut Net, _pkt: &Packet, _a: &str, _b: &str) -> bool {
        false
    }
    /// Việc đã hẹn bằng net.at(t, Ev::App(code)).
    fn event(&mut self, _net: &mut Net, _code: u64) {}
}

pub struct Net {
    rng: Rng,
    pub now: i64,
    q: BinaryHeap<Reverse<(i64, u64)>>, // (thời điểm, thứ tự thêm) – cùng thời điểm thì theo thứ tự thêm
    evs: HashMap<u64, Ev>,
    seq: u64,
    pub nodes: Vec<Node>,
    adj: Vec<Vec<(usize, usize)>>, // nút -> [(láng giềng, chỉ số liên kết)]
    pub links: Vec<Link>,
    pub anycast: HashMap<String, Vec<String>>, // địa chỉ anycast -> host phục vụ
    snoop: HashMap<(usize, String), Vec<usize>>, // (switch, nhóm) -> cổng có thành viên (IGMP snooping)
    mroute: HashMap<String, Vec<usize>>,      // nhóm -> switch (LAN) có thành viên (router biết)
    pub member: Vec<Vec<String>>,             // host -> các nhóm đã tham gia
    pub hop_trace: bool,                      // in từng bước chuyển (tắt với TCP/UDP cho gọn)
    pub quiet: bool,                          // không in nhật ký (dùng khi chỉ cần chỉ số, ví dụ lệnh so_sanh)
    pub hops: Vec<Value>,                     // dữ liệu cho hoạt hình HTML
    pub notes: Vec<Value>,
    pub changes: Vec<Value>, // thay đổi trạng thái nút/liên kết (hỏng, phục hồi)
    pub dist: Vec<Vec<Option<i64>>>, // dist[s][v]: chi phí nhỏ nhất từ s tới v
    pub nh: Vec<Vec<Option<usize>>>, // nh[s][v]: chặng kế từ s để tới v
}

impl Net {
    pub fn new(seed: i64) -> Net {
        let nodes: Vec<Node> = NODES
            .iter()
            .map(|&(name, kind, ip, lan, _, _)| Node { name, kind, ip, lan, up: true, sent: 0, recv: 0, deliv: 0, fwd: 0, drop: 0 })
            .collect();
        let idx = |n: &str| NODES.iter().position(|x| x.0 == n).unwrap();
        let mut adj = vec![Vec::new(); NODES.len()];
        let mut links = Vec::new();
        for &(a, b, cost, delay, bw) in LINKS.iter() {
            let (ia, ib) = (idx(a), idx(b));
            adj[ia].push((ib, links.len()));
            adj[ib].push((ia, links.len()));
            links.push(Link { a: ia, b: ib, cost, delay, bw, up: true, loss: 0, jitter: 0, free: [0, 0] });
        }
        let mut net = Net {
            rng: Rng::new(seed),
            now: 0,
            q: BinaryHeap::new(),
            evs: HashMap::new(),
            seq: 0,
            nodes,
            adj,
            links,
            anycast: HashMap::new(),
            snoop: HashMap::new(),
            mroute: HashMap::new(),
            member: vec![Vec::new(); NODES.len()],
            hop_trace: true,
            quiet: false,
            hops: Vec::new(),
            notes: Vec::new(),
            changes: Vec::new(),
            dist: Vec::new(),
            nh: Vec::new(),
        };
        net.compute_routes();
        net
    }

    pub fn idx(&self, name: &str) -> usize {
        self.nodes.iter().position(|n| n.name == name).unwrap_or_else(|| panic!("không có nút {name}"))
    }
    pub fn link_mut(&mut self, a: &str, b: &str) -> &mut Link {
        let (ia, ib) = (self.idx(a), self.idx(b));
        let li = self.adj[ia].iter().find(|&&(v, _)| v == ib).unwrap().1;
        &mut self.links[li]
    }

    // ---------- hàng đợi sự kiện ----------
    pub fn at(&mut self, t: i64, ev: Ev) {
        self.seq += 1;
        self.q.push(Reverse((t, self.seq)));
        self.evs.insert(self.seq, ev);
    }

    pub fn run(&mut self, app: &mut dyn App) {
        while let Some(Reverse((t, s))) = self.q.pop() {
            self.now = t;
            match self.evs.remove(&s).unwrap() {
                Ev::Arrive { node, pkt, frm } => self.arrive(app, node, pkt, frm),
                Ev::Lost { pkt, a, b } => self.lost(app, pkt, a, b),
                Ev::App(code) => app.event(self, code),
            }
        }
    }

    pub fn log(&mut self, text: &str, node: &str) {
        if !self.quiet {
            println!("{:>7} ms  {text}", fmt_t(self.now));
        }
        self.notes.push(json!([self.now, node, text]));
    }

    // ---------- định tuyến: Dijkstra từ mọi nút ----------
    pub fn compute_routes(&mut self) {
        let n = self.nodes.len();
        self.dist = vec![vec![None; n]; n];
        self.nh = vec![vec![None; n]; n];
        for s in 0..n {
            if !self.nodes[s].up {
                continue;
            }
            let mut dist: Vec<Option<i64>> = vec![None; n];
            let mut first: Vec<Option<usize>> = vec![None; n];
            let mut done = vec![false; n];
            dist[s] = Some(0);
            loop {
                // chọn nút chưa xét có khoảng cách nhỏ nhất (cùng khoảng cách: nút đứng trước trong NODES)
                let mut u: Option<usize> = None;
                for v in 0..n {
                    if let Some(dv) = dist[v] {
                        if !done[v] && u.map_or(true, |uu| dv < dist[uu].unwrap()) {
                            u = Some(v);
                        }
                    }
                }
                let Some(u) = u else { break };
                done[u] = true;
                if u != s && self.nodes[u].kind == "host" {
                    continue; // host không chuyển tiếp cho nút khác
                }
                for &(v, li) in &self.adj[u] {
                    let l = &self.links[li];
                    if !l.up || !self.nodes[v].up {
                        continue;
                    }
                    let d = dist[u].unwrap() + l.cost;
                    if dist[v].map_or(true, |dv| d < dv) {
                        dist[v] = Some(d);
                        first[v] = if u == s { Some(v) } else { first[u] };
                    }
                }
            }
            self.dist[s] = dist;
            self.nh[s] = first;
        }
    }

    pub fn path(&self, a: &str, b: &str) -> Vec<String> {
        let ib = self.idx(b);
        let mut p = vec![self.idx(a)];
        while *p.last().unwrap() != ib {
            match self.nh[*p.last().unwrap()][ib] {
                Some(x) => p.push(x),
                None => break,
            }
        }
        p.iter().map(|&i| self.nodes[i].name.to_string()).collect()
    }

    /// Đích thật của gói: với anycast là máy phục vụ gần nhất (chi phí nhỏ nhất) còn hoạt động.
    pub fn target(&self, node: usize, pkt: &Packet) -> Option<usize> {
        if pkt.cast != "any" {
            return self.nodes.iter().position(|n| n.name == pkt.dst);
        }
        let mut best: Option<(i64, usize)> = None;
        for h in self.anycast.get(&pkt.dst).cloned().unwrap_or_default() {
            let hi = self.idx(&h);
            if let Some(d) = self.dist[node][hi] {
                if self.nodes[hi].up && best.map_or(true, |b| d < b.0) {
                    best = Some((d, hi));
                }
            }
        }
        best.map(|b| b.1)
    }

    // ---------- gửi, truyền trên liên kết, nhận ----------
    /// Host `node` phát gói `pkt` vào mạng.
    pub fn send(&mut self, node: &str, pkt: Packet) {
        let i = self.idx(node);
        self.nodes[i].sent += 1;
        let nbr = self.adj[i][0].0; // host chỉ có một liên kết (tới switch)
        self.transmit(i, nbr, &pkt);
    }

    fn transmit(&mut self, a: usize, b: usize, pkt: &Packet) {
        let li = self.adj[a].iter().find(|&&(v, _)| v == b).unwrap().1;
        if !self.links[li].up || !self.nodes[b].up {
            self.nodes[a].drop += 1;
            if self.hop_trace {
                let (na, nb) = (self.nodes[a].name, self.nodes[b].name);
                self.log(&format!("{na}: liên kết tới {nb} không hoạt động, bỏ gói"), na);
            }
            return;
        }
        let mut p = pkt.clone(); // mỗi chặng một bản sao (như copy.copy của Python)
        let dir = if self.links[li].a == a { 0 } else { 1 };
        let l = &mut self.links[li];
        let start = self.now.max(l.free[dir]);
        let tx = p.size * 1000 / l.bw; // thời gian đẩy gói lên đường truyền
        l.free[dir] = start + tx;
        let mut arr = start + tx + l.delay;
        let (jitter, loss) = (l.jitter, l.loss);
        if jitter != 0 {
            arr += self.rng.below(jitter + 1);
        }
        let (na, nb) = (self.nodes[a].name, self.nodes[b].name);
        let mut lost = false;
        if p.drop_on.as_ref().is_some_and(|(x, y)| x == na && y == nb) {
            lost = true;
            p.drop_on = None;
        } else if loss != 0 && self.rng.below(1000) < loss {
            lost = true;
        }
        self.hops.push(json!([start, arr, na, nb, p.kind, p.label, lost]));
        if lost {
            self.at((start + arr) / 2, Ev::Lost { pkt: p, a, b });
        } else {
            self.at(arr, Ev::Arrive { node: b, pkt: p, frm: a });
        }
    }

    fn lost(&mut self, app: &mut dyn App, pkt: Packet, a: usize, b: usize) {
        self.nodes[a].drop += 1;
        let (na, nb) = (self.nodes[a].name, self.nodes[b].name);
        if !app.loss(self, &pkt, na, nb) {
            self.log(&format!("× {}: mất trên {na}→{nb}", pkt.label), na);
        }
    }

    fn arrive(&mut self, app: &mut dyn App, node: usize, pkt: Packet, frm: usize) {
        self.nodes[node].recv += 1;
        let (nn, nf) = (self.nodes[node].name, self.nodes[frm].name);
        if self.hop_trace {
            self.log(&format!("{nf:>3} → {nn:<3} {}", pkt.label), nn);
        }
        match self.nodes[node].kind {
            "host" => self.host_in(app, node, pkt),
            "switch" => self.switch_in(node, pkt, frm),
            _ => self.router_in(node, pkt, frm),
        }
    }

    fn host_in(&mut self, app: &mut dyn App, n: usize, pkt: Packet) {
        let name = self.nodes[n].name;
        let mine = match pkt.cast {
            "uni" => pkt.dst == name,
            "any" => self.anycast.get(&pkt.dst).is_some_and(|v| v.iter().any(|h| h == name)),
            "bc" => true, // khung quảng bá tới được thì thuộc LAN này
            "mc" => self.member[n].contains(&pkt.dst),
            _ => false,
        };
        if mine {
            self.nodes[n].deliv += 1;
            app.deliver(self, name, &pkt);
        } else {
            self.nodes[n].drop += 1;
            if self.hop_trace {
                self.log(&format!("{name} bỏ qua: gói không dành cho mình"), name);
            }
        }
    }

    fn switch_in(&mut self, n: usize, pkt: Packet, frm: usize) {
        let ports: Vec<usize> = self.adj[n].iter().map(|&(v, _)| v).collect();
        let is_router = |net: &Net, v: usize| net.nodes[v].kind == "router";
        let outs: Vec<usize> = match pkt.cast {
            "bc" => ports.iter().copied().filter(|&v| v != frm).collect(), // gửi tràn
            "igmp" => {
                let mem = self.snoop.entry((n, pkt.dst.clone())).or_default();
                if pkt.data.join && !mem.contains(&frm) {
                    mem.push(frm);
                }
                if !pkt.data.join {
                    mem.retain(|&x| x != frm);
                }
                ports.iter().copied().filter(|&v| is_router(self, v)).collect()
            }
            "mc" => {
                let mem = self.snoop.get(&(n, pkt.dst.clone())).cloned().unwrap_or_default();
                ports.iter().copied().filter(|&v| v != frm && (mem.contains(&v) || is_router(self, v))).collect()
            }
            _ => {
                let nh = self.target(n, &pkt).and_then(|t| self.nh[n][t]);
                nh.into_iter().collect()
            }
        };
        for v in outs {
            self.nodes[n].fwd += 1;
            self.transmit(n, v, &pkt);
        }
    }

    fn router_in(&mut self, n: usize, mut pkt: Packet, frm: usize) {
        let name = self.nodes[n].name;
        let lan = self.nodes[frm].lan;
        if pkt.cast == "bc" {
            self.nodes[n].drop += 1;
            self.log(&format!("{name} chặn quảng bá: không chuyển ra ngoài LAN {lan}"), name);
            return;
        }
        if pkt.cast == "igmp" {
            self.nodes[n].deliv += 1;
            let snoop_empty = self.snoop.get(&(frm, pkt.dst.clone())).map_or(true, |m| m.is_empty());
            let lans = self.mroute.entry(pkt.dst.clone()).or_default();
            if pkt.data.join && !lans.contains(&frm) {
                lans.push(frm);
                self.log(&format!("{name} ghi nhận: LAN {lan} có thành viên {}", pkt.dst), name);
            } else if !pkt.data.join && snoop_empty && lans.contains(&frm) {
                lans.retain(|&x| x != frm);
                self.log(&format!("{name} ghi nhận: LAN {lan} hết thành viên {}", pkt.dst), name);
            }
            return;
        }
        pkt.ttl -= 1;
        if pkt.ttl <= 0 {
            self.nodes[n].drop += 1;
            self.log(&format!("{name} bỏ gói: TTL = 0"), name);
            return;
        }
        let mut outs: Vec<usize> = Vec::new();
        if pkt.cast == "mc" {
            for sw in self.mroute.get(&pkt.dst).cloned().unwrap_or_default() {
                if let Some(nh) = self.nh[n][sw] {
                    if nh != frm && !outs.contains(&nh) {
                        outs.push(nh);
                    }
                }
            }
            if outs.is_empty() {
                self.log(&format!("{name} không chuyển tiếp: phía sau không có thành viên"), name);
            }
        } else {
            if let Some(nh) = self.target(n, &pkt).and_then(|t| self.nh[n][t]) {
                outs.push(nh);
            }
            if outs.is_empty() {
                self.nodes[n].drop += 1;
                self.log(&format!("{name} bỏ gói: không có đường tới {}", pkt.dst), name);
            }
        }
        for v in outs {
            self.nodes[n].fwd += 1;
            self.transmit(n, v, &pkt);
        }
    }

    // ---------- tiện ích cho kịch bản ----------
    pub fn set_link(&mut self, a: &str, b: &str, up: bool) {
        self.link_mut(a, b).up = up;
        self.changes.push(json!([self.now, "link", format!("{a}-{b}"), up]));
        self.compute_routes();
    }

    pub fn set_node(&mut self, n: &str, up: bool) {
        let i = self.idx(n);
        self.nodes[i].up = up;
        self.changes.push(json!([self.now, "node", n, up]));
        self.compute_routes();
    }

    pub fn summary(&self) {
        println!("Tổng kết theo nút (Gửi = phát ra, Nhận = gói tới, Giao = đưa lên");
        println!("ứng dụng, Ch.tiếp = số bản sao chuyển đi, Bỏ = bỏ qua/chặn/mất):");
        println!("  {:<4} {:<13} {:>4} {:>5} {:>5} {:>8} {:>4}", "Nút", "Địa chỉ", "Gửi", "Nhận", "Giao", "Ch.tiếp", "Bỏ");
        for n in &self.nodes {
            println!("  {:<4} {:<13} {:>4} {:>5} {:>5} {:>8} {:>4}", n.name, n.ip, n.sent, n.recv, n.deliv, n.fwd, n.drop);
        }
    }

    /// Chi phí từ a tới b (dùng khi in).
    pub fn cost(&self, a: &str, b: &str) -> i64 {
        self.dist[self.idx(a)][self.idx(b)].unwrap_or(-1)
    }
}
