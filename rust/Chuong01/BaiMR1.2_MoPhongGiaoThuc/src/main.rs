//! Bài mở rộng MR1.2 – Mô phỏng giao thức và kiểu truyền: TCP, UDP, unicast, broadcast, multicast, anycast.
//!
//! Bài mở rộng, không có trong giáo trình; minh họa trực quan Chương 1 (mô hình TCP/IP, tầng giao vận,
//! các kiểu địa chỉ) trước khi lập trình socket thật ở các chương sau. Toàn bộ chạy trong bộ mô phỏng
//! sự kiện rời rạc (mang.rs): không mở socket, không cần quyền quản trị, kết quả tất định theo seed.
//!
//! Cách dùng:
//!     cargo run -q --release -- <kịch_bản> [--seed N] [--loss P] [--doan N] [--mat 20,20] [--html tệp.html]
//!     kịch_bản: tcp | udp | unicast | broadcast | multicast | anycast            (giao thức, kiểu truyền)
//!               dong_bo | da_luong | nhom_luong | bat_dong_bo | phoi_hop   (bên trong máy chủ)
//!               so_sanh (bảng so sánh 5 mô hình máy chủ) | tat_ca
//!     --seed  hạt giống bộ sinh ngẫu nhiên (mặc định 7)
//!     --loss  xác suất mất gói trên liên kết R2–R3, 0..1 (mặc định: 0; UDP: 0.2)
//!     --doan  số đoạn dữ liệu TCP (mặc định 40)
//!     --mat   các đoạn TCP bị ép mất; lặp số đoạn để mất nhiều lần (mặc định 20,20)
//!     --html  ghi tệp HTML hoạt hình tự chứa (JavaScript thuần, không cần mạng)

mod kich_ban;
mod mang;
mod may_chu;
mod tcp;

use mang::{Net, LINKS, NODES};
use serde_json::{json, Value};
use std::process;
use tcp::{TcpScenario, MSS};

/// Khung trang hoạt hình được nhúng vào chương trình lúc biên dịch.
const KHUNG_HTML: &str = include_str!("../khung_hoat_hinh.html");

/// Kịch bản giao thức, kiểu truyền; các kịch bản bên trong máy chủ nằm ở may_chu::TITLES.
const TITLES: [(&str, &str); 6] = [
    ("tcp", "TCP – bắt tay, cửa sổ trượt, cwnd, mất gói, đóng FIN"),
    ("udp", "UDP – datagram, mất gói, sai thứ tự, không gửi lại"),
    ("unicast", "Unicast – một nguồn tới một đích"),
    ("broadcast", "Broadcast – gửi tràn trong LAN, dừng ở router"),
    ("multicast", "Multicast – IGMP, chỉ giao cho thành viên nhóm"),
    ("anycast", "Anycast – máy phục vụ gần nhất, dự phòng khi hỏng"),
];

/// Mọi kịch bản theo thứ tự chạy của tat_ca: giao thức, kiểu truyền rồi tới bên trong máy chủ.
fn all_titles() -> impl Iterator<Item = (&'static str, &'static str)> {
    TITLES.into_iter().chain(may_chu::TITLES)
}

struct Options {
    kich_ban: String,
    seed: i64,
    loss: f64,
    doan: i64,
    mat: String,
    html: String,
}

fn usage(msg: &str) -> ! {
    eprintln!("Cách dùng: mo_phong <kịch_bản> [--seed N] [--loss P] [--doan N] [--mat 20,20] [--html tệp.html]");
    eprintln!("  kịch_bản: tcp | udp | unicast | broadcast | multicast | anycast            (giao thức, kiểu truyền)");
    eprintln!("            dong_bo | da_luong | nhom_luong | bat_dong_bo | phoi_hop   (bên trong máy chủ)");
    eprintln!("            so_sanh (bảng so sánh 5 mô hình máy chủ) | tat_ca");
    eprintln!("lỗi: {msg}");
    process::exit(2);
}

/// Đọc số từ giá trị tham số; sai thì in cách dùng và thoát.
fn num<T: std::str::FromStr>(name: &str, val: &str) -> T {
    val.parse().unwrap_or_else(|_| usage(&format!("giá trị sai cho --{name}: {val}")))
}

fn parse_args() -> Options {
    let mut o = Options { kich_ban: String::new(), seed: 7, loss: -1.0, doan: 40, mat: "20,20".into(), html: String::new() };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(name) = a.strip_prefix("--") {
            // nhận cả "--seed 3" và "--seed=3"
            let (name, val) = match name.split_once('=') {
                Some((n, v)) => (n.to_string(), v.to_string()),
                None => {
                    i += 1;
                    (name.to_string(), args.get(i).cloned().unwrap_or_else(|| usage(&format!("--{name} thiếu giá trị"))))
                }
            };
            match name.as_str() {
                "seed" => o.seed = num(&name, &val),
                "loss" => o.loss = num(&name, &val),
                "doan" => o.doan = num(&name, &val),
                "mat" => o.mat = val.clone(),
                "html" => o.html = val.clone(),
                _ => usage(&format!("tham số lạ --{name}")),
            }
        } else if o.kich_ban.is_empty() {
            o.kich_ban = a.clone();
        } else {
            usage(&format!("thừa tham số {a}"));
        }
        i += 1;
    }
    if o.kich_ban != "tat_ca" && o.kich_ban != "so_sanh" && !all_titles().any(|t| t.0 == o.kich_ban) {
        usage(&format!("kịch bản không hợp lệ: '{}'", o.kich_ban));
    }
    o
}

fn run_one(name: &'static str, title: &str, o: &Options) -> Value {
    println!("=== Kịch bản {name} (seed {}) ===", o.seed);
    println!("{title}");
    let mut net = Net::new(o.seed);
    let loss_set = o.loss >= 0.0;
    if loss_set {
        net.link_mut("R2", "R3").loss = (o.loss * 1000.0 + 0.5) as i64;
    }
    if may_chu::TITLES.iter().any(|t| t.0 == name) {
        let extra = may_chu::chay(&mut net, name);
        println!();
        let mut run = json!({"id": name, "title": title, "hops": net.hops, "notes": net.notes, "changes": net.changes});
        run.as_object_mut().unwrap().extend(extra);
        return run;
    }
    let mut cwnd = None;
    match name {
        "tcp" => {
            let drops: Vec<i64> = o.mat.split(',').filter(|x| !x.trim().is_empty()).map(|x| x.trim().parse().unwrap_or(0)).collect();
            let list: Vec<String> = drops.iter().map(|d| format!("#{d}")).collect();
            let list = if list.is_empty() { "không".to_string() } else { list.join(", ") };
            println!("Truyền {} đoạn × {MSS} byte từ H1 tới H5; ép mất đoạn: {list}", o.doan);
            let mut sc = TcpScenario::new(&mut net, o.doan, drops);
            sc.start(&mut net);
            net.run(&mut sc);
            sc.report();
            cwnd = Some(sc.cwnd_log.iter().map(|&(t, v)| json!([t, v])).collect::<Vec<_>>());
        }
        "udp" => kich_ban::udp(&mut net, loss_set),
        "unicast" => kich_ban::unicast(&mut net),
        "broadcast" => kich_ban::broadcast(&mut net),
        "multicast" => kich_ban::multicast(&mut net),
        _ => kich_ban::anycast(&mut net),
    }
    println!();
    net.summary();
    println!();
    let mut run = json!({"id": name, "title": title, "hops": net.hops, "notes": net.notes, "changes": net.changes});
    if let Some(c) = cwnd {
        run["cwnd"] = Value::Array(c);
    }
    run
}

fn write_html(path: &str, runs: Vec<Value>) {
    let nodes: Vec<Value> = NODES.iter().map(|n| json!({"name": n.0, "kind": n.1, "ip": n.2, "x": n.4, "y": n.5})).collect();
    let links: Vec<Value> = LINKS.iter().map(|l| json!({"a": l.0, "b": l.1, "cost": l.2})).collect();
    let data = json!({"nodes": nodes, "links": links, "runs": runs});
    // "</" trong chuỗi JSON có thể đóng nhầm thẻ <script>, nên đổi thành "<\/"
    let js = serde_json::to_string(&data).unwrap().replace("</", "<\\/");
    if let Err(e) = std::fs::write(path, KHUNG_HTML.replace("{{.Data}}", &js)) {
        eprintln!("Không ghi được {path}: {e}");
        process::exit(1);
    }
    println!("Đã ghi hoạt hình HTML: {path}");
}

fn main() {
    let o = parse_args();
    if o.kich_ban == "so_sanh" {
        let kq: Vec<_> = may_chu::TITLES.iter().map(|&(m, _)| (m, may_chu::chi_so(&mut Net::new(o.seed), m))).collect();
        may_chu::so_sanh(&kq);
        return;
    }
    let mut runs = Vec::new();
    for (name, title) in all_titles() {
        if o.kich_ban == "tat_ca" || o.kich_ban == name {
            runs.push(run_one(name, title, &o));
        }
    }
    if !o.html.is_empty() {
        write_html(&o.html, runs);
    }
}
