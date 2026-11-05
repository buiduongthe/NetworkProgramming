//! Server Caro nhiều phòng trên TCP, khám phá trong LAN bằng UDP broadcast.
//!
//! Thiết kế (mở rộng từ MR3.3):
//!   - Luồng chấp nhận + một luồng đọc cho mỗi kết nối: chỉ nhận khung JSON, gửi sự kiện vào kênh mpsc.
//!   - Luồng khám phá (RESPONDER của Bài 6.5): nhận yêu cầu discover trên UDP, gửi sự kiện vào kênh.
//!   - MỘT luồng trò chơi giữ toàn bộ trạng thái (sảnh, các phòng, bàn cờ), xử lý tuần tự từng sự kiện.
//!     Hạn chót của nước đi trong mọi phòng được gom lại: chờ kênh đến hạn chót sớm nhất.
//!   - Server là trọng tài: kiểm tra lượt, tọa độ, ô trống, phát hiện thắng; client chỉ hiển thị.
//!   - Mỗi nước đi hợp lệ được phát (broadcast) tới cả hai người chơi và mọi người xem trong phòng.
//! Trạng thái của một phòng: CHO (thiếu người) -> DANG_CHOI -> KET_THUC -(cả hai xin chơi lại)-> DANG_CHOI.

use crate::ban_co::{cell_name, five_line, new_board, Board, EMPTY, N};
use crate::giao_thuc::{decode_json, recv_frame, repr, send_json, DISC_SERVICE};
use crate::{die, Args};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn reason_text(r: &str) -> &'static str {
    match r {
        "five" => "5 quân liên tiếp",
        "timeout" => "đối thủ hết giờ",
        "left" => "đối thủ rời phòng",
        _ => "hòa (kín bàn)",
    }
}

fn error_text(code: &str) -> &'static str {
    match code {
        "BAD_JSON" => "Khung không phải đối tượng JSON",
        "NEED_HELLO" => "Phải gửi hello trước",
        "BAD_NAME" => "Tên phải là chuỗi 1–16 ký tự",
        "NAME_TAKEN" => "Tên đã có người dùng",
        "UNKNOWN_TYPE" => "Loại thông điệp không hỗ trợ",
        "ALREADY_IN_ROOM" => "Bạn đang ở trong một phòng",
        "BAD_ROOM_NAME" => "Tên phòng phải là chuỗi 1–16 ký tự",
        "ROOM_EXISTS" => "Phòng đã tồn tại",
        "NO_SUCH_ROOM" => "Không có phòng này",
        "ROOM_FULL" => "Phòng đã đủ hai người chơi",
        "NOT_IN_ROOM" => "Bạn chưa vào phòng nào",
        "NOT_PLAYER" => "Người xem không được đánh",
        "GAME_NOT_RUNNING" => "Ván chưa bắt đầu hoặc đã kết thúc",
        "NOT_YOUR_TURN" => "Chưa đến lượt bạn",
        "BAD_MOVE" => "Nước đi phải có r, c là số nguyên",
        "OUT_OF_BOARD" => "Ô nằm ngoài bàn cờ",
        _ => "Ô đã có quân", // CELL_TAKEN
    }
}

// ---------------------------------------------------------------- các luồng nhận
enum Event {
    Open(u64, TcpStream, String),
    Msg(u64, Option<Value>),
    Closed(u64),
    Discover(SocketAddr, Value),
}

fn reader(id: u64, conn: TcpStream, ev: Sender<Event>) {
    while let Ok(p) = recv_frame(&conn) {
        if ev.send(Event::Msg(id, decode_json(&p))).is_err() {
            return;
        }
    }
    let _ = ev.send(Event::Closed(id));
}

fn acceptor(srv: TcpListener, ev: Sender<Event>) {
    for (id, conn) in (1u64..).zip(srv.incoming().flatten()) {
        conn.set_nodelay(true).ok(); // tắt Nagle: nước đi đến ngay (Go bật sẵn)
        let addr = conn.peer_addr().map(|a| a.to_string()).unwrap_or_default();
        let rd = conn.try_clone().expect("try_clone");
        if ev.send(Event::Open(id, conn, addr)).is_err() {
            return;
        }
        let e2 = ev.clone();
        thread::spawn(move || reader(id, rd, e2));
    }
}

/// RESPONDER (Mã giả 6.7): nhận discover, để luồng trò chơi trả lời (nó mới biết số phòng).
fn responder(udp: Arc<UdpSocket>, ev: Sender<Event>) {
    let mut buf = [0u8; 2048];
    while let Ok((n, addr)) = udp.recv_from(&mut buf) {
        if let Some(q) = decode_json(&buf[..n]) {
            if q["type"] == "discover" && q["service"] == DISC_SERVICE && ev.send(Event::Discover(addr, q)).is_err() {
                return;
            }
        }
    }
}

// ---------------------------------------------------------------- trạng thái
#[derive(PartialEq, Clone, Copy)]
enum State {
    Cho,
    DangChoi,
    KetThuc,
}

impl State {
    fn text(self) -> &'static str {
        match self {
            State::Cho => "CHO",
            State::DangChoi => "DANG_CHOI",
            State::KetThuc => "KET_THUC",
        }
    }
}

struct Client {
    conn: TcpStream,
    addr: String,
    name: Option<String>,
    room: Option<String>, // tên phòng đang ở
    role: Option<u8>,     // b'X', b'O' hoặc b'v' (người xem)
}

struct Room {
    name: String,
    seats: [Option<u64>; 2], // mã client của quân X (0) và quân O (1)
    watchers: Vec<u64>,
    board: Board,
    state: State,
    turn: u8,
    moves: usize,
    game_no: u32,
    deadline: Option<Instant>,
    rematch: Vec<u8>,
}

fn seat(mark: u8) -> usize {
    if mark == b'X' {
        0
    } else {
        1
    }
}

fn other(mark: u8) -> u8 {
    if mark == b'X' {
        b'O'
    } else {
        b'X'
    }
}

fn mark_str(m: u8) -> String {
    (m as char).to_string()
}

struct Server {
    ten: String,
    port: u16,
    het_gio: Duration,
    udp: Arc<UdpSocket>,
    clients: HashMap<u64, Client>,
    rooms: Vec<Room>, // giữ thứ tự tạo
}

impl Server {
    fn name_of(&self, id: Option<u64>) -> Option<String> {
        id.and_then(|i| self.clients.get(&i)).and_then(|c| c.name.clone())
    }

    fn room_idx(&self, name: &Option<String>) -> Option<usize> {
        name.as_ref().and_then(|n| self.rooms.iter().position(|r| r.name == *n))
    }

    // ----- gửi -----
    fn send(&self, id: u64, obj: &Value) {
        if let Some(c) = self.clients.get(&id) {
            let _ = send_json(&c.conn, obj);
        }
    }

    fn members(&self, ri: usize) -> Vec<u64> {
        let r = &self.rooms[ri];
        r.seats.iter().flatten().copied().chain(r.watchers.iter().copied()).collect()
    }

    fn to_room(&self, ri: usize, obj: &Value) {
        for id in self.members(ri) {
            self.send(id, obj);
        }
    }

    fn error(&self, id: u64, code: &str, extra: &str) {
        let c = &self.clients[&id];
        println!("[server] Từ chối {}: {code}{extra}", c.name.clone().unwrap_or_else(|| c.addr.clone()));
        self.send(id, &json!({"type": "error", "code": code, "msg": error_text(code)}));
    }

    fn summary(&self, ri: usize) -> Value {
        let r = &self.rooms[ri];
        json!({"room": r.name, "x": self.name_of(r.seats[0]), "o": self.name_of(r.seats[1]),
               "watchers": r.watchers.len(), "state": r.state.text()})
    }

    fn room_list(&self) -> Vec<Value> {
        (0..self.rooms.len()).map(|i| self.summary(i)).collect()
    }

    // ----- ván cờ -----
    fn start_game(&mut self, ri: usize) {
        let het_gio = self.het_gio;
        let r = &mut self.rooms[ri];
        r.game_no += 1;
        r.board = new_board();
        r.turn = b'X';
        r.moves = 0;
        r.state = State::DangChoi;
        r.rematch.clear();
        r.deadline = Some(Instant::now() + het_gio);
        let (seats, game_no, room) = (r.seats, r.game_no, r.name.clone());
        let (x, o) = (self.name_of(seats[0]).unwrap_or_default(), self.name_of(seats[1]).unwrap_or_default());
        println!("[server] Phòng {room}, ván {game_no}: X = {x}, O = {o}");
        self.to_room(ri, &json!({"type": "start", "room": room, "game_no": game_no, "x": x, "o": o,
                                 "size": N, "timeout_ms": het_gio.as_millis() as u64}));
    }

    fn finish(&mut self, ri: usize, winner_mark: Option<u8>, reason: &str, line: Vec<(usize, usize)>) {
        let r = &mut self.rooms[ri];
        r.state = State::KetThuc;
        r.deadline = None;
        let (seats, game_no, moves, room) = (r.seats, r.game_no, r.moves, r.name.clone());
        let winner = winner_mark.and_then(|m| self.name_of(seats[seat(m)]));
        let result = match (&winner, winner_mark) {
            (Some(w), Some(m)) => format!("{w} ({}) thắng", m as char),
            _ => "hòa".to_string(),
        };
        println!("[server] Phòng {room}, ván {game_no} kết thúc sau {moves} nước: {result} – {}", reason_text(reason));
        let line: Vec<Value> = line.iter().map(|&(a, b)| json!([a, b])).collect();
        self.to_room(ri, &json!({"type": "over", "room": room, "game_no": game_no, "winner": winner,
                                 "mark": winner_mark.map(mark_str), "reason": reason, "moves": moves, "line": line}));
    }

    fn on_move(&mut self, id: u64, msg: &Value) {
        let cl = &self.clients[&id];
        let Some(ri) = self.room_idx(&cl.room) else { return self.error(id, "NOT_IN_ROOM", "") };
        let role = cl.role.unwrap_or(b'v');
        if role == b'v' {
            return self.error(id, "NOT_PLAYER", "");
        }
        if self.rooms[ri].state != State::DangChoi {
            return self.error(id, "GAME_NOT_RUNNING", "");
        }
        if role != self.rooms[ri].turn {
            return self.error(id, "NOT_YOUR_TURN", "");
        }
        let (Some(r), Some(c)) = (msg["r"].as_i64(), msg["c"].as_i64()) else { return self.error(id, "BAD_MOVE", "") };
        if !(0..N as i64).contains(&r) || !(0..N as i64).contains(&c) {
            return self.error(id, "OUT_OF_BOARD", &format!(" ({r}, {c})"));
        }
        let (r, c) = (r as usize, c as usize);
        if self.rooms[ri].board[r][c] != EMPTY {
            return self.error(id, "CELL_TAKEN", &format!(" ({})", cell_name(r as i64, c as i64)));
        }
        let by = self.clients[&id].name.clone();
        let room = &mut self.rooms[ri];
        room.board[r][c] = role; // nước đi hợp lệ: ghi vào bàn cờ của server
        room.moves += 1;
        let line = five_line(&room.board, r, c); // phán xử TRƯỚC khi báo nước đi
        let ended = line.is_some() || room.moves == N * N;
        let nxt = if ended { None } else { Some(other(role)) };
        let m = json!({"type": "moved", "room": room.name, "r": r, "c": c, "mark": mark_str(role), "by": by,
                       "move_no": room.moves, "next": nxt.map(mark_str)}); // next = null: ván đã hết
        self.to_room(ri, &m);
        if let Some(line) = line {
            return self.finish(ri, Some(role), "five", line);
        }
        if ended {
            return self.finish(ri, None, "draw", vec![]);
        }
        let het_gio = self.het_gio;
        let room = &mut self.rooms[ri];
        room.turn = nxt.unwrap();
        room.deadline = Some(Instant::now() + het_gio); // mỗi nước có hạn chót riêng
    }

    fn on_timeout(&mut self, ri: usize) {
        let room = &self.rooms[ri];
        let loser = self.name_of(room.seats[seat(room.turn)]).unwrap_or_default();
        println!("[server] Phòng {}: {loser} ({}) hết giờ", room.name, room.turn as char);
        let winner = other(room.turn);
        self.finish(ri, Some(winner), "timeout", vec![]);
    }

    // ----- phòng -----
    fn enter(&mut self, id: u64, ri: usize, role: u8) {
        let room_name = self.rooms[ri].name.clone();
        {
            let cl = self.clients.get_mut(&id).unwrap();
            cl.room = Some(room_name.clone());
            cl.role = Some(role);
        }
        if role == b'v' {
            self.rooms[ri].watchers.push(id);
        } else {
            self.rooms[ri].seats[seat(role)] = Some(id);
        }
        let vai = if role == b'v' { "người xem".to_string() } else { format!("quân {}", role as char) };
        println!("[server] {} vào phòng {room_name} ({vai})", self.name_of(Some(id)).unwrap_or_default());
        let role_txt = if role == b'v' { "xem".to_string() } else { mark_str(role) };
        self.send(id, &json!({"type": "joined", "room": room_name, "role": role_txt, "info": self.summary(ri)}));
        let r = &self.rooms[ri];
        if role == b'v' && r.state != State::Cho {
            // người xem đến muộn: gửi ảnh chụp bàn cờ
            let rows: Vec<String> = r.board.iter().map(|row| String::from_utf8_lossy(row).into_owned()).collect();
            let turn = if r.state == State::DangChoi { Some(mark_str(r.turn)) } else { None };
            self.send(id, &json!({"type": "board", "room": r.name, "game_no": r.game_no, "rows": rows, "moves": r.moves,
                                  "turn": turn, "state": r.state.text()}));
        }
    }

    fn leave_room(&mut self, id: u64) {
        let Some(ri) = self.room_idx(&self.clients[&id].room) else { return };
        let role = {
            let cl = self.clients.get_mut(&id).unwrap();
            cl.room = None;
            cl.role.take().unwrap_or(b'v')
        };
        if role == b'v' {
            self.rooms[ri].watchers.retain(|w| *w != id);
            return;
        }
        if self.rooms[ri].state == State::DangChoi {
            self.finish(ri, Some(other(role)), "left", vec![]); // bỏ dở ván: đối thủ thắng
        }
        self.rooms[ri].seats[seat(role)] = None;
        let (name, room_name) = (self.name_of(Some(id)).unwrap_or_default(), self.rooms[ri].name.clone());
        println!("[server] {name} rời phòng {room_name}");
        self.to_room(ri, &json!({"type": "left", "room": room_name, "player": name, "role": mark_str(role)}));
        let r = &mut self.rooms[ri];
        r.state = State::Cho;
        r.deadline = None;
        r.rematch.clear();
        if r.seats == [None, None] {
            println!("[server] Phòng {room_name} đóng (không còn người chơi)");
            let watchers = std::mem::take(&mut r.watchers);
            for w in watchers {
                if let Some(c) = self.clients.get_mut(&w) {
                    c.room = None;
                    c.role = None;
                }
                self.send(w, &json!({"type": "room_closed", "room": room_name}));
            }
            self.rooms.remove(ri);
        }
    }

    // ----- xử lý thông điệp -----
    fn on_message(&mut self, id: u64, msg: Option<Value>) {
        let Some(msg) = msg else { return self.error(id, "BAD_JSON", "") };
        let kind = msg.get("type").cloned().unwrap_or(Value::Null);
        if self.clients[&id].name.is_none() {
            if kind != "hello" {
                return self.error(id, "NEED_HELLO", "");
            }
            let name = match msg["name"].as_str() {
                Some(n) if (1..=16).contains(&n.trim().chars().count()) => n.trim().to_string(),
                _ => return self.error(id, "BAD_NAME", ""),
            };
            if self.clients.values().any(|o| o.name.as_deref() == Some(name.as_str())) {
                return self.error(id, "NAME_TAKEN", &format!(" ({name})"));
            }
            let cl = self.clients.get_mut(&id).unwrap();
            cl.name = Some(name.clone());
            println!("[server] {name} vào sảnh ({})", cl.addr);
            return self.send(id, &json!({"type": "welcome", "name": name, "rooms": self.room_list()}));
        }
        match kind.as_str() {
            Some("list") => self.send(id, &json!({"type": "rooms", "rooms": self.room_list()})),
            Some(k @ ("create" | "join" | "watch")) => self.on_room_request(id, k, &msg["room"]),
            Some("move") => self.on_move(id, &msg),
            Some("rematch") => self.on_rematch(id),
            Some("leave") => {
                if self.clients[&id].room.is_none() {
                    return self.error(id, "NOT_IN_ROOM", "");
                }
                self.leave_room(id);
            }
            Some("bye") => {
                self.leave_room(id);
                let cl = self.clients.get_mut(&id).unwrap();
                println!("[server] {} tạm biệt", cl.name.take().unwrap_or_default()); // không còn trong sảnh
                let _ = cl.conn.shutdown(Shutdown::Both); // luồng đọc sẽ báo Closed
            }
            _ => self.error(id, "UNKNOWN_TYPE", &format!(" ({})", repr(&kind))),
        }
    }

    fn on_room_request(&mut self, id: u64, kind: &str, name: &Value) {
        if self.clients[&id].room.is_some() {
            return self.error(id, "ALREADY_IN_ROOM", "");
        }
        let name = match name.as_str() {
            Some(n) if (1..=16).contains(&n.trim().chars().count()) => n.trim().to_string(),
            _ => return self.error(id, "BAD_ROOM_NAME", ""),
        };
        let ri = self.rooms.iter().position(|r| r.name == name);
        if kind == "create" {
            if ri.is_some() {
                return self.error(id, "ROOM_EXISTS", &format!(" ({name})"));
            }
            self.rooms.push(Room {
                name: name.clone(),
                seats: [None, None],
                watchers: Vec::new(),
                board: new_board(),
                state: State::Cho,
                turn: b'X',
                moves: 0,
                game_no: 0,
                deadline: None,
                rematch: Vec::new(),
            });
            println!("[server] {} tạo phòng {name}", self.name_of(Some(id)).unwrap_or_default());
            return self.enter(id, self.rooms.len() - 1, b'X');
        }
        let Some(ri) = ri else { return self.error(id, "NO_SUCH_ROOM", &format!(" ({name})")) };
        if kind == "watch" {
            return self.enter(id, ri, b'v');
        }
        let Some(free) = [b'X', b'O'].into_iter().find(|m| self.rooms[ri].seats[seat(*m)].is_none()) else {
            return self.error(id, "ROOM_FULL", &format!(" ({name})"));
        };
        self.enter(id, ri, free);
        if self.rooms[ri].seats.iter().all(Option::is_some) {
            self.start_game(ri);
        }
    }

    fn on_rematch(&mut self, id: u64) {
        let cl = &self.clients[&id];
        let (ri, role) = match (self.room_idx(&cl.room), cl.role) {
            (Some(ri), Some(role @ (b'X' | b'O'))) => (ri, role),
            _ => return self.error(id, "NOT_PLAYER", ""),
        };
        if self.rooms[ri].state != State::KetThuc {
            return self.error(id, "GAME_NOT_RUNNING", "");
        }
        let r = &mut self.rooms[ri];
        if !r.rematch.contains(&role) {
            r.rematch.push(role);
        }
        if r.rematch.len() < 2 {
            let m = json!({"type": "rematch_wait", "room": r.name, "by": self.name_of(Some(id))});
            return self.to_room(ri, &m);
        }
        r.seats.swap(0, 1); // đổi màu quân: người kia đi trước
        let seats = r.seats;
        for (m, sid) in [(b'X', seats[0]), (b'O', seats[1])] {
            if let Some(c) = sid.and_then(|s| self.clients.get_mut(&s)) {
                c.role = Some(m);
            }
        }
        self.start_game(ri);
    }

    fn on_discover(&self, addr: SocketAddr, q: &Value) {
        let players = self.clients.values().filter(|c| c.name.is_some()).count();
        let reply = json!({"id": q["id"], "name": self.ten, "port": self.port, "rooms": self.rooms.len(), "players": players});
        println!("[server] Trả lời discover từ {addr}");
        let _ = self.udp.send_to(reply.to_string().as_bytes(), addr);
    }

    fn on_closed(&mut self, id: u64) {
        if !self.clients.contains_key(&id) {
            return;
        }
        if self.clients[&id].name.is_some() {
            self.leave_room(id);
            println!("[server] {} ngắt kết nối", self.name_of(Some(id)).unwrap_or_default());
        }
        self.clients.remove(&id); // đóng socket (drop)
    }

    /// Phòng có hạn chót sớm nhất (nếu có).
    fn expired_room(&self) -> Option<(usize, Instant)> {
        let mut best: Option<(usize, Instant)> = None;
        for (i, r) in self.rooms.iter().enumerate() {
            if let Some(d) = r.deadline {
                if best.is_none_or(|b| d < b.1) {
                    best = Some((i, d));
                }
            }
        }
        best
    }
}

/// Socket UDP nghe khám phá: bind 0.0.0.0 (mới nhận được broadcast) với SO_REUSEADDR như bản Python.
fn udp_kham_pha(port: u16) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::DGRAM, None)?;
    s.set_reuse_address(true)?;
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();
    s.bind(&addr.into())?;
    Ok(s.into())
}

pub fn run(a: Args) {
    let host = a.pos.first().cloned().unwrap_or_else(|| "0.0.0.0".into());
    let port = a.port(1, 9100);
    let kham_pha = a.num("kham-pha", 9999) as u16;
    let ten = a.opt.get("ten").cloned().unwrap_or_else(|| "Caro".into());
    let het_gio_ms = a.num("het-gio-ms", 30000).max(1) as u64;
    let srv = TcpListener::bind((host.as_str(), port)).unwrap_or_else(|e| die(&format!("[server] Không mở được {host}:{port}: {e}")));
    let udp = Arc::new(udp_kham_pha(kham_pha).unwrap_or_else(|e| die(&format!("[server] Không mở được UDP {kham_pha}: {e}"))));
    ctrlc::set_handler(|| {
        println!("[server] Dừng server");
        std::process::exit(0);
    })
    .expect("Không cài được hàm bắt Ctrl+C");
    println!("[server] {ten}: Caro {N}×{N} tại {host}:{port}, khám phá UDP cổng {kham_pha}, mỗi nước tối đa {het_gio_ms} ms");
    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();
    thread::spawn(move || acceptor(srv, tx));
    let u2 = Arc::clone(&udp);
    thread::spawn(move || responder(u2, tx2));
    let mut s = Server { ten, port, het_gio: Duration::from_millis(het_gio_ms), udp, clients: HashMap::new(), rooms: Vec::new() };
    loop {
        let ev = match s.expired_room() {
            Some((ri, d)) if Instant::now() >= d => {
                s.on_timeout(ri); // có phòng hết giờ: xử lý trước
                continue;
            }
            Some((_, d)) => match rx.recv_timeout(d.saturating_duration_since(Instant::now())) {
                Ok(ev) => ev,
                Err(RecvTimeoutError::Timeout) => continue, // vòng sau sẽ thấy phòng hết giờ
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match rx.recv() {
                Ok(ev) => ev,
                Err(_) => return,
            },
        };
        match ev {
            Event::Open(id, conn, addr) => {
                s.clients.insert(id, Client { conn, addr, name: None, room: None, role: None });
            }
            Event::Msg(id, msg) => {
                if s.clients.contains_key(&id) {
                    s.on_message(id, msg);
                }
            }
            Event::Closed(id) => s.on_closed(id),
            Event::Discover(addr, q) => s.on_discover(addr, &q),
        }
    }
}
