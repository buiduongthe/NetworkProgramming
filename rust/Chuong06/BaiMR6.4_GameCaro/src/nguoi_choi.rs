//! Client Caro: người chơi thật (dòng lệnh), bot, hoặc người xem tự động.
//!
//! Kết nối: chỉ định host, port; hoặc --tim CỔNG để tìm server trong LAN bằng UDP broadcast (Bài 6.5).
//! Chế độ người chơi (không có --bot, --xem): gõ lệnh
//!     ds | tao <phòng> | vao <phòng> | xem <phòng> | <ô> (ví dụ h8) | lai | roi | thoat
//! Chế độ bot (--bot tan_cong|phong_thu|can_bang|tap_su|im_lang, kèm --tao hoặc --vao):
//!     --thu-loi: cố tình gửi nước sai (chưa đến lượt, ô đã có quân, ngoài bàn) để xem server kiểm tra.
//! Chế độ người xem tự động (--xem <phòng>): in tiến trình và bàn cờ cuối mỗi ván, thoát khi phòng đóng.

use crate::ban_co::{cell_name, choose_move, new_board, parse_cell, render, style, Board, Lcg, EMPTY, N};
use crate::giao_thuc::{chuoi, discover, recv_json, send_json};
use crate::{die, Args};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{self, BufRead};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn reason_text(r: &str) -> &'static str {
    match r {
        "five" => "5 quân liên tiếp",
        "timeout" => "đối thủ hết giờ",
        "left" => "đối thủ rời phòng",
        _ => "hòa (kín bàn)",
    }
}

fn t(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".into(),
        o => o.to_string(),
    }
}

fn over_text(m: &Value) -> String {
    let reason = reason_text(m["reason"].as_str().unwrap_or(""));
    let result = match m["winner"].as_str() {
        Some(w) => format!("{w} ({}) thắng – {reason}", t(&m["mark"])),
        None => reason.to_string(),
    };
    format!("Ván {} kết thúc sau {} nước: {result}", m["game_no"], m["moves"])
}

fn line_text(m: &Value) -> String {
    let cells: Vec<String> = m["line"].as_array().map(|a| a.iter().map(|p| cell_name(p[0].as_i64().unwrap_or(0), p[1].as_i64().unwrap_or(0))).collect()).unwrap_or_default();
    cells.join(" ")
}

/// Bàn cờ phía client – chỉ là bản sao để hiển thị, được cập nhật từ thông điệp của server.
struct View {
    board: Board,
    last: Option<(usize, usize)>,
    mark: Option<u8>,
    turn: Option<u8>,
}

impl View {
    fn new() -> View {
        View { board: new_board(), last: None, mark: None, turn: Some(b'X') }
    }

    fn apply(&mut self, m: &Value, me: &str) {
        let mk = |v: &Value| v.as_str().and_then(|s| s.bytes().next());
        match m["type"].as_str() {
            Some("start") => {
                self.board = new_board();
                self.last = None;
                self.turn = Some(b'X');
                self.mark = if m["x"] == me { Some(b'X') } else if m["o"] == me { Some(b'O') } else { None };
            }
            Some("board") => {
                // ảnh chụp bàn cờ cho người xem đến muộn
                if let Some(rows) = m["rows"].as_array() {
                    self.board = rows.iter().map(|r| r.as_str().unwrap_or("").bytes().collect()).collect();
                }
                self.turn = mk(&m["turn"]);
            }
            Some("moved") => {
                let (r, c) = (m["r"].as_u64().unwrap_or(0) as usize, m["c"].as_u64().unwrap_or(0) as usize);
                if r < N && c < N {
                    self.board[r][c] = mk(&m["mark"]).unwrap_or(b'?');
                    self.last = Some((r, c));
                }
                self.turn = mk(&m["next"]); // lượt kế do server quyết định (None: ván đã hết)
            }
            Some("over") => self.turn = None,
            _ => {}
        }
    }
}

struct Opts {
    tao: Option<String>,
    vao: Option<String>,
    xem: Option<String>,
    bot: Option<String>,
    seed: i64,
    thu_loi: bool,
    so_van: i64,
    tre_ms: u64,
}

fn connect(a: &Args, me: &str) -> TcpStream {
    let (mut host, mut port) = (a.pos.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into()), a.port(2, 9100));
    let tim = a.opt.get("tim").map(|p| p.parse::<u16>().unwrap_or_else(|_| die(&format!("--tim sai: {p}"))));
    if let Some(tp) = tim {
        println!("[{me}] Tìm server Caro bằng broadcast tới cổng {tp}...");
        let found = discover(tp, 800, "255.255.255.255");
        let Some(f) = found.first() else {
            eprintln!("[{me}] Không tìm thấy server nào");
            std::process::exit(1)
        };
        host = f.ip.clone();
        port = f.port as u16;
        println!("[{me}] Thấy {} tại {host}:{port}, kết nối", f.name);
    }
    let addr = (host.as_str(), port).to_socket_addrs().ok().and_then(|mut x| x.next());
    let s = match addr.map(|ad| TcpStream::connect_timeout(&ad, Duration::from_secs(3))) {
        Some(Ok(s)) => s,
        Some(Err(e)) => {
            eprintln!("[{me}] Không kết nối được: {e}");
            std::process::exit(1)
        }
        None => {
            eprintln!("[{me}] Không phân giải được {host}");
            std::process::exit(1)
        }
    };
    s.set_nodelay(true).ok(); // tắt Nagle: gửi thông điệp nhỏ ngay (Go bật sẵn)
    if tim.is_none() {
        println!("[{me}] Đã kết nối {host}:{port}");
    }
    s
}

fn room_req(o: &Opts) -> Value {
    match (&o.tao, &o.vao) {
        (Some(r), _) => json!({"type": "create", "room": r}),
        (_, r) => json!({"type": "join", "room": r}),
    }
}

// ---------------------------------------------------------------- bot
fn quit_room(s: &TcpStream, me: &str, games: i64) -> io::Result<()> {
    println!("[{me}] Đã chơi {games} ván, rời phòng");
    send_json(s, &json!({"type": "bye"})) // bye = rời phòng + rời sảnh trong một thông điệp
}

/// Bot phản ứng theo từng thông điệp của server.
/// Sau mỗi ván, người THUA (hoặc quân O nếu hòa) lên tiếng trước: xin chơi lại hoặc rời phòng; người thắng
/// chờ quyết định đó rồi mới trả lời. Nhờ quy ước này hai bot không "nói cùng lúc", nhật ký có thứ tự cố định.
fn run_bot(s: &TcpStream, me: &str, o: &Opts) -> io::Result<()> {
    let bot = o.bot.clone().unwrap_or_default();
    let st = style(&bot);
    let (mut view, mut games, mut tried) = (View::new(), 0i64, HashSet::new());
    let mut rng = Lcg::new(o.seed);
    let (mut waiting, mut in_room) = (false, false);
    loop {
        let Some(msg) = recv_json(s)? else { continue };
        let ty = msg["type"].as_str().unwrap_or("").to_string();
        view.apply(&msg, me);
        let mut want_more = games < o.so_van && bot != "im_lang";
        match ty.as_str() {
            "welcome" => {
                println!("[{me}] Vào sảnh; số phòng hiện có: {}", msg["rooms"].as_array().map_or(0, |r| r.len()));
                send_json(s, &room_req(o))?;
            }
            "joined" => {
                in_room = true;
                println!("[{me}] Vào phòng {}, cầm quân {}", t(&msg["room"]), t(&msg["role"]));
            }
            "error" => {
                println!("[{me}] Server báo lỗi {}: {}", t(&msg["code"]), t(&msg["msg"]));
                if !in_room {
                    // không tạo/vào được phòng: bot dừng
                    return send_json(s, &json!({"type": "bye"}));
                }
            }
            "start" => {
                let mk = view.mark.map(|m| (m as char).to_string()).unwrap_or_else(|| "None".into());
                println!("[{me}] Ván {} bắt đầu: X = {}, O = {} (tôi cầm {mk})", msg["game_no"], t(&msg["x"]), t(&msg["o"]));
                if o.thu_loi && view.mark == Some(b'O') && tried.insert("early") {
                    println!("[{me}] (thử lỗi) đánh h8 khi chưa đến lượt");
                    send_json(s, &json!({"type": "move", "r": 7, "c": 7}))?;
                }
            }
            "rematch_wait" if msg["by"] != me && waiting => {
                waiting = false;
                if !want_more {
                    return quit_room(s, me, games);
                }
                println!("[{me}] {} xin chơi lại – đồng ý", t(&msg["by"]));
                send_json(s, &json!({"type": "rematch"}))?;
            }
            "left" => {
                println!("[{me}] {} rời phòng", t(&msg["player"]));
                if waiting {
                    return quit_room(s, me, games);
                }
            }
            "over" => {
                games += 1;
                want_more = games < o.so_van && bot != "im_lang";
                println!("[{me}] {}", over_text(&msg));
                let leader = msg["winner"] == me || (msg["winner"].is_null() && view.mark == Some(b'X'));
                if msg["reason"] == "left" {
                    return quit_room(s, me, games); // đối thủ đã đi: không còn ai để chơi
                }
                if leader {
                    waiting = true;
                } else if want_more {
                    println!("[{me}] Xin chơi lại");
                    send_json(s, &json!({"type": "rematch"}))?;
                } else {
                    return quit_room(s, me, games);
                }
            }
            _ => {}
        }
        if (ty == "start" || ty == "moved") && view.mark.is_some() && view.turn == view.mark && bot != "im_lang" {
            thread::sleep(Duration::from_millis(o.tre_ms)); // "suy nghĩ" để người xem kịp theo dõi
            if let Some((r, c)) = view.last.filter(|_| o.thu_loi && !tried.contains("bad_cells")) {
                tried.insert("bad_cells");
                println!("[{me}] (thử lỗi) đánh vào ô đã có quân {} và ô ngoài bàn (15, 3)", cell_name(r as i64, c as i64));
                send_json(s, &json!({"type": "move", "r": r, "c": c}))?;
                send_json(s, &json!({"type": "move", "r": N, "c": 3}))?;
            }
            let (r, c) = choose_move(&view.board, view.mark.unwrap(), st.unwrap(), &mut rng);
            send_json(s, &json!({"type": "move", "r": r, "c": c}))?;
        }
    }
}

// ---------------------------------------------------------------- người xem tự động
fn run_watcher(s: &TcpStream, me: &str, room: &str) -> io::Result<()> {
    let mut view = View::new();
    loop {
        let Some(msg) = recv_json(s)? else { continue };
        view.apply(&msg, me);
        match msg["type"].as_str() {
            Some("welcome") => send_json(s, &json!({"type": "watch", "room": room}))?,
            Some("joined") => {
                let info = &msg["info"];
                let ten = |v: &Value| chuoi(v).unwrap_or_else(|| "chưa có".into());
                println!("[{me}] Xem phòng {} (X = {}, O = {})", t(&msg["room"]), ten(&info["x"]), ten(&info["o"]));
            }
            Some("error") => {
                println!("[{me}] Server báo lỗi {}: {}", t(&msg["code"]), t(&msg["msg"]));
                return Ok(());
            }
            Some("board") => println!("[{me}] Nhận ảnh chụp bàn cờ: ván {}, đã đi {} nước", msg["game_no"], msg["moves"]),
            Some("start") => println!("[{me}] Ván {}: X = {}, O = {}", msg["game_no"], t(&msg["x"]), t(&msg["o"])),
            Some("moved") if msg["move_no"].as_u64().unwrap_or(1) % 10 == 0 => {
                let cell = cell_name(msg["r"].as_i64().unwrap_or(0), msg["c"].as_i64().unwrap_or(0));
                println!("[{me}] Nước {}: {} ({}) {cell}", msg["move_no"], t(&msg["by"]), t(&msg["mark"]));
            }
            Some("over") => {
                println!("[{me}] {}", over_text(&msg));
                for line in render(&view.board, view.last) {
                    println!("{line}");
                }
                if msg["line"].as_array().is_some_and(|l| !l.is_empty()) {
                    println!("[{me}] Dãy thắng: {}", line_text(&msg));
                }
            }
            Some("left") => println!("[{me}] {} rời phòng", t(&msg["player"])),
            Some("room_closed") => {
                println!("[{me}] Phòng {} đã đóng", t(&msg["room"]));
                return send_json(s, &json!({"type": "bye"}));
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- người chơi thật
const HELP: &str = "Lệnh: ds | tao <phòng> | vao <phòng> | xem <phòng> | <ô> ví dụ h8 | lai | roi | thoat";

fn run_human(s: &TcpStream, me: &str, o: &Opts) -> io::Result<()> {
    let view = Arc::new(Mutex::new(View::new()));
    let quitting = Arc::new(AtomicBool::new(false));
    {
        let (view, quitting, me, rs) = (Arc::clone(&view), Arc::clone(&quitting), me.to_string(), s.try_clone()?);
        thread::spawn(move || {
            let show_board = |v: &View| {
                for line in render(&v.board, v.last) {
                    println!("{line}");
                }
                if let Some(m) = v.mark {
                    println!("[{me}] Bạn cầm {}; {}", m as char, if v.turn == v.mark { "ĐẾN LƯỢT BẠN" } else { "chờ đối thủ" });
                }
            };
            loop {
                let msg = match recv_json(&rs) {
                    Ok(Some(m)) => m,
                    Ok(None) => continue,
                    Err(_) => {
                        if !quitting.load(Ordering::SeqCst) {
                            println!("[{me}] Mất kết nối tới server"); // gõ thoat thì lỗi đọc sau đó là bình thường
                        }
                        return;
                    }
                };
                let mut v = view.lock().unwrap();
                v.apply(&msg, &me);
                match msg["type"].as_str() {
                    Some("welcome" | "rooms") => {
                        let rooms = msg["rooms"].as_array().cloned().unwrap_or_default();
                        println!("[{me}] Có {} phòng{}", rooms.len(), if rooms.is_empty() { "" } else { ":" });
                        for r in rooms {
                            let ten = |x: &Value| chuoi(x).unwrap_or_else(|| "chưa có".into());
                            println!("    {}: X = {}, O = {}, {} người xem, {}", t(&r["room"]), ten(&r["x"]), ten(&r["o"]), r["watchers"], t(&r["state"]));
                        }
                    }
                    Some("joined") => println!("[{me}] Vào phòng {} với vai trò {}", t(&msg["room"]), t(&msg["role"])),
                    Some("start") => {
                        println!("[{me}] Ván {}: X = {}, O = {}; mỗi nước tối đa {} s", msg["game_no"], t(&msg["x"]), t(&msg["o"]), msg["timeout_ms"].as_u64().unwrap_or(0) / 1000);
                        show_board(&v);
                    }
                    Some("moved" | "board") => show_board(&v),
                    Some("over") => {
                        let day = if msg["line"].as_array().is_some_and(|l| !l.is_empty()) { format!("; dãy {}", line_text(&msg)) } else { String::new() };
                        println!("[{me}] {}{day}", over_text(&msg));
                        println!("[{me}] Gõ 'lai' để xin chơi lại hoặc 'roi' để rời phòng");
                    }
                    Some("rematch_wait") => println!("[{me}] {} xin chơi lại", t(&msg["by"])),
                    Some("left") => println!("[{me}] {} rời phòng", t(&msg["player"])),
                    Some("room_closed") => println!("[{me}] Phòng {} đã đóng", t(&msg["room"])),
                    Some("error") => println!("[{me}] Lỗi {}: {}", t(&msg["code"]), t(&msg["msg"])),
                    _ => {}
                }
            }
        });
    }
    println!("[{me}] {HELP}");
    if o.tao.is_some() || o.vao.is_some() {
        send_json(s, &room_req(o))?;
    }
    for line in io::stdin().lock().lines().map_while(Result::ok) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let Some(cmd) = parts.first().map(|c| c.to_lowercase()) else { continue };
        let arg = parts.get(1).copied().unwrap_or("");
        match cmd.as_str() {
            "thoat" => break,
            "ds" => send_json(s, &json!({"type": "list"}))?,
            "tao" | "vao" | "xem" => {
                let kind = match cmd.as_str() {
                    "tao" => "create",
                    "vao" => "join",
                    _ => "watch",
                };
                send_json(s, &json!({"type": kind, "room": arg}))?;
            }
            "lai" => send_json(s, &json!({"type": "rematch"}))?,
            "roi" => send_json(s, &json!({"type": "leave"}))?,
            _ => match parse_cell(&cmd) {
                Some((r, c)) => {
                    if view.lock().unwrap().board[r][c] != EMPTY {
                        // kiểm tra sớm phía client; server vẫn kiểm tra lại
                        println!("[{me}] Ô {cmd} đã có quân");
                        continue;
                    }
                    send_json(s, &json!({"type": "move", "r": r, "c": c}))?;
                }
                None => println!("[{me}] {HELP}"),
            },
        }
    }
    quitting.store(true, Ordering::SeqCst);
    send_json(s, &json!({"type": "bye"}))
}

pub fn run(a: Args) {
    let me = a.pos.first().cloned().unwrap_or_else(|| die("Cách dùng: choi <tên> [host] [port] [--tim CỔNG] [--tao P | --vao P | --xem P] [--bot KIỂU] ..."));
    let o = Opts {
        tao: a.opt.get("tao").cloned(),
        vao: a.opt.get("vao").cloned(),
        xem: a.opt.get("xem").cloned(),
        bot: a.opt.get("bot").cloned(),
        seed: a.num("seed", 1),
        thu_loi: a.opt.contains_key("thu-loi"),
        so_van: a.num("so-van", 1),
        tre_ms: a.num("tre-ms", 300).max(0) as u64,
    };
    if [&o.tao, &o.vao, &o.xem].iter().filter(|x| x.is_some()).count() > 1 {
        die("Chỉ dùng một trong --tao, --vao, --xem");
    }
    if let Some(b) = &o.bot {
        if style(b).is_none() && b != "im_lang" {
            die(&format!("--bot phải là tan_cong, phong_thu, can_bang, tap_su hoặc im_lang (nhận '{b}')"));
        }
        if o.tao.is_none() && o.vao.is_none() {
            die("Bot cần --tao hoặc --vao");
        }
    }
    let s = connect(&a, &me);
    let r = send_json(&s, &json!({"type": "hello", "name": me})).and_then(|_| match (&o.xem, &o.bot) {
        (Some(room), _) => run_watcher(&s, &me, room),
        (None, Some(_)) => run_bot(&s, &me, &o),
        _ => run_human(&s, &me, &o),
    });
    if r.is_err() {
        println!("[{me}] Server đóng kết nối");
    }
}
