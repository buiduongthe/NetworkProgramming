//! Bài mở rộng MR1.2 – Các kịch bản UDP, unicast, broadcast, multicast, anycast trên bộ mô phỏng.
//! Mỗi kịch bản là một struct cài trait App; việc hẹn giờ được đánh số bằng Ev::App(số hiệu).

use crate::mang::{App, Ev, Net, PData, Packet};

const MS: i64 = 1000; // 1 ms = 1000 µs

fn show_path(net: &Net, a: &str, b: &str) {
    let p = net.path(a, b);
    println!("  Đường {a}→{b}: {} (chi phí {})", p.join(" → "), net.cost(a, b));
}

fn msg(m: &str) -> PData {
    PData { msg: m.to_string(), ..Default::default() }
}

// ---------------- UDP ----------------
/// H2 gửi 12 datagram tới H6, cách nhau 10 ms. Liên kết R2–R3 mất gói ngẫu nhiên và có trễ
/// dao động (jitter) nên datagram có thể mất hoặc đến sai thứ tự; UDP không gửi lại.
struct Udp {
    got: Vec<i64>,
    expect: i64,
    late: i64,
}

impl App for Udp {
    fn deliver(&mut self, net: &mut Net, node: &str, pkt: &Packet) {
        let n = pkt.data.n;
        self.got.push(n);
        let note = if n == self.expect {
            "đúng thứ tự".to_string()
        } else if n > self.expect {
            let miss: Vec<String> = (self.expect..n).map(|i| format!("#{i}")).collect();
            format!("đến sớm, đang thiếu {}", miss.join(", "))
        } else {
            self.late += 1;
            "ĐẾN MUỘN – sai thứ tự".to_string()
        };
        self.expect = self.expect.max(n + 1);
        net.log(&format!("H6 nhận datagram #{n:<2} {note}"), node);
    }

    fn event(&mut self, net: &mut Net, i: u64) {
        net.log(&format!("H2 gửi datagram #{i} (200 byte)"), "H2");
        let data = PData { n: i as i64, ..Default::default() };
        net.send("H2", Packet::new("uni", "UDP", "H2", "H6", &format!("UDP #{i}"), 200, data));
    }
}

pub fn udp(net: &mut Net, loss_set: bool) {
    let l = net.link_mut("R2", "R3");
    if !loss_set {
        l.loss = 200; // mặc định 20 %
    }
    l.jitter = 15 * MS; // trễ thêm ngẫu nhiên 0..15 ms
    let loss = l.loss;
    net.hop_trace = false;
    println!("Liên kết R2–R3: mất gói {}.{} %, trễ dao động 0–15 ms", loss / 10, loss % 10);
    show_path(net, "H2", "H6");
    let mut app = Udp { got: Vec::new(), expect: 1, late: 0 };
    for i in 1..=12u64 {
        net.at((i as i64 - 1) * 10 * MS, Ev::App(i));
    }
    net.run(&mut app);
    let lost: Vec<String> = (1..=12).filter(|i| !app.got.contains(i)).map(|i| format!("#{i}")).collect();
    println!();
    let lost_txt = if lost.is_empty() { "không".to_string() } else { lost.join(", ") };
    println!("Tóm tắt UDP: gửi 12, nhận {}, mất {} ({lost_txt}), sai thứ tự {}", app.got.len(), lost.len(), app.late);
    println!("  UDP không xác nhận, không gửi lại, không sắp xếp lại:");
    println!("  ứng dụng tự xử lý (đánh số thứ tự, bỏ qua hoặc yêu cầu lại).");
}

// ---------------- Unicast ----------------
struct Unicast;

impl App for Unicast {
    fn deliver(&mut self, net: &mut Net, node: &str, pkt: &Packet) {
        net.log(&format!("{node} nhận \"{}\" từ {} (TTL còn {})", pkt.data.msg, pkt.src, pkt.ttl), node);
        if pkt.data.reply {
            let label = format!("UNI {node}→{}", pkt.src);
            net.send(node, Packet::new("uni", "UNI", node, &pkt.src, &label, 100, msg(&format!("chào {}", pkt.src))));
        }
    }

    fn event(&mut self, net: &mut Net, code: u64) {
        match code {
            1 => net.send("H1", Packet::new("uni", "UNI", "H1", "H2", "UNI H1→H2", 100, msg("xin chào H2"))),
            2 => {
                let data = PData { reply: true, ..msg("xin chào H5") };
                net.send("H1", Packet::new("uni", "UNI", "H1", "H5", "UNI H1→H5", 100, data));
            }
            _ => {
                net.log("Liên kết R1–R2 hỏng → các router tính lại đường", "R1");
                net.set_link("R1", "R2", false);
                show_path(net, "H1", "H5");
                net.send("H1", Packet::new("uni", "UNI", "H1", "H5", "UNI H1→H5", 100, msg("lần 2")));
            }
        }
    }
}

pub fn unicast(net: &mut Net) {
    println!("Bảng định tuyến của R1 (đích → chặng kế, chi phí):");
    let r1 = net.idx("R1");
    for h in ["H2", "H5"] {
        let nh = net.nodes[net.nh[r1][net.idx(h)].unwrap()].name;
        println!("  {h:<3} → {nh:<3} chi phí {}", net.cost("R1", h));
    }
    show_path(net, "H1", "H2");
    show_path(net, "H1", "H5");
    net.at(0, Ev::App(1));
    net.at(5 * MS, Ev::App(2));
    net.at(50 * MS, Ev::App(3));
    net.run(&mut Unicast);
    println!();
    println!("Tóm tắt unicast: mỗi gói chỉ tới đúng một đích; switch chỉ");
    println!("  chuyển ra đúng một cổng; router giảm TTL, chọn chặng kế theo");
    println!("  chi phí nhỏ nhất; đường đổi khi liên kết hỏng.");
}

// ---------------- Broadcast ----------------
struct Broadcast;

impl App for Broadcast {
    fn deliver(&mut self, net: &mut Net, node: &str, pkt: &Packet) {
        net.log(&format!("{node} nhận quảng bá \"{}\" từ {}", pkt.data.msg, pkt.src), node);
    }

    fn event(&mut self, net: &mut Net, code: u64) {
        if code == 1 {
            let p = Packet::new("bc", "BC", "H1", "192.168.1.255", "BC 192.168.1.255", 60, msg("Ai có 192.168.1.13?"));
            net.send("H1", p);
        } else {
            let p = Packet::new("bc", "BC", "H4", "255.255.255.255", "BC 255.255.255.255", 60, msg("DHCP DISCOVER"));
            net.send("H4", p);
        }
    }
}

pub fn broadcast(net: &mut Net) {
    net.at(0, Ev::App(1));
    net.at(10 * MS, Ev::App(2));
    net.run(&mut Broadcast);
    println!();
    println!("Tóm tắt broadcast: switch gửi tràn mọi cổng trong LAN; router");
    println!("  chặn nên H4, H5, H6 không nhận quảng bá của H1 và LAN A không");
    println!("  nhận quảng bá của H4 (miền quảng bá = một LAN).");
}

// ---------------- Multicast ----------------
const G: &str = "239.1.1.1";

enum McAction {
    Join(&'static str, bool),
    Send(i64),
}

struct Multicast {
    actions: Vec<McAction>,
}

impl App for Multicast {
    fn deliver(&mut self, net: &mut Net, node: &str, pkt: &Packet) {
        net.log(&format!("{node} nhận {} của nhóm {G}", pkt.data.msg), node);
    }

    fn event(&mut self, net: &mut Net, code: u64) {
        match self.actions[code as usize] {
            McAction::Join(h, yes) => {
                let i = net.idx(h);
                if yes {
                    net.member[i].push(G.to_string());
                } else {
                    net.member[i].retain(|g| g != G);
                }
                let what = if yes { "tham gia" } else { "rời" };
                let typ = if yes { "Report" } else { "Leave" };
                net.log(&format!("{h} {what} nhóm {G}: gửi IGMP {typ}"), h);
                let data = PData { join: yes, ..Default::default() };
                net.send(h, Packet::new("igmp", "IGMP", h, G, &format!("IGMP {typ} {G}"), 60, data));
            }
            McAction::Send(n) => {
                net.log(&format!("H1 gửi luồng #{n} tới nhóm {G}"), "H1");
                let p = Packet::new("mc", "MC", "H1", G, &format!("MC {G} #{n}"), 500, msg(&format!("luồng #{n}")));
                net.send("H1", p);
            }
        }
    }
}

pub fn multicast(net: &mut Net) {
    use McAction::*;
    let plan = [
        (0, Join("H2", true)),
        (5 * MS, Send(1)),
        (20 * MS, Join("H5", true)),
        (40 * MS, Send(2)),
        (70 * MS, Join("H6", true)),
        (75 * MS, Join("H2", false)),
        (90 * MS, Send(3)),
    ];
    let mut app = Multicast { actions: Vec::new() };
    for (t, a) in plan {
        net.at(t, Ev::App(app.actions.len() as u64));
        app.actions.push(a);
    }
    net.run(&mut app);
    println!();
    println!("Tóm tắt multicast: chỉ thành viên nhóm nhận dữ liệu; switch");
    println!("  (IGMP snooping) chỉ chuyển ra cổng có thành viên; router chỉ");
    println!("  chuyển sang LAN có thành viên; mỗi liên kết chỉ mang một bản");
    println!("  sao dù phía sau có nhiều thành viên.");
}

// ---------------- Anycast ----------------
const A: &str = "10.53.0.53";

fn best_server(net: &Net, c: &str) -> String {
    let probe = Packet::new("any", "ANY", c, A, "", 100, PData::default());
    net.target(net.idx(c), &probe).map(|i| net.nodes[i].name.to_string()).unwrap_or_else(|| "None".into())
}

struct Anycast {
    actions: Vec<(&'static str, i64)>, // (máy hỏi, số truy vấn); số 0 = H3 ngừng hoạt động
}

impl App for Anycast {
    fn deliver(&mut self, net: &mut Net, node: &str, pkt: &Packet) {
        if pkt.cast == "any" {
            net.log(&format!("{node} (phục vụ {A}) nhận \"{}\" từ {}", pkt.data.msg, pkt.src), node);
            let label = format!("UNI {node}→{}", pkt.src);
            net.send(node, Packet::new("uni", "UNI", node, &pkt.src, &label, 120, msg(&format!("trả lời từ {node}"))));
        } else {
            net.log(&format!("{node} nhận \"{}\"", pkt.data.msg), node);
        }
    }

    fn event(&mut self, net: &mut Net, code: u64) {
        let (c, n) = self.actions[code as usize];
        if n == 0 {
            net.log(&format!("H3 ngừng hoạt động → rút quảng bá {A}"), "H3");
            net.set_node("H3", false);
            let best = best_server(net, "H1");
            println!("  H1: còn H6 chi phí {} → chọn {best}", net.cost("H1", "H6"));
            return;
        }
        net.log(&format!("{c} gửi truy vấn #{n} tới {A}"), c);
        let p = Packet::new("any", "ANY", c, A, &format!("ANY {c}→{A}"), 80, msg(&format!("truy vấn #{n}")));
        net.send(c, p);
    }
}

pub fn anycast(net: &mut Net) {
    net.anycast.insert(A.to_string(), vec!["H3".into(), "H6".into()]);
    println!("Địa chỉ anycast {A} (DNS) do H3 và H6 cùng quảng bá.");
    for c in ["H1", "H5"] {
        let costs: Vec<String> = ["H3", "H6"].iter().map(|s| format!("{s} chi phí {}", net.cost(c, s))).collect();
        println!("  {c}: {} → chọn {}", costs.join(", "), best_server(net, c));
    }
    let mut app = Anycast { actions: vec![("H1", 1), ("H5", 1), ("H3", 0), ("H1", 2)] };
    for (i, t) in [0, 2 * MS, 40 * MS, 41 * MS].into_iter().enumerate() {
        net.at(t, Ev::App(i as u64));
    }
    net.run(&mut app);
    println!();
    println!("Tóm tắt anycast: cùng một địa chỉ, gói tới máy phục vụ gần nhất");
    println!("  theo chi phí định tuyến; khi H3 hỏng, truy vấn của H1 được");
    println!("  chuyển sang H6 (dự phòng tự động).");
}
