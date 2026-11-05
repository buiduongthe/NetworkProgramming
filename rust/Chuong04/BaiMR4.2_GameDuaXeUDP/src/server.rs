//! Server đua xe thời gian thực trên UDP (server có thẩm quyền, vòng tick 20 Hz).
//!
//! Mô hình:
//!   - Client gửi ĐẦU VÀO (ga/nhả/phanh) kèm số thứ tự = tick mà đầu vào áp dụng; mỗi gói mang kèm 3 đầu vào
//!     gần nhất nên mất một gói vẫn còn bản sao ở gói sau.
//!   - Server chạy vòng lặp tick cố định 50 ms: lấy đầu vào của tick hiện tại cho từng xe; nếu không có
//!     (mọi bản sao đều mất) thì GIỮ đầu vào cũ; mô phỏng vật lý; gửi ẢNH CHỤP trạng thái tới mọi client.
//!   - Ảnh chụp mang số tick: client chỉ giữ ảnh mới nhất, bỏ ảnh đến muộn (cũ hơn).
//!   - --mat, --dao giả lập mất gói và đảo thứ tự (theo hàm băm có hạt giống nên tái lập được).
//!     Danh sách "0,20" nghĩa là cuộc đua 1 mất 0 %, cuộc đua 2 mất 20 %.

use crate::dua_xe::{fmt2, lossy, step, END_TICKS, MAX_RACE, START, TICK_MS, TRACK, V_CUA};
use crate::{die, Args};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::process;
use std::time::{Duration, Instant};

const DIR_IN: i64 = 0;
const DIR_SNAP: i64 = 1;
const DIR_REORDER: i64 = 2;

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Cho,  // chờ đủ xe
    Dua,  // đang đua (gồm đếm ngược)
    Xong, // đã có kết quả, gửi thêm vài ảnh chụp
}

impl Phase {
    fn text(self) -> &'static str {
        match self {
            Phase::Cho => "CHO",
            Phase::Dua => "DUA",
            Phase::Xong => "XONG",
        }
    }
}

struct Car {
    name: String,
    addr: SocketAddr,
    id: i64,
    x: i64,
    v: i64,
    done: i64,                   // số tick từ lúc xuất phát đến khi về đích (-1: chưa)
    inputs: BTreeMap<i64, i64>,  // tick -> đầu vào đã nhận (bộ đệm đầu vào)
    last_in: i64,                // đầu vào áp dụng gần nhất (dùng lại khi thiếu)
    recv: i64,
    dropped: i64,
    held: i64,
    crashes: i64,
    delayed: Option<Vec<u8>>, // ảnh chụp đang bị giữ lại để gửi muộn (giả lập đảo thứ tự)
}

impl Car {
    fn new(name: String, addr: SocketAddr) -> Car {
        Car { name, addr, id: -1, x: 0, v: 0, done: -1, inputs: BTreeMap::new(), last_in: 0, recv: 0, dropped: 0, held: 0, crashes: 0, delayed: None }
    }
}

fn pct_list(text: &str) -> Vec<i64> {
    text.split(',').map(|p| p.trim().parse().unwrap_or_else(|_| die(&format!("Tỉ lệ sai: {p}")))).collect()
}

struct RaceServer {
    sock: UdpSocket,
    so_xe: usize,
    seed: i64,
    mat: Vec<i64>,
    dao: Vec<i64>,
    race: i64,
    cars: Vec<Car>, // theo thứ tự vào phòng
    state: Phase,
    tick: i64,
    end_tick: i64,
    next_tick: Option<Instant>,
}

impl RaceServer {
    fn reset(&mut self) {
        self.cars.clear();
        self.state = Phase::Cho;
        self.tick = -1;
        self.end_tick = -1;
        self.next_tick = None;
    }

    fn send(&self, addr: SocketAddr, obj: &Value) {
        let _ = self.sock.send_to(serde_json::to_string(obj).unwrap().as_bytes(), addr);
    }

    // ----- nhận gói -----
    fn on_packet(&mut self, data: &[u8], addr: SocketAddr) {
        let Ok(p) = serde_json::from_slice::<Value>(data) else {
            return; // datagram hỏng: bỏ qua
        };
        if !p.is_object() {
            return;
        }
        match p["t"].as_str() {
            Some("join") => self.on_join(&p, addr),
            Some("in") => self.on_input(&p, addr),
            _ => {}
        }
    }

    fn on_join(&mut self, p: &Value, addr: SocketAddr) {
        let Some(name) = p["name"].as_str().filter(|n| (1..=12).contains(&n.chars().count())) else {
            return;
        };
        let known = self.cars.iter().position(|c| c.addr == addr);
        let idx = match known {
            Some(i) if self.state != Phase::Xong => i,
            _ => {
                if self.state != Phase::Cho || self.cars.iter().any(|c| c.name == name) {
                    return self.send(addr, &json!({"t": "busy"})); // đang đua hoặc trùng tên: client thử lại sau
                }
                self.cars.push(Car::new(name.to_string(), addr));
                println!("[server] Thêm một xe từ {} ({}/{})", addr.ip(), self.cars.len(), self.so_xe);
                self.cars.len() - 1
            }
        };
        // trả lời cả khi client gửi lại join (welcome trước đó có thể đã mất)
        let w = json!({"t": "welcome", "name": self.cars[idx].name, "need": self.so_xe, "race": self.race + 1,
                       "mat": self.loss(), "dao": self.reorder()});
        self.send(addr, &w);
        if self.state == Phase::Cho && self.cars.len() == self.so_xe {
            self.start_race();
        }
    }

    fn on_input(&mut self, p: &Value, addr: SocketAddr) {
        let (seed, loss, tick, state) = (self.seed, self.loss(), self.tick, self.state);
        let Some(car) = self.cars.iter_mut().find(|c| c.addr == addr) else { return };
        let (Some(seq), Some(inputs)) = (p["seq"].as_i64(), p["in"].as_array()) else { return };
        if state != Phase::Dua {
            return;
        }
        if lossy(seed, DIR_IN, car.id, seq, loss) {
            car.dropped += 1; // giả lập: gói này "mất trên đường"
            return;
        }
        car.recv += 1;
        for (j, inp) in inputs.iter().take(3).enumerate() {
            // inputs[j] là đầu vào của tick seq - j
            let t = seq - j as i64;
            if let Some(i) = inp.as_i64().filter(|i| (-1..=1).contains(i)) {
                if t > tick {
                    car.inputs.entry(t).or_insert(i); // chỉ nhận đầu vào cho tick chưa mô phỏng
                }
            }
        }
    }

    // ----- cuộc đua -----
    fn loss(&self) -> i64 {
        self.mat[(self.race as usize).min(self.mat.len() - 1)]
    }

    fn reorder(&self) -> i64 {
        self.dao[(self.race as usize).min(self.dao.len() - 1)]
    }

    /// Chỉ số các xe theo mã xe (id).
    fn ordered(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.cars.len()).collect();
        v.sort_by_key(|&i| self.cars[i].id);
        v
    }

    fn start_race(&mut self) {
        let mut by_name: Vec<usize> = (0..self.cars.len()).collect();
        by_name.sort_by(|&a, &b| self.cars[a].name.cmp(&self.cars[b].name));
        for (i, &c) in by_name.iter().enumerate() {
            self.cars[c].id = i as i64; // mã xe theo thứ tự tên: không phụ thuộc ai vào trước
        }
        self.state = Phase::Dua;
        self.tick = -1;
        self.next_tick = Some(Instant::now() + Duration::from_millis(TICK_MS));
        let names: Vec<String> = self.ordered().iter().map(|&i| self.cars[i].name.clone()).collect();
        println!(
            "[server] Cuộc đua {}: {}; mất gói {} %, đảo thứ tự {} %; xuất phát sau {START} tick",
            self.race + 1,
            names.join(", "),
            self.loss(),
            self.reorder()
        );
    }

    fn do_tick(&mut self) {
        self.tick += 1;
        let k = self.tick;
        let racing = self.state == Phase::Dua && k >= START;
        for i in self.ordered() {
            let car = &mut self.cars[i];
            let got = car.inputs.remove(&k);
            car.inputs.retain(|&t, _| t >= k); // dọn đầu vào quá hạn
            let inp = match got {
                Some(v) => v,
                None => {
                    if racing && car.done < 0 {
                        car.held += 1; // thiếu đầu vào: giữ đầu vào cũ
                    }
                    car.last_in
                }
            };
            car.last_in = inp;
            if !racing || car.done >= 0 {
                continue;
            }
            let (x, v, crashed) = step(car.x, car.v, inp);
            car.x = x;
            car.v = v;
            if crashed {
                car.crashes += 1;
                println!("[server] t={}: {} văng ở khúc cua (quá {} ô/tick)", k - START, car.name, fmt2(V_CUA));
            }
            if car.x >= TRACK {
                car.x = TRACK;
                car.v = 0;
                car.done = k - START + 1;
            }
        }
        if self.state == Phase::Dua && (self.cars.iter().all(|c| c.done >= 0) || k >= START + MAX_RACE) {
            self.state = Phase::Xong;
            self.end_tick = k;
            let rank: Vec<String> = self
                .ranking()
                .iter()
                .enumerate()
                .map(|(n, &i)| {
                    let c = &self.cars[i];
                    let d = if c.done >= 0 { c.done.to_string() } else { "-".into() };
                    format!("{}. {} {d} tick", n + 1, c.name)
                })
                .collect();
            println!("[server] Cuộc đua {} kết thúc: {}", self.race + 1, rank.join("; "));
        }
        self.broadcast(k);
        if self.state == Phase::Xong && k >= self.end_tick + END_TICKS {
            self.finish_race();
        }
    }

    fn ranking(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.cars.len()).collect();
        v.sort_by_key(|&i| {
            let c = &self.cars[i];
            (c.done < 0, c.done, -c.x, c.id)
        });
        v
    }

    fn broadcast(&mut self, k: i64) {
        let order = self.ordered();
        let names: Vec<&str> = order.iter().map(|&i| self.cars[i].name.as_str()).collect();
        let cars: Vec<Value> = order.iter().map(|&i| {
            let c = &self.cars[i];
            json!([c.id, c.x, c.v, c.done])
        }).collect();
        let mut snap = json!({"t": "snap", "race": self.race + 1, "tick": k, "phase": self.state.text(),
                              "names": names, "cars": cars});
        if self.state == Phase::Xong {
            let rank: Vec<Value> = self.ranking().iter().map(|&i| json!([self.cars[i].name, self.cars[i].done])).collect();
            snap["rank"] = Value::Array(rank);
        }
        let data = serde_json::to_string(&snap).unwrap().into_bytes();
        let (seed, loss, reorder) = (self.seed, self.loss(), self.reorder());
        for i in order {
            let car = &mut self.cars[i];
            let prev = car.delayed.take();
            if lossy(seed, DIR_SNAP, car.id, k, loss) {
                // ảnh chụp "mất"
            } else if lossy(seed, DIR_REORDER, car.id, k, reorder) {
                car.delayed = Some(data.clone()); // giữ lại, gửi SAU ảnh chụp tick kế tiếp
            } else {
                let _ = self.sock.send_to(&data, car.addr);
            }
            if let Some(p) = prev {
                let _ = self.sock.send_to(&p, car.addr); // ảnh chụp cũ đến sau ảnh mới: client phải bỏ
            }
        }
    }

    fn finish_race(&mut self) {
        for i in self.ordered() {
            let c = &self.cars[i];
            println!(
                "[server] {}: nhận {}/{} gói đầu vào (giả lập mất {}), {} tick phải giữ đầu vào cũ, văng {} lần",
                c.name,
                c.recv,
                c.recv + c.dropped,
                c.dropped,
                c.held,
                c.crashes
            );
        }
        self.race += 1;
        self.reset();
    }
}

pub fn run(args: Args) {
    let host = args.pos.first().cloned().unwrap_or_else(|| "0.0.0.0".into());
    let port = args.port(1, 9200);
    let so_xe = args.num("so-xe", 2).max(1) as usize;
    let so_van = args.num("so-van", 0);
    let sock = UdpSocket::bind((host.as_str(), port)).unwrap_or_else(|e| die(&format!("[server] Không mở được {host}:{port}: {e}")));
    ctrlc::set_handler(|| {
        println!("[server] Dừng server");
        process::exit(0);
    })
    .expect("Không cài được hàm bắt Ctrl+C");
    println!("[server] Đua xe UDP tại {host}:{port}, {} tick/s, cần {so_xe} xe, đường dài {} ô", 1000 / TICK_MS, TRACK / 100);
    let mut srv = RaceServer {
        sock,
        so_xe,
        seed: args.num("seed", 1),
        mat: pct_list(&args.text("mat", "0")),
        dao: pct_list(&args.text("dao", "0")),
        race: 0,
        cars: Vec::new(),
        state: Phase::Cho,
        tick: -1,
        end_tick: -1,
        next_tick: None,
    };
    let mut buf = [0u8; 2048];
    loop {
        // chờ gói tới, nhưng không quá thời điểm của tick kế tiếp (thay cho select có thời gian chờ)
        let wait = srv.next_tick.map(|t| t.saturating_duration_since(Instant::now()));
        if wait != Some(Duration::ZERO) {
            srv.sock.set_read_timeout(wait).ok();
            match srv.sock.recv_from(&mut buf) {
                Ok((n, addr)) => srv.on_packet(&buf[..n].to_vec(), addr),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(e) if e.kind() == ErrorKind::ConnectionReset => {} // Windows: ICMP "port unreachable"
                Err(e) => println!("[server] Lỗi nhận: {e}"),
            }
        }
        if let Some(t) = srv.next_tick {
            if Instant::now() >= t {
                srv.next_tick = Some(t + Duration::from_millis(TICK_MS)); // lịch cố định: không trôi
                srv.do_tick();
                if srv.state == Phase::Cho && so_van > 0 && srv.race >= so_van {
                    println!("[server] Đã đua {} cuộc, dừng server", srv.race);
                    break;
                }
            }
        }
    }
}
