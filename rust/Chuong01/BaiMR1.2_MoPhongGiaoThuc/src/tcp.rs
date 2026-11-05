//! Bài mở rộng MR1.2 – Kịch bản TCP: bắt tay ba bước, cửa sổ trượt, điều khiển tắc nghẽn, đóng FIN.
//!
//! TCP được rút gọn để dễ theo dõi (dựa trên TCP Reno, RFC 9293 và RFC 5681):
//! - bắt tay SYN → SYN+ACK → ACK; đóng FIN → ACK, FIN → ACK;
//! - bên nhận xác nhận tích lũy (cumulative ACK) từng đoạn, giữ lại đoạn đến sớm (ngoài thứ tự);
//! - bên gửi chỉ gửi khi số byte đang bay < cwnd (cửa sổ tắc nghẽn);
//! - khởi đầu chậm (slow start): cwnd < ssthresh thì mỗi ACK mới tăng cwnd thêm 1 MSS (gấp đôi mỗi RTT);
//! - tránh tắc nghẽn (congestion avoidance): mỗi ACK mới tăng MSS·MSS/cwnd (≈ +1 MSS mỗi RTT);
//! - 3 ACK trùng → gửi lại nhanh (fast retransmit): ssthresh = max(đang bay/2, 2 MSS), cwnd = ssthresh;
//! - hết giờ (RTO) → ssthresh = max(đang bay/2, 2 MSS), cwnd = 1 MSS, gửi lại từ đoạn chưa xác nhận,
//!   RTO tăng gấp đôi (back-off).
//! Đơn giản hóa: RTO cố định 100 ms ban đầu (RFC 6298 tính từ RTT đo được), không thổi phồng cwnd
//! trong khôi phục nhanh, không có ACK trễ, cửa sổ bên nhận đủ lớn.

use crate::mang::{fmt_t, App, Ev, Net, PData, Packet};
use serde_json::json;
use std::collections::{BTreeSet, HashMap};

pub const MSS: i64 = 1000;
const HDR: i64 = 40; // byte tiêu đề TCP/IP
const W: usize = 34; // độ rộng phần mũi tên trong sơ đồ
const EV_FIN2: u64 = 0; // việc hẹn: máy chủ gửi FIN của mình; số hiệu > 0 là bộ hẹn giờ RTO thế hệ đó

fn clen(s: &str) -> usize {
    s.chars().count() // độ dài tính theo ký tự (như len() của Python), không theo byte
}

/// Mũi tên giữa hai trục thời gian: ─ nhãn ───► hoặc ◄─── nhãn ─, mất gói thì dừng giữa đường bằng ×.
fn arrow(label: &str, right: bool, lost: bool) -> String {
    if right {
        let s = format!("─ {label} ");
        if !lost {
            format!("{s}{}►", "─".repeat(W - 1 - clen(&s)))
        } else {
            format!("{:<W$}", format!("{s}────×"))
        }
    } else {
        let s = format!(" {label} ─");
        if !lost {
            format!("◄{}{s}", "─".repeat(W - 1 - clen(&s)))
        } else {
            format!("{:>W$}", format!("×────{s}"))
        }
    }
}

fn cw(c: i64) -> String {
    format!("{}.{}", c / MSS, c % MSS * 10 / MSS)
}

pub struct TcpScenario {
    c: &'static str,
    s: &'static str,
    total: i64,
    drops: Vec<i64>, // các đoạn bị ép mất (lặp số đoạn = mất nhiều lần)
    cwnd: i64,
    ssthresh: i64,
    una: i64, // byte đầu tiên chưa được xác nhận
    nxt: i64, // byte sẽ gửi tiếp
    dup: i64,
    recovering: bool,
    rto: i64,
    gen: u64,
    timer_on: bool,
    sent: HashMap<i64, usize>, // đoạn -> số lần đã gửi
    tx: i64,
    retx: i64,
    fast: i64,
    timeouts: i64,
    rnext: i64,          // bên nhận: byte mong đợi tiếp theo
    rbuf: BTreeSet<i64>, // đoạn đến sớm
    rows: Vec<(i64, usize, String)>, // dòng sơ đồ: (thời điểm, thứ tự, văn bản)
    pub cwnd_log: Vec<(i64, i64)>,
    events: Vec<(i64, char)>, // (thời điểm, 'F' hoặc 'T') cho biểu đồ
    state: &'static str,
    t_data0: i64,
    t_done: i64,
}

impl TcpScenario {
    pub fn new(net: &mut Net, nseg: i64, drops: Vec<i64>) -> TcpScenario {
        net.hop_trace = false;
        TcpScenario {
            c: "H1",
            s: "H5",
            total: nseg * MSS,
            drops,
            cwnd: MSS,
            ssthresh: 8 * MSS,
            una: 1,
            nxt: 1,
            dup: 0,
            recovering: false,
            rto: 100_000,
            gen: 0,
            timer_on: false,
            sent: HashMap::new(),
            tx: 0,
            retx: 0,
            fast: 0,
            timeouts: 0,
            rnext: 1,
            rbuf: BTreeSet::new(),
            rows: Vec::new(),
            cwnd_log: vec![(0, MSS)],
            events: Vec::new(),
            state: "CLOSED",
            t_data0: 0,
            t_done: 0,
        }
    }

    // ---------- ghi sơ đồ ----------
    fn row(&mut self, t: i64, text: String) {
        self.rows.push((t, self.rows.len(), text));
    }

    fn pkt_row(&mut self, net: &mut Net, pkt: &Packet, t_recv: i64, note: &str, lost: bool) {
        let right = pkt.src == self.c;
        let recv = if lost { "×".to_string() } else { fmt_t(t_recv) };
        let body = arrow(&pkt.label, right, lost);
        self.row(pkt.data.t, format!("{:>6} {:>6}  |{body}|  {note}", fmt_t(pkt.data.t), recv));
        let to = if right { self.s } else { self.c }; // ghi chú cho hoạt hình HTML
        let what = if lost { format!("× {} {note}", pkt.label) } else { format!("{to} nhận {} {note}", pkt.label) };
        net.notes.push(json!([net.now, to, what.trim_end()]));
    }

    fn event_row(&mut self, net: &mut Net, text: String) {
        self.row(net.now, format!("{:>6} {:>6}  => {text}", fmt_t(net.now), ""));
        net.notes.push(json!([net.now, self.c, text]));
    }

    fn set_cwnd(&mut self, net: &Net, v: i64) {
        self.cwnd = v;
        self.cwnd_log.push((net.now, v));
    }

    // ---------- gửi ----------
    #[allow(clippy::too_many_arguments)]
    fn out(&self, net: &mut Net, frm: &str, to: &str, label: &str, flags: &'static str, seq: i64, ack: i64, length: i64, drop: bool) {
        let data = PData { flags, seq, ack, len: length, t: net.now, ..Default::default() };
        let mut p = Packet::new("uni", "TCP", frm, to, label, HDR + length, data);
        if drop {
            p.drop_on = Some(if frm == self.c { ("R2".into(), "R3".into()) } else { ("R2".into(), "R1".into()) });
        }
        net.send(frm, p);
    }

    pub fn start(&mut self, net: &mut Net) {
        self.state = "SYN_SENT";
        self.out(net, self.c, self.s, "SYN seq=0", "SYN", 0, 0, 0, false);
    }

    fn send_data(&mut self, net: &mut Net) {
        while self.nxt <= self.total && self.nxt - self.una + MSS <= self.cwnd {
            let k = (self.nxt - 1) / MSS + 1;
            let n = self.sent.get(&k).copied().unwrap_or(0) + 1;
            self.sent.insert(k, n);
            self.tx += 1;
            let again = n > 1;
            if again {
                self.retx += 1;
            }
            let label = format!("#{k} seq={}{}", self.nxt, if again { " gửi lại" } else { "" });
            let drop = self.drops.iter().filter(|&&d| d == k).count() >= n;
            self.out(net, self.c, self.s, &label, "DATA", self.nxt, 0, MSS, drop);
            self.nxt += MSS;
            if !self.timer_on {
                self.arm(net);
            }
        }
    }

    fn arm(&mut self, net: &mut Net) {
        self.gen += 1;
        self.timer_on = true;
        net.at(net.now + self.rto, Ev::App(self.gen));
    }

    fn stop_timer(&mut self) {
        self.gen += 1;
        self.timer_on = false;
    }

    fn timeout(&mut self, net: &mut Net, g: u64) {
        if g != self.gen {
            return; // bộ hẹn giờ này đã bị hủy/khởi động lại
        }
        self.timeouts += 1;
        let flight = self.nxt - self.una;
        self.ssthresh = (flight / 2).max(2 * MSS);
        self.set_cwnd(net, MSS);
        self.events.push((net.now, 'T'));
        let text = format!("HẾT GIỜ RTO={} ms: ssthresh={}, cwnd=1", self.rto / 1000, cw(self.ssthresh));
        self.event_row(net, text);
        self.rto = (self.rto * 2).min(800_000);
        self.nxt = self.una;
        self.dup = 0;
        self.recovering = false;
        self.timer_on = false;
        self.send_data(net);
    }

    fn on_ack(&mut self, net: &mut Net, pkt: &Packet) {
        let ack = pkt.data.ack;
        if ack > self.una {
            // ACK mới
            self.una = ack;
            self.nxt = self.nxt.max(ack);
            self.dup = 0;
            if self.recovering {
                self.recovering = false;
                self.set_cwnd(net, self.ssthresh);
            } else if self.cwnd < self.ssthresh {
                self.set_cwnd(net, self.cwnd + MSS); // khởi đầu chậm
            } else {
                self.set_cwnd(net, self.cwnd + MSS * MSS / self.cwnd); // tránh tắc nghẽn
            }
            self.rto = 100_000;
            let note = format!("cwnd={}", cw(self.cwnd));
            self.pkt_row(net, pkt, net.now, &note, false);
            if self.una > self.total {
                self.stop_timer();
                if self.state == "ESTABLISHED" || self.state == "SYN_SENT2" {
                    self.state = "FIN_WAIT_1";
                    self.t_done = net.now;
                    let label = format!("FIN seq={}", self.total + 1);
                    self.out(net, self.c, self.s, &label, "FIN", self.total + 1, 0, 0, false);
                }
                return;
            }
            self.arm(net);
            self.send_data(net);
        } else if ack == self.una {
            // ACK trùng
            self.dup += 1;
            let note = format!("trùng {}", self.dup);
            self.pkt_row(net, pkt, net.now, &note, false);
            if self.dup == 3 {
                self.fast += 1;
                self.ssthresh = ((self.nxt - self.una) / 2).max(2 * MSS);
                self.set_cwnd(net, self.ssthresh);
                self.recovering = true;
                self.events.push((net.now, 'F'));
                let text = format!("3 ACK TRÙNG: gửi lại nhanh, ssthresh=cwnd={}", cw(self.cwnd));
                self.event_row(net, text);
                let k = (self.una - 1) / MSS + 1;
                *self.sent.get_mut(&k).unwrap() += 1;
                self.tx += 1;
                self.retx += 1;
                let drop = self.drops.iter().filter(|&&d| d == k).count() >= self.sent[&k];
                let label = format!("#{k} seq={} gửi lại", self.una);
                self.out(net, self.c, self.s, &label, "DATA", self.una, 0, MSS, drop);
            }
        }
    }

    // ---------- kết quả ----------
    pub fn report(&mut self) {
        println!("Sơ đồ thời gian – trình tự ({} khách, {} máy chủ; thời gian ms):", self.c, self.s);
        println!("{:>6} {:>6}  {:<w$}{}", "gửi", "nhận", self.c, self.s, w = W + 1);
        self.rows.sort();
        for (_, _, text) in &self.rows {
            println!("{}", text.trim_end());
        }
        println!();
        self.chart();
        let dur = self.t_done - self.t_data0;
        println!();
        println!("Tóm tắt TCP: {} đoạn ({} byte), truyền {} lần, gửi lại {}", self.total / MSS, self.total, self.tx, self.retx);
        println!("  ({} lần gửi lại nhanh, {} lần hết giờ); xong sau {} ms,", self.fast, self.timeouts, fmt_t(dur));
        println!("  thông lượng ≈ {} kbit/s (liên kết chậm nhất 10 Mbit/s,", self.total * 8 * 1000 / dur.max(1));
        println!("  RTT ≈ 26 ms): cửa sổ nhỏ và mất gói làm thông lượng thấp hơn");
        println!("  nhiều so với băng thông.");
    }

    fn chart(&self) {
        let cols: usize = 52;
        let t_end = self.cwnd_log.last().unwrap().0;
        let mut vals = Vec::new();
        for c in 0..cols as i64 {
            let t = t_end * (c + 1) / cols as i64;
            let mut v = MSS;
            for &(tt, cv) in &self.cwnd_log {
                if tt <= t {
                    v = cv;
                }
            }
            vals.push(v / MSS);
        }
        let top = *vals.iter().max().unwrap();
        println!("Biểu đồ cwnd (đơn vị MSS = {MSS} byte) theo thời gian:");
        for y in (1..=top).rev() {
            let line: String = vals.iter().map(|&v| if v >= y { '█' } else { ' ' }).collect();
            println!("{y:>4} │{line}");
        }
        println!("     └{}", "─".repeat(cols));
        let mut marks = vec![' '; cols];
        for &(t, m) in &self.events {
            let i = ((t * cols as i64 / t_end.max(1)) as usize).min(cols - 1);
            marks[i] = m;
        }
        let marks: String = marks.into_iter().collect();
        println!("      {}", marks.trim_end());
        let right = format!("{} ms", fmt_t(t_end));
        println!("      0{:>w$}", right, w = cols - 1);
        println!("      F = gửi lại nhanh (3 ACK trùng), T = hết giờ RTO");
    }
}

impl App for TcpScenario {
    fn deliver(&mut self, net: &mut Net, node: &str, pkt: &Packet) {
        let f = pkt.data.flags;
        let now = net.now;
        if node == self.s {
            // ---- phía máy chủ H5
            match f {
                "SYN" => {
                    self.pkt_row(net, pkt, now, "", false);
                    self.out(net, self.s, self.c, "SYN+ACK seq=0 ack=1", "SYNACK", 0, 1, 0, false);
                }
                "ACK" if self.state == "SYN_SENT2" => {
                    self.pkt_row(net, pkt, now, "ESTABLISHED", false);
                    self.state = "ESTABLISHED";
                }
                "DATA" => {
                    let seq = pkt.data.seq;
                    if seq == self.rnext {
                        self.rnext += MSS;
                        while self.rbuf.remove(&self.rnext) {
                            // đoạn đến sớm nay đã liền mạch
                            self.rnext += MSS;
                        }
                    } else if seq > self.rnext {
                        self.rbuf.insert(seq);
                    }
                    self.pkt_row(net, pkt, now, "", false);
                    let label = format!("ACK {}", self.rnext);
                    self.out(net, self.s, self.c, &label, "ACK", 0, self.rnext, 0, false);
                }
                "FIN" => {
                    self.pkt_row(net, pkt, now, "CLOSE_WAIT", false);
                    let label = format!("ACK {}", pkt.data.seq + 1);
                    self.out(net, self.s, self.c, &label, "FINACK", 0, pkt.data.seq + 1, 0, false);
                    net.at(now + 1000, Ev::App(EV_FIN2));
                }
                "LASTACK" => self.pkt_row(net, pkt, now, "CLOSED", false),
                _ => {}
            }
            return;
        }
        // ---- phía khách H1
        match f {
            "SYNACK" => {
                self.pkt_row(net, pkt, now, "", false);
                self.state = "SYN_SENT2";
                self.out(net, self.c, self.s, "ACK ack=1", "ACK", 0, 1, 0, false);
                self.t_data0 = now;
                self.send_data(net);
            }
            "ACK" => self.on_ack(net, pkt),
            "FINACK" => self.pkt_row(net, pkt, now, "FIN_WAIT_2", false),
            "FIN2" => {
                self.pkt_row(net, pkt, now, "TIME_WAIT", false);
                self.out(net, self.c, self.s, "ACK ack=2", "LASTACK", 0, 2, 0, false);
            }
            _ => {}
        }
    }

    fn loss(&mut self, net: &mut Net, pkt: &Packet, a: &str, b: &str) -> bool {
        self.pkt_row(net, pkt, 0, &format!("MẤT {a}→{b}"), true);
        true
    }

    fn event(&mut self, net: &mut Net, code: u64) {
        if code == EV_FIN2 {
            self.out(net, self.s, self.c, "FIN seq=1", "FIN2", 1, 0, 0, false);
        } else {
            self.timeout(net, code);
        }
    }
}
