//! Server trò chơi đoán số nhiều người chơi trên TCP.
//!
//! Server chọn một số bí mật; người chơi lần lượt đoán, server trả lời "thấp quá", "cao quá" hoặc "đúng".
//! Thiết kế:
//!   - Mỗi kết nối có một luồng đọc (reader) chỉ làm một việc: nhận khung JSON và gửi sự kiện vào kênh mpsc.
//!   - MỘT luồng trò chơi (vòng lặp sự kiện) giữ toàn bộ trạng thái, xử lý tuần tự từng sự kiện,
//!     nên không cần khóa (Mutex). Hết giờ mỗi lượt = thời gian chờ của recv_timeout().
//!   - Máy trạng thái của server: CHO (chờ đủ người) -> DANG_CHOI -> (kết thúc ván) -> CHO.
//!   - Server là bên quyết định: kiểm tra mọi thông điệp (đúng lượt, kiểu số, khoảng hợp lệ), không tin client.

use crate::giao_thuc::{decode_json, recv_frame, repr, send_json, Lcg};
use crate::{die, Args};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::process;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(PartialEq, Clone, Copy)]
enum State {
    Cho,      // chờ đủ người
    DangChoi, // đang chơi
}

fn hint_text(h: &str) -> &'static str {
    match h {
        "THAP" => "thấp quá",
        "CAO" => "cao quá",
        _ => "ĐÚNG",
    }
}

fn error_text(code: &str) -> &'static str {
    match code {
        "BAD_JSON" => "Khung không phải đối tượng JSON",
        "NEED_HELLO" => "Phải gửi hello trước",
        "BAD_NAME" => "Tên phải là chuỗi 1–16 ký tự",
        "NAME_TAKEN" => "Tên đã có người dùng",
        "GAME_IN_PROGRESS" => "Ván đang diễn ra, hãy vào lại sau",
        "GAME_NOT_STARTED" => "Ván chưa bắt đầu",
        "NOT_YOUR_TURN" => "Chưa đến lượt bạn",
        "BAD_VALUE" => "Giá trị đoán phải là số nguyên",
        "OUT_OF_RANGE" => "Số đoán nằm ngoài khoảng cho phép",
        _ => "Loại thông điệp không hỗ trợ",
    }
}

/// Sự kiện gửi cho vòng trò chơi. Mỗi kết nối có một mã số (id) thay cho đối tượng socket.
enum Event {
    Open(u64, TcpStream, String), // kết nối mới: id, tay cầm để gửi, "ip:port"
    Msg(u64, Option<Value>),      // một khung; None = khung hỏng
    Closed(u64),                  // luồng đọc gặp EOF/lỗi
}

/// Luồng đọc của một kết nối: chuyển mỗi khung thành sự kiện cho vòng trò chơi.
fn reader(id: u64, mut conn: TcpStream, events: Sender<Event>) {
    loop {
        match recv_frame(&mut conn) {
            Ok(payload) => {
                if events.send(Event::Msg(id, decode_json(&payload))).is_err() {
                    return;
                }
            }
            Err(_) => {
                let _ = events.send(Event::Closed(id));
                return;
            }
        }
    }
}

/// Luồng chấp nhận kết nối: mỗi client mới có một luồng đọc riêng.
fn acceptor(srv: TcpListener, events: Sender<Event>) {
    let mut next_id = 1;
    for conn in srv.incoming().flatten() {
        let addr = conn.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".into());
        conn.set_nodelay(true).ok(); // tắt Nagle: phản hồi nhỏ đi ngay (Go bật sẵn)
        let rd = conn.try_clone().expect("try_clone");
        if events.send(Event::Open(next_id, conn, addr)).is_err() {
            return;
        }
        let ev = events.clone();
        let id = next_id;
        thread::spawn(move || reader(id, rd, ev));
        next_id += 1;
    }
}

struct Opts {
    so_nguoi: usize,
    het_gio_ms: u64,
    min: i64,
    max: i64,
}

/// Toàn bộ trạng thái trò chơi; chỉ luồng trò chơi được đọc/ghi.
struct Game {
    a: Opts,
    rng: Lcg,
    state: State,
    conns: HashMap<u64, TcpStream>, // id -> socket (để gửi)
    names: Vec<(u64, String)>,      // người đã gửi hello hợp lệ, theo thứ tự vào phòng
    order: Vec<u64>,                // thứ tự lượt
    turn: usize,                    // chỉ số người đang có lượt trong order
    turn_no: i64,
    deadline: Option<Instant>, // hạn chót của lượt hiện tại
    secret: i64,
    lo: i64,
    hi: i64,
    round: i64,
}

impl Game {
    fn name_of(&self, id: u64) -> Option<String> {
        self.names.iter().find(|(c, _)| *c == id).map(|(_, n)| n.clone())
    }

    // ----- gửi -----
    fn send(&self, id: u64, obj: &Value) {
        if let Some(c) = self.conns.get(&id) {
            let _ = send_json(c, obj); // client đã mất: luồng đọc sẽ báo Closed
        }
    }

    fn broadcast(&self, obj: &Value) {
        for (id, _) in &self.names {
            self.send(*id, obj);
        }
    }

    fn error(&self, id: u64, code: &str, who: &str, extra: &str) {
        println!("[server] Từ chối {who}: {code}{extra}");
        self.send(id, &json!({"type": "error", "code": code, "msg": error_text(code)}));
    }

    /// Đóng hẳn kết nối (luồng đọc sẽ thoát).
    fn drop_conn(&mut self, id: u64) {
        self.names.retain(|(c, _)| *c != id);
        if let Some(c) = self.conns.remove(&id) {
            let _ = c.shutdown(Shutdown::Both);
        }
    }

    // ----- máy trạng thái -----
    fn start_round(&mut self) {
        self.round += 1;
        self.state = State::DangChoi;
        self.secret = self.rng.next_int(self.a.min, self.a.max);
        self.lo = self.a.min;
        self.hi = self.a.max;
        self.order = self.names.iter().map(|(c, _)| *c).collect();
        self.turn = 0;
        self.turn_no = 0;
        let order_names: Vec<String> = self.names.iter().map(|(_, n)| n.clone()).collect();
        println!(
            "[server] Bắt đầu ván {}: số bí mật = {} (chỉ server biết), thứ tự: {}",
            self.round,
            self.secret,
            order_names.join(", ")
        );
        self.broadcast(&json!({"type": "start", "round": self.round, "order": order_names,
                               "min": self.a.min, "max": self.a.max}));
        self.next_turn();
    }

    /// Giao lượt cho người kế tiếp và đặt hạn chót.
    fn next_turn(&mut self) {
        self.turn_no += 1;
        let name = self.name_of(self.order[self.turn]).unwrap_or_default();
        self.deadline = Some(Instant::now() + Duration::from_millis(self.a.het_gio_ms));
        self.broadcast(&json!({"type": "turn", "player": name, "lo": self.lo, "hi": self.hi,
                               "turn_no": self.turn_no, "timeout_ms": self.a.het_gio_ms}));
    }

    fn advance(&mut self) {
        self.turn = (self.turn + 1) % self.order.len();
        self.next_turn();
    }

    fn end_round(&mut self, winner: Option<String>) {
        match &winner {
            Some(w) => println!("[server] Kết thúc ván {}: {w} thắng sau {} lượt", self.round, self.turn_no),
            None => println!("[server] Kết thúc ván {}: không còn người chơi", self.round),
        }
        self.broadcast(&json!({"type": "game_over", "winner": winner, "secret": self.secret, "turns": self.turn_no}));
        let ids: Vec<u64> = self.names.iter().map(|(c, _)| *c).collect();
        for id in ids {
            // mỗi ván một phiên: đóng mọi kết nối
            self.send(id, &json!({"type": "bye"}));
            self.drop_conn(id);
        }
        self.state = State::Cho;
        self.order.clear();
        self.deadline = None;
    }

    fn remove_player(&mut self, id: u64) {
        let name = self.name_of(id).unwrap_or_default();
        self.names.retain(|(c, _)| *c != id);
        println!("[server] {name} rời trò chơi (còn {} người)", self.names.len());
        self.broadcast(&json!({"type": "left", "player": name}));
        if self.state != State::DangChoi {
            return;
        }
        let i = self.order.iter().position(|c| *c == id).unwrap();
        let was_current = i == self.turn;
        self.order.remove(i);
        if self.order.is_empty() {
            self.end_round(None);
            return;
        }
        if i < self.turn {
            self.turn -= 1; // giữ nguyên người đang có lượt
        }
        if was_current {
            self.turn %= self.order.len();
            self.next_turn();
        }
    }

    fn on_timeout(&mut self) {
        let name = self.name_of(self.order[self.turn]).unwrap_or_default();
        println!("[server] {name} hết giờ ở lượt {}", self.turn_no);
        self.broadcast(&json!({"type": "timeout", "player": name}));
        self.advance();
    }

    // ----- xử lý thông điệp -----
    fn on_message(&mut self, id: u64, addr: &str, msg: Option<Value>) {
        let who = self.name_of(id).unwrap_or_else(|| addr.to_string());
        let Some(msg) = msg else {
            return self.error(id, "BAD_JSON", &who, "");
        };
        let kind = msg.get("type").cloned().unwrap_or(Value::Null);
        if self.name_of(id).is_none() {
            // chưa vào phòng: chỉ chấp nhận hello
            if kind != "hello" {
                return self.error(id, "NEED_HELLO", &who, "");
            }
            return self.on_hello(id, addr, &msg);
        }
        if kind == "guess" {
            return self.on_guess(id, &who, msg.get("value").cloned().unwrap_or(Value::Null));
        }
        if kind == "bye" {
            self.remove_player(id);
            self.drop_conn(id);
            return;
        }
        self.error(id, "UNKNOWN_TYPE", &who, &format!(" ({})", repr(&kind)));
    }

    fn on_hello(&mut self, id: u64, addr: &str, msg: &Value) {
        let name = match msg.get("name").and_then(Value::as_str) {
            Some(n) if (1..=16).contains(&n.trim().chars().count()) => n.trim().to_string(),
            _ => return self.error(id, "BAD_NAME", addr, ""),
        };
        if self.names.iter().any(|(_, n)| *n == name) {
            return self.error(id, "NAME_TAKEN", addr, &format!(" ({name})"));
        }
        if self.state == State::DangChoi {
            self.error(id, "GAME_IN_PROGRESS", &name, "");
            self.send(id, &json!({"type": "bye"}));
            self.drop_conn(id);
            return;
        }
        self.names.push((id, name.clone()));
        println!("[server] {name} vào phòng chờ ({}/{})", self.names.len(), self.a.so_nguoi);
        self.send(id, &json!({"type": "welcome", "name": name, "waiting": self.names.len(),
                              "need": self.a.so_nguoi, "min": self.a.min, "max": self.a.max}));
        if self.names.len() >= self.a.so_nguoi {
            self.start_round();
        }
    }

    fn on_guess(&mut self, id: u64, who: &str, value: Value) {
        if self.state != State::DangChoi {
            return self.error(id, "GAME_NOT_STARTED", who, "");
        }
        if self.order[self.turn] != id {
            return self.error(id, "NOT_YOUR_TURN", who, "");
        }
        // chỉ số nguyên JSON (50); 50.0, true, "50" đều không hợp lệ
        let Some(v) = value.as_i64() else {
            return self.error(id, "BAD_VALUE", who, &format!(" ({})", repr(&value)));
        };
        if !(self.lo..=self.hi).contains(&v) {
            return self.error(id, "OUT_OF_RANGE", who, &format!(" ({v} ngoài {}–{})", self.lo, self.hi));
        }
        let hint = if v == self.secret { "DUNG" } else if v < self.secret { "THAP" } else { "CAO" };
        println!("[server] Lượt {}: {who} đoán {v} -> {}", self.turn_no, hint_text(hint));
        self.broadcast(&json!({"type": "result", "player": who, "value": v, "hint": hint}));
        if hint == "DUNG" {
            self.end_round(Some(who.to_string()));
            return;
        }
        if hint == "THAP" {
            self.lo = v + 1;
        } else {
            self.hi = v - 1;
        }
        self.advance();
    }
}

pub fn run(args: Args) {
    let host = args.pos.first().cloned().unwrap_or_else(|| "0.0.0.0".into());
    let port: u16 = args.pos.get(1).map(|p| p.parse().unwrap_or_else(|_| die(&format!("Cổng sai: {p}")))).unwrap_or(9000);
    let so_van = args.num("so-van", 0);
    let seed = match args.num("seed", 0) {
        0 => SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(1),
        s => s,
    };
    let a = Opts {
        so_nguoi: args.num("so-nguoi", 2).max(1) as usize,
        het_gio_ms: args.num("het-gio-ms", 30000).max(0) as u64,
        min: args.num("min", 1),
        max: args.num("max", 100),
    };
    let srv = TcpListener::bind((host.as_str(), port)).unwrap_or_else(|e| {
        eprintln!("[server] Không mở được {host}:{port}: {e}");
        process::exit(1)
    });
    ctrlc::set_handler(|| {
        println!("[server] Dừng server");
        process::exit(0);
    })
    .expect("Không cài được hàm bắt Ctrl+C");
    println!("[server] Trò chơi đoán số tại {host}:{port}; cần {} người; mỗi lượt tối đa {} ms", a.so_nguoi, a.het_gio_ms);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || acceptor(srv, tx));
    let mut game = Game {
        rng: Lcg::new(seed),
        state: State::Cho,
        conns: HashMap::new(),
        names: Vec::new(),
        order: Vec::new(),
        turn: 0,
        turn_no: 0,
        deadline: None,
        secret: 0,
        lo: a.min,
        hi: a.max,
        round: 0,
        a,
    };
    let mut addrs: HashMap<u64, String> = HashMap::new();
    loop {
        if game.deadline.is_some_and(|d| Instant::now() >= d) {
            game.on_timeout(); // kiểm tra trước: thông điệp đến liên tục không "hoãn" được hết giờ
        }
        let ev = match game.deadline {
            Some(d) => match rx.recv_timeout(d.saturating_duration_since(Instant::now())) {
                Ok(ev) => ev,
                Err(RecvTimeoutError::Timeout) => {
                    game.on_timeout(); // hết giờ của lượt hiện tại
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(ev) => ev,
                Err(_) => break,
            },
        };
        match ev {
            Event::Open(id, conn, addr) => {
                println!("[server] {addr} kết nối");
                game.conns.insert(id, conn);
                addrs.insert(id, addr);
            }
            Event::Msg(id, msg) => {
                let addr = addrs.get(&id).cloned().unwrap_or_else(|| "?".into());
                game.on_message(id, &addr, msg);
            }
            Event::Closed(id) => {
                addrs.remove(&id);
                if game.name_of(id).is_some() {
                    game.remove_player(id); // mất kết nối giữa chừng
                }
                game.conns.remove(&id); // đóng socket (drop)
            }
        }
        if so_van > 0 && game.round >= so_van && game.state == State::Cho {
            println!("[server] Đã chơi {} ván, dừng server", game.round);
            break;
        }
    }
}
