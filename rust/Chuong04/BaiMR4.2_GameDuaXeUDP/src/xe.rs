//! Client đua xe UDP: người lái (bàn phím) hoặc bot.
//!
//! Mỗi khi nhận ảnh chụp tick k (chỉ nhận ảnh MỚI HƠN ảnh đang có), client:
//!   1. đối chiếu: so vị trí xe mình mà client đã dự đoán với vị trí server gửi (lệch khi đầu vào bị mất);
//!   2. dự đoán phía client: từ ảnh chụp k, chạy lại vật lý với các đầu vào của chính mình để biết xe
//!      sẽ ở đâu ở tick k+1 (xe khác thì ngoại suy theo vận tốc cuối);
//!   3. chọn đầu vào cho tick k+2 và gửi kèm 2 đầu vào trước đó (dư thừa).
//! Khi một ảnh chụp bị mất, khung hình của tick đó được vẽ bằng dự đoán – người chơi không thấy xe "đứng hình".
//! Người lái: gõ g (ga), n (nhả), p (phanh) rồi Enter; đầu vào giữ nguyên tới khi đổi.

use crate::dua_xe::{bot_input, fmt2, input_text, step, style, track_line, LEAD, START, TICK_MS, TRACK, WINDOW};
use crate::{die, Args};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{self, BufRead, ErrorKind, Write};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// Ảnh chụp trạng thái server gửi.
#[derive(Clone)]
struct Snap {
    race: i64,
    tick: i64,
    phase: String,
    names: Vec<String>,
    cars: Vec<[i64; 4]>, // [id, x, v, done]
    rank: Vec<(String, i64)>,
}

impl Snap {
    fn parse(p: &Value) -> Option<Snap> {
        let ints = |v: &Value| -> Option<[i64; 4]> {
            let a = v.as_array()?;
            Some([a.first()?.as_i64()?, a.get(1)?.as_i64()?, a.get(2)?.as_i64()?, a.get(3)?.as_i64()?])
        };
        Some(Snap {
            race: p["race"].as_i64()?,
            tick: p["tick"].as_i64()?,
            phase: p["phase"].as_str()?.to_string(),
            names: p["names"].as_array()?.iter().filter_map(|n| n.as_str().map(String::from)).collect(),
            cars: p["cars"].as_array()?.iter().filter_map(ints).collect(),
            rank: p["rank"]
                .as_array()
                .map(|r| r.iter().filter_map(|e| Some((e[0].as_str()?.to_string(), e[1].as_i64()?))).collect())
                .unwrap_or_default(),
        })
    }

    fn car(&self, cid: i64) -> [i64; 4] {
        *self.cars.iter().find(|c| c[0] == cid).expect("không có xe")
    }
}

/// Những gì client biết về cuộc đua: ảnh chụp mới nhất, đầu vào của mình, thống kê.
struct RaceView {
    me: String,
    my_id: Option<i64>,
    last_tick: i64,
    snap: Option<Snap>,
    my_inputs: BTreeMap<i64, i64>, // tick -> đầu vào mình đã chọn
    got: i64,
    stale: i64,
    missed: i64,
    max_gap: i64,
    err_n: i64, // dự đoán xe mình sai (server đã dùng đầu vào khác)
    err_max: i64,
    oerr_n: i64, // ngoại suy xe khác sai sau một quãng hụt ảnh chụp
    oerr_max: i64,
}

impl RaceView {
    fn new(me: &str) -> RaceView {
        RaceView { me: me.into(), my_id: None, last_tick: -1, snap: None, my_inputs: BTreeMap::new(), got: 0, stale: 0, missed: 0, max_gap: 0, err_n: 0, err_max: 0, oerr_n: 0, oerr_max: 0 }
    }

    /// Đầu vào mình dùng ở tick t (nếu chưa chọn thì giữ đầu vào gần nhất – giống server).
    fn input_at(&self, t: i64) -> i64 {
        if t < 0 {
            return 0;
        }
        self.my_inputs.range(..=t).next_back().map(|(_, &v)| v).unwrap_or(0)
    }

    /// Dự đoán phía client: chạy lại vật lý từ ảnh chụp với chính các đầu vào của mình.
    fn predict_own(&self, snap: &Snap, t_end: i64) -> (i64, i64, i64) {
        let [_, mut x, mut v, mut done] = snap.car(self.my_id.unwrap());
        for t in snap.tick + 1..=t_end {
            if t < START || done >= 0 {
                continue;
            }
            let (nx, nv, _) = step(x, v, self.input_at(t));
            x = nx;
            v = nv;
            if x >= TRACK {
                x = TRACK;
                v = 0;
                done = t - START + 1;
            }
        }
        (x, v, done)
    }

    /// Xe khác: ngoại suy tuyến tính (dead reckoning) theo vận tốc trong ảnh chụp.
    fn predict_other(&self, snap: &Snap, cid: i64, t_end: i64) -> (i64, i64, i64) {
        let [_, x, v, done] = snap.car(cid);
        let ticks = t_end - snap.tick.max(START - 1);
        if done >= 0 || ticks <= 0 {
            return (x, v, done);
        }
        (TRACK.min(x + v * ticks), v, done)
    }

    /// Trạng thái mọi xe ở tick T theo hiểu biết hiện tại (ảnh chụp thật nếu có, không thì dự đoán).
    fn state_at(&self, t: i64) -> Vec<(String, i64, i64, i64)> {
        let s = self.snap.as_ref().unwrap();
        let mut out = Vec::new();
        for (cid, name) in s.names.iter().enumerate() {
            let cid = cid as i64;
            let (x, v, done) = if s.tick == t {
                let c = s.car(cid);
                (c[1], c[2], c[3])
            } else if Some(cid) == self.my_id {
                self.predict_own(s, t)
            } else {
                self.predict_other(s, cid, t)
            };
            out.push((name.clone(), x, v, done));
        }
        out
    }

    fn frame(&self, t: i64, predicted: bool) -> Vec<String> {
        let mut lines = vec![format!("[{}] Khung t={}{}", self.me, t - START, if predicted { " (dự đoán: thiếu ảnh chụp)" } else { "" })];
        for (name, x, v, done) in self.state_at(t) {
            let letter = name.chars().next().unwrap_or('?').to_uppercase().next().unwrap_or('?');
            let tail = if done >= 0 { format!(" về đích ({done} tick)") } else { format!(" x={} v={}", fmt2(x), fmt2(v)) };
            lines.push(format!("  {letter}|{}{tail}", track_line(letter, x)));
        }
        lines
    }

    /// Nhận một ảnh chụp; trả false nếu là ảnh cũ (đến muộn) – bị bỏ, ảnh mới nhất thắng.
    fn accept(&mut self, s: &Snap) -> bool {
        let k = s.tick;
        if k <= self.last_tick {
            self.stale += 1;
            return false;
        }
        if self.my_id.is_none() {
            self.my_id = s.names.iter().position(|n| *n == self.me).map(|i| i as i64);
        }
        if self.last_tick >= 0 && k - self.last_tick > 1 {
            let gap = k - self.last_tick - 1;
            self.missed += gap;
            self.max_gap = self.max_gap.max(gap);
            if k >= START {
                // xe khác: ngoại suy trong quãng hụt lệch bao nhiêu?
                let old = self.snap.as_ref().unwrap();
                for cid in 0..s.names.len() as i64 {
                    if Some(cid) != self.my_id {
                        let err = (self.predict_other(old, cid, k).0 - s.car(cid)[1]).abs();
                        if err != 0 {
                            self.oerr_n += 1;
                            self.oerr_max = self.oerr_max.max(err);
                        }
                    }
                }
            }
        }
        if let Some(old) = self.snap.as_ref().filter(|_| k >= START) {
            // đối chiếu dự đoán với sự thật của server
            let px = self.predict_own(old, k).0;
            let err = (px - s.car(self.my_id.unwrap())[1]).abs();
            if err != 0 {
                self.err_n += 1;
                self.err_max = self.err_max.max(err);
            }
        }
        self.got += 1;
        true
    }

    /// Chọn đầu vào cho tick k+LEAD, trả gói đầu vào cần gửi (mang kèm WINDOW-1 đầu vào trước đó).
    fn decide(&mut self, choose: impl Fn(i64, i64, i64) -> i64) -> Value {
        let k = self.snap.as_ref().unwrap().tick;
        let t_new = k + LEAD;
        let (x, v, done) = self.predict_own(self.snap.as_ref().unwrap(), t_new - 1);
        let inp = choose(x, v, done);
        let start = self.my_inputs.keys().next_back().map(|t| t + 1).unwrap_or(t_new);
        for t in start..t_new {
            // tick bị bỏ qua (do mất ảnh chụp): giữ đầu vào cũ
            let keep = self.input_at(t);
            self.my_inputs.insert(t, keep);
        }
        self.my_inputs.insert(t_new, inp);
        let window: Vec<i64> = (0..WINDOW).map(|j| self.input_at(t_new - j)).collect();
        json!({"t": "in", "seq": t_new, "in": window})
    }
}

fn send(sock: &UdpSocket, addr: SocketAddr, obj: &Value) {
    let _ = sock.send_to(serde_json::to_string(obj).unwrap().as_bytes(), addr);
}

/// Nhận một datagram JSON; None nếu hết thời gian chờ hoặc gói hỏng.
fn recv_json(sock: &UdpSocket) -> Result<Option<Value>, ()> {
    let mut buf = [0u8; 4096];
    match sock.recv_from(&mut buf) {
        Ok((n, _)) => Ok(serde_json::from_slice(&buf[..n]).ok()),
        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Err(()),
        Err(_) => Ok(None), // ví dụ ConnectionReset trên Windows: bỏ qua
    }
}

/// Gửi join (lặp lại mỗi 250 ms vì UDP có thể mất) tới khi nhận welcome.
fn join(sock: &UdpSocket, addr: SocketAddr, me: &str) -> Value {
    sock.set_read_timeout(Some(Duration::from_millis(250))).ok();
    loop {
        send(sock, addr, &json!({"t": "join", "name": me}));
        if let Ok(Some(p)) = recv_json(sock) {
            if p["t"] == "welcome" {
                return p;
            }
        }
    }
}

/// Trạng thái người lái: đầu vào hiện tại do luồng bàn phím cập nhật.
struct Human {
    inp: Arc<AtomicI64>,
    t_last: Instant,
}

fn race_once(sock: &UdpSocket, addr: SocketAddr, me: &str, bot: Option<(i64, i64)>, khung: i64, mut human: Option<&mut Human>) {
    let w = join(sock, addr, me);
    let race = w["race"].as_i64().unwrap_or(0);
    println!("[{me}] Vào cuộc đua {race} (cần {} xe; mất gói giả lập {} %, đảo thứ tự {} %)", w["need"], w["mat"], w["dao"]);
    let mut view = RaceView::new(me);
    let mut next_frame = START;
    sock.set_read_timeout(Some(Duration::from_millis(if human.is_some() { 50 } else { 3000 }))).ok();
    let mut last_draw: Option<Instant> = None;
    loop {
        let p = match recv_json(sock) {
            Ok(p) => p,
            Err(()) => {
                if human.is_none() {
                    println!("[{me}] Mất liên lạc với server");
                    return;
                }
                None
            }
        };
        let snap = p.filter(|p| p["t"] == "snap").and_then(|p| Snap::parse(&p)).filter(|s| s.race == race);
        if let Some(s) = snap {
            if view.accept(&s) {
                let k = s.tick;
                if let Some(h) = human.as_deref_mut() {
                    h.t_last = Instant::now();
                }
                if human.is_none() && khung > 0 && s.phase == "DUA" && view.snap.is_some() {
                    while next_frame <= k {
                        // khung cho mọi tick cần vẽ đến hết tick k
                        let out = if next_frame == k {
                            view.snap = Some(s.clone());
                            view.frame(k, false)
                        } else {
                            view.frame(next_frame, true) // dùng ảnh chụp cũ + dự đoán
                        };
                        println!("{}", out.join("\n"));
                        next_frame += khung;
                    }
                }
                view.snap = Some(s.clone());
                view.last_tick = k;
                if s.phase == "XONG" {
                    let rank: Vec<String> = s
                        .rank
                        .iter()
                        .enumerate()
                        .map(|(i, (n, d))| format!("{}. {n} {} tick", i + 1, if *d >= 0 { d.to_string() } else { "-".into() }))
                        .collect();
                    println!("[{me}] Kết quả: {}", rank.join(", "));
                    println!("[{me}] Ảnh chụp: nhận {}, hụt {} tick (dài nhất {}), bỏ {} ảnh cũ", view.got, view.missed, view.max_gap, view.stale);
                    println!(
                        "[{me}] Dự đoán xe mình: lệch {} lần (lớn nhất {} ô); ngoại suy xe khác: lệch {} lần (lớn nhất {} ô)",
                        view.err_n,
                        fmt2(view.err_max),
                        view.oerr_n,
                        fmt2(view.oerr_max)
                    );
                    return;
                }
                let pkt = match (&human, bot) {
                    (Some(h), _) => {
                        let cur = h.inp.load(Ordering::Relaxed);
                        view.decide(move |_, _, _| cur)
                    }
                    (None, Some(st)) => view.decide(move |x, v, done| if done < 0 { bot_input(x, v, st) } else { 0 }),
                    (None, None) => unreachable!(),
                };
                send(sock, addr, &pkt);
            }
        }
        if let Some(h) = human.as_deref() {
            if view.snap.is_some() && last_draw.is_none_or(|t| t.elapsed() >= Duration::from_millis(100)) {
                // vẽ lại 10 lần/giây
                last_draw = Some(Instant::now());
                let est = view.last_tick + (h.t_last.elapsed().as_millis() as u64 / TICK_MS) as i64;
                if est >= START {
                    println!("\x1b[H\x1b[2J{}", view.frame(est, est != view.last_tick).join("\n"));
                    println!("Đầu vào: {}  (g = ga, n = nhả, p = phanh, Enter)", input_text(h.inp.load(Ordering::Relaxed)));
                } else {
                    println!("\x1b[H\x1b[2JXuất phát sau {:.1} s", (START - est) as f64 * TICK_MS as f64 / 1000.0);
                }
                io::stdout().flush().ok();
            }
        }
    }
}

pub fn run(args: Args) {
    let me = args.pos.first().cloned().unwrap_or_else(|| die("Cách dùng: xe <tên> [host] [port] [--bot lieu|can_than] [--khung 10] [--so-van 1]"));
    let host = args.pos.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let port = args.port(2, 9200);
    let bot = args.opt.get("bot").map(|b| style(b).unwrap_or_else(|| die(&format!("--bot phải là lieu hoặc can_than (nhận '{b}')"))));
    let khung = args.num("khung", 10);
    let so_van = args.num("so-van", 1);
    let addr = (host.as_str(), port).to_socket_addrs().ok().and_then(|mut a| a.next()).unwrap_or_else(|| die(&format!("Không phân giải được {host}")));
    let sock = UdpSocket::bind(if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).unwrap_or_else(|e| die(&format!("Không tạo được socket: {e}")));
    let mut human = None;
    if bot.is_none() {
        let inp = Arc::new(AtomicI64::new(0));
        let inp2 = Arc::clone(&inp);
        thread::spawn(move || {
            // luồng đọc bàn phím: g/n/p + Enter đổi đầu vào hiện tại
            for line in io::stdin().lock().lines().map_while(Result::ok) {
                match line.trim().to_lowercase().chars().next() {
                    Some('g') => inp2.store(1, Ordering::Relaxed),
                    Some('n') => inp2.store(0, Ordering::Relaxed),
                    Some('p') => inp2.store(-1, Ordering::Relaxed),
                    _ => {}
                }
            }
        });
        human = Some(Human { inp, t_last: Instant::now() });
    }
    for _ in 0..so_van {
        race_once(&sock, addr, &me, bot, khung, human.as_mut());
    }
}
