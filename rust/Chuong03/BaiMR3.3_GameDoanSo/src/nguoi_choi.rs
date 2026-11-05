//! Client trò chơi đoán số: người chơi thật (bàn phím) hoặc bot.
//!
//! Chế độ người chơi: một luồng nhận thông điệp của server và in ra; luồng chính chỉ hỏi số khi đến lượt.
//! Client cũng kiểm tra đầu vào (phải là số nguyên) để báo lỗi sớm cho người dùng, nhưng server vẫn
//! kiểm tra lại mọi thứ – server không bao giờ tin client.
//!
//! Các bot (dùng cho demo tất định):
//!   chia_doi   – tìm kiếm nhị phân: đoán điểm giữa khoảng [lo, hi] server gửi kèm lượt.
//!   ngau_nhien – đoán ngẫu nhiên trong [lo, hi] (bộ sinh LCG, hạt giống --seed).
//!   nghich     – cố tình gửi thông điệp sai để xem server kiểm tra: đoán khi ván chưa bắt đầu,
//!                khung không phải JSON, đoán khi chưa đến lượt, giá trị là chuỗi, số ngoài khoảng.
//!   im_lang    – không bao giờ trả lời: bị server tính hết giờ, rồi tự ngắt kết nối.

use crate::giao_thuc::{recv_json, send_frame, send_json, text, Lcg};
use crate::{die, Args};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{self, BufRead, IsTerminal, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn hint_text(h: &str) -> String {
    match h {
        "THAP" => "thấp quá".into(),
        "CAO" => "cao quá".into(),
        "DUNG" => "ĐÚNG".into(),
        other => other.into(),
    }
}

/// Chuyển thông điệp của server thành dòng chữ cho người chơi; trả None nếu không cần in.
fn describe(me: &str, m: &Value) -> Option<String> {
    let f = |k: &str| text(m.get(k).unwrap_or(&Value::Null));
    let t = f("type");
    match t.as_str() {
        "welcome" => Some(format!("[{me}] Vào phòng chờ: {}/{} người, đoán số trong {}–{}", f("waiting"), f("need"), f("min"), f("max"))),
        "start" => {
            let order: Vec<String> = m["order"].as_array().map(|a| a.iter().map(text).collect()).unwrap_or_default();
            Some(format!("[{me}] Ván {} bắt đầu, thứ tự: {}", f("round"), order.join(", ")))
        }
        "turn" if f("player") == me => Some(format!("[{me}] Đến lượt bạn (lượt {}), khoảng {}–{}", f("turn_no"), f("lo"), f("hi"))),
        "result" => Some(format!("[{me}] {} đoán {}: {}", f("player"), f("value"), hint_text(&f("hint")))),
        "timeout" => Some(format!("[{me}] {} hết giờ, mất lượt", f("player"))),
        "left" => Some(format!("[{me}] {} đã rời trò chơi", f("player"))),
        "error" => Some(format!("[{me}] Server báo lỗi {}: {}", f("code"), f("msg"))),
        "game_over" => {
            let who = m["winner"].as_str().filter(|w| !w.is_empty()).unwrap_or("không ai").to_string();
            Some(format!("[{me}] Kết thúc: {who} thắng, số bí mật là {} ({} lượt)", f("secret"), f("turns")))
        }
        "bye" => Some(format!("[{me}] Server kết thúc phiên")),
        _ => None,
    }
}

fn say(line: Option<String>) {
    if let Some(l) = line {
        println!("{l}");
    }
}

// ---------------------------------------------------------------- bot
fn run_bot(s: &mut TcpStream, me: &str, kind: &str, seed: i64, think_ms: u64) -> io::Result<()> {
    let mut rng = Lcg::new(seed);
    let mut tricks_done: HashSet<&str> = HashSet::new(); // các "trò nghịch" đã làm (mỗi trò một lần)
    loop {
        let Some(msg) = recv_json(s)? else { continue };
        say(describe(me, &msg));
        let t = text(&msg["type"]);
        if t == "game_over" || t == "bye" {
            return Ok(());
        }
        if kind == "nghich" && t == "welcome" {
            println!("[{me}] (nghịch) đoán khi ván chưa bắt đầu, rồi gửi khung không phải JSON");
            send_json(s, &json!({"type": "guess", "value": 1}))?;
            send_frame(s, "xin chào".as_bytes())?;
        }
        let player = text(&msg["player"]);
        if t == "timeout" && player == me && kind == "im_lang" {
            println!("[{me}] (im lặng) bị hết giờ, ngắt kết nối");
            return Ok(());
        }
        if t != "turn" {
            continue;
        }
        let (lo, hi) = (msg["lo"].as_i64().unwrap_or(0), msg["hi"].as_i64().unwrap_or(0));
        if player != me {
            if kind == "nghich" && tricks_done.insert("out_of_turn") {
                println!("[{me}] (nghịch) đoán khi đang là lượt của {player}");
                send_json(s, &json!({"type": "guess", "value": lo}))?;
            }
            continue;
        }
        if kind == "im_lang" {
            continue;
        }
        thread::sleep(Duration::from_millis(think_ms)); // "suy nghĩ" để người xem kịp theo dõi
        if kind == "nghich" && tricks_done.insert("bad_values") {
            println!("[{me}] (nghịch) gửi giá trị chuỗi và số ngoài khoảng");
            send_json(s, &json!({"type": "guess", "value": "năm mươi"}))?;
            send_json(s, &json!({"type": "guess", "value": hi + 50}))?;
        }
        let guess = if kind == "ngau_nhien" { rng.next_int(lo, hi) } else { (lo + hi).div_euclid(2) };
        println!("[{me}] Đoán {guess}");
        send_json(s, &json!({"type": "guess", "value": guess}))?;
    }
}

// ---------------------------------------------------------------- người chơi thật
fn run_human(s: &TcpStream, me: &str) -> io::Result<()> {
    let (tx, turns) = mpsc::channel::<bool>(); // luồng nhận báo cho luồng chính: true = "turn", false = "end"
    let mut rs = s.try_clone()?;
    let me2 = me.to_string();
    thread::spawn(move || {
        while let Ok(m) = recv_json(&mut rs) {
            let Some(msg) = m else { continue };
            say(describe(&me2, &msg));
            let t = text(&msg["type"]);
            if t == "turn" && text(&msg["player"]) == me2 {
                let _ = tx.send(true);
            }
            if t == "game_over" || t == "bye" {
                break;
            }
        }
        let _ = tx.send(false);
    });
    let echo = !io::stdin().is_terminal(); // đầu vào từ tệp/ống: in lại để nhật ký dễ đọc
    let mut lines = io::stdin().lock().lines();
    while turns.recv().unwrap_or(false) {
        loop {
            print!("[{me}] Nhập số: ");
            io::stdout().flush()?;
            let Some(Ok(line)) = lines.next() else {
                // hết đầu vào (Ctrl+D / Ctrl+Z): rời trò chơi
                println!();
                send_json(s, &json!({"type": "bye"}))?;
                return Ok(());
            };
            let line = line.trim().to_string();
            if echo {
                println!("{line}");
            }
            match line.parse::<i64>() {
                // kiểm tra phía client: chỉ để báo lỗi sớm
                Ok(value) => {
                    send_json(s, &json!({"type": "guess", "value": value}))?;
                    break;
                }
                Err(_) => println!("[{me}] Vui lòng nhập một số nguyên"),
            }
        }
    }
    Ok(())
}

pub fn run(args: Args) {
    let me = args.pos.first().cloned().unwrap_or_else(|| die("Cách dùng: choi <tên> [host] [port] [--bot KIỂU] [--seed N] [--nghi-ms 300]"));
    let host = args.pos.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let port: u16 = args.pos.get(2).map(|p| p.parse().unwrap_or_else(|_| die(&format!("Cổng sai: {p}")))).unwrap_or(9000);
    let bot = args.opt.get("bot").cloned();
    if let Some(b) = &bot {
        if !["chia_doi", "ngau_nhien", "nghich", "im_lang"].contains(&b.as_str()) {
            die(&format!("--bot phải là chia_doi, ngau_nhien, nghich hoặc im_lang (nhận '{b}')"));
        }
    }
    let seed = args.num("seed", 1);
    let think_ms = args.num("nghi-ms", 300).max(0) as u64;
    let addr = (host.as_str(), port).to_socket_addrs().ok().and_then(|mut a| a.next());
    let mut s = match addr.map(|a| TcpStream::connect_timeout(&a, Duration::from_secs(3))) {
        Some(Ok(s)) => s,
        Some(Err(e)) => {
            eprintln!("[{me}] Không kết nối được {host}:{port}: {e}");
            process::exit(1)
        }
        None => {
            eprintln!("[{me}] Không phân giải được {host}");
            process::exit(1)
        }
    };
    s.set_nodelay(true).ok(); // tắt Nagle: gửi thông điệp nhỏ ngay (Go bật sẵn)
    println!("[{me}] Đã kết nối {host}:{port}");
    let r = send_json(&s, &json!({"type": "hello", "name": me})).and_then(|_| match &bot {
        Some(b) => run_bot(&mut s, &me, b, seed, think_ms),
        None => run_human(&s, &me),
    });
    if r.is_err() {
        println!("[{me}] Server đóng kết nối");
    }
}
