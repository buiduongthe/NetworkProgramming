//! Client phòng hội thoại video.
//!
//! Vòng gửi: theo tốc độ khung cho phép (min(fps camera, fps tối đa server đặt)), lấy một khung từ
//! camera tổng hợp, giảm độ phân giải theo yêu cầu của server, nén zlib, chia mảnh ≤ 1200 byte, gửi UDP.
//! Luồng nhận: ghép mảnh thành khung lưới do server gửi, giải nén, giữ khung mới nhất.
//! Mỗi giây in fps và kbit/s gửi/nhận, số mảnh mất. Lệnh `chup <nhãn>` lưu khung lưới đang xem
//! thành PNG (out/chup_N.png); khi kết thúc, --ghep tệp.png ghép mọi ảnh đã chụp kèm nhãn.
//! Thư viện chuẩn Rust không đọc được camera thật; bản Rust không có --camera (cần crate nokhwa…).
//!
//! Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi client chạy):
//!   tat_camera   bat_camera   nhan_fps <n>   nhan_do_phan_giai <1|2>   chup <nhãn>   thoat

use crate::chung::*;
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

struct ChiaSe {
    mo: bool,
    fps: u32,
    chia: usize,
    dung: bool,
    ghep: GhepKhung,
    xem: Option<(usize, usize, Vec<u8>)>, // khung lưới mới nhất: (w, h, rgb)
    khung_nhan: u64,
    byte_nhan: u64,
    anh_chup: Vec<(usize, usize, Vec<u8>, String)>,
}

/// Ghép các ảnh chụp (rộng, cao, dữ liệu RGB, nhãn) thành một ảnh: 2 cột, nhãn dưới mỗi ảnh.
fn ghep_anh(duong_dan: &str, ds: &[(usize, usize, Vec<u8>, String)]) -> io::Result<(usize, usize)> {
    let cot = 2;
    let o_w = ds.iter().map(|d| d.0).max().unwrap_or(0) + 16;
    let cao_hang: Vec<usize> = ds.chunks(cot).map(|c| c.iter().map(|d| d.1).max().unwrap_or(0) + 34).collect();
    let (ww, hh) = (cot * o_w + 16, cao_hang.iter().sum::<usize>() + 8);
    let mut anh = vec![0xFFu8; ww * hh * 3];
    for (n, (w, h, du_lieu, nhan)) in ds.iter().enumerate() {
        let x0 = 16 + (n % cot) * o_w + (o_w - 16 - w) / 2;
        let y0 = 8 + cao_hang[..n / cot].iter().sum::<usize>();
        for y in 0..*h {
            anh[((y0 + y) * ww + x0) * 3..((y0 + y) * ww + x0 + w) * 3].copy_from_slice(&du_lieu[y * w * 3..(y + 1) * w * 3]);
        }
        let nhan: String = nhan.chars().take(52).collect();
        ve_chu(&mut anh, ww, hh, 16 + (n % cot) * o_w, y0 + h + 6, &nhan, 1, [0, 0, 0]);
    }
    ghi_png(duong_dan, &anh, ww, hh)?;
    Ok((ww, hh))
}

pub fn run(args: &[String]) {
    // tham số: <tên> <host> <cổng_tcp> [--fps 10] [--thoi-gian 10] [--mat-goi 0] [--ghep tệp.png] [--camera]
    let (mut pos, mut opt) = (Vec::new(), std::collections::HashMap::new());
    let mut i = 0;
    while i < args.len() {
        match args[i].strip_prefix("--") {
            Some("camera") => {
                opt.insert("camera".to_string(), String::new());
                i += 1;
            }
            Some(k) => {
                opt.insert(k.to_string(), args.get(i + 1).cloned().unwrap_or_default());
                i += 2;
            }
            None => {
                pos.push(args[i].clone());
                i += 1;
            }
        }
    }
    if pos.len() < 3 {
        eprintln!("Cách dùng: khach <tên> <host> <cổng_tcp> [--fps 10] [--thoi-gian 10] [--mat-goi 0.02] [--ghep tệp.png]");
        std::process::exit(2);
    }
    let num = |k: &str, d: f64| opt.get(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
    let (ten, host) = (pos[0].clone(), pos[1].clone());
    let cong: u16 = pos[2].parse().unwrap_or(41630);
    let (fps_cam, thoi_gian, mat_goi) = (num("fps", 10.0).max(1.0) as u32, num("thoi-gian", 10.0), num("mat-goi", 0.0));
    let ghep_ra = opt.get("ghep").cloned();
    let tag = format!("[khach {ten}]");
    let log = |text: &str| println!("{tag} {text}");
    std::sync::LazyLock::force(&T0); // khởi động đồng hồ chung ngay khi client chạy

    // ---- Kênh điều khiển TCP
    let addr = (host.as_str(), cong).to_socket_addrs().ok().and_then(|mut x| x.next()).expect("không phân giải được host");
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).unwrap_or_else(|e| {
        log(&format!("Không kết nối được: {e}"));
        std::process::exit(1)
    });
    tcp.set_nodelay(true).ok();
    let mut lines = BufReader::new(tcp.try_clone().unwrap()).lines();
    gui_json(&tcp, &json!({"loai": "vao", "ten": ten})).expect("gửi vào phòng");
    let chao: Value = lines.next().and_then(|l| l.ok()).and_then(|l| serde_json::from_str(&l).ok()).unwrap_or(Value::Null);
    let ma = chao["ma"].as_u64().unwrap_or(0) as u16;
    let camera = CameraTongHop::new(&ten, ma);
    if opt.contains_key("camera") {
        log("Bản Rust không đọc được camera thật; dùng camera tổng hợp");
    }
    log(&format!("Vào phòng, mã {ma}; camera tổng hợp {RONG}x{CAO}, {fps_cam} fps"));

    let udp_dich = (host.as_str(), chao["cong_udp"].as_u64().unwrap_or(0) as u16).to_socket_addrs().unwrap().next().unwrap();
    let cuc_bo = if udp_dich.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
    let udp = Arc::new(udp_dem_lon(cuc_bo.parse().unwrap()).expect("udp"));
    udp.connect(udp_dich).expect("udp connect"); // chỉ nhận datagram từ server

    let cs = Arc::new(Mutex::new(ChiaSe {
        mo: false,
        fps: fps_cam,
        chia: 1,
        dung: false,
        ghep: GhepKhung::new(),
        xem: None,
        khung_nhan: 0,
        byte_nhan: 0,
        anh_chup: Vec::new(),
    }));
    let t0 = bay_gio();

    // luồng đọc kênh điều khiển
    {
        let (cs, tag) = (Arc::clone(&cs), tag.clone());
        thread::spawn(move || {
            for line in lines.map_while(Result::ok) {
                let Ok(tin) = serde_json::from_str::<Value>(&line) else { break };
                match tin["loai"].as_str() {
                    Some("camera") => {
                        let mut c = cs.lock().unwrap();
                        c.mo = tin["mo"].as_bool().unwrap_or(false);
                        c.fps = tin["fps"].as_u64().unwrap_or(10).max(1) as u32;
                        c.chia = tin["chia"].as_u64().unwrap_or(1).max(1) as usize;
                        drop(c);
                        println!("{tag} Camera: {}, gửi tối đa {} fps, 1/{}", tin["ly_do"].as_str().unwrap_or(""), tin["fps"], tin["chia"]);
                    }
                    Some("nhan") => println!("{tag} Nhận: {} fps, độ phân giải 1/{}", tin["fps"], tin["chia"]),
                    Some("su_kien") => println!("{tag} Sự kiện: {}", tin["noi_dung"].as_str().unwrap_or("")),
                    Some("tu_choi") => println!("{tag} Bị từ chối: {}", tin["ly_do"].as_str().unwrap_or("")),
                    _ => {}
                }
            }
            cs.lock().unwrap().dung = true;
        });
    }
    // luồng nhận lưới từ server
    {
        let (cs, udp) = (Arc::clone(&cs), Arc::clone(&udp));
        thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while let Ok(n) = udp.recv(&mut buf) {
                let Some(m) = tach_header(&buf[..n]).filter(|m| m.loai == LOAI_MANH) else { continue };
                let khung = cs.lock().unwrap().ghep.them(&m, bay_gio());
                if let Some((w, h, du_lieu)) = khung {
                    let Some(anh) = giai_nen(&du_lieu) else { continue };
                    let mut c = cs.lock().unwrap();
                    c.xem = Some((w as usize, h as usize, anh));
                    c.khung_nhan += 1;
                    c.byte_nhan += du_lieu.len() as u64;
                }
            }
        });
    }
    // luồng đọc lệnh bàn phím
    {
        let (cs, tag, tcp) = (Arc::clone(&cs), tag.clone(), tcp.try_clone().unwrap());
        thread::spawn(move || {
            for dong in io::stdin().lock().lines().map_while(Result::ok) {
                let mut dong = dong.split('#').next().unwrap_or("").trim().to_string();
                if let Some(r) = dong.strip_prefix('@') {
                    let (moc, lenh) = r.split_once(' ').unwrap_or((r, ""));
                    let cho = moc.parse::<f64>().unwrap_or(0.0) - (bay_gio() - t0);
                    if cho > 0.0 {
                        thread::sleep(Duration::from_secs_f64(cho));
                    }
                    dong = lenh.trim().to_string();
                }
                if dong.is_empty() {
                    continue;
                }
                println!("{tag} > {dong}");
                let w: Vec<&str> = dong.split_whitespace().collect();
                match w.as_slice() {
                    ["tat_camera"] => {
                        // tự tắt: có hiệu lực ngay
                        cs.lock().unwrap().mo = false;
                        let _ = gui_json(&tcp, &json!({"loai": "tu_tat_camera"}));
                    }
                    ["bat_camera"] => {
                        let _ = gui_json(&tcp, &json!({"loai": "tu_bat_camera"})); // tự bật: chờ server cho phép
                    }
                    ["nhan_fps", n] if n.parse::<i64>().is_ok() => {
                        let _ = gui_json(&tcp, &json!({"loai": "nhan", "fps": n.parse::<i64>().unwrap()}));
                    }
                    ["nhan_do_phan_giai", n] if n.parse::<i64>().is_ok() => {
                        let _ = gui_json(&tcp, &json!({"loai": "nhan", "chia": n.parse::<i64>().unwrap()}));
                    }
                    ["chup", nhan @ ..] => {
                        let mut c = cs.lock().unwrap();
                        let Some((w, h, anh)) = c.xem.clone() else {
                            println!("{tag} chưa có hình để chụp");
                            continue;
                        };
                        let _ = std::fs::create_dir_all("out");
                        let duong = format!("out/chup_{}.png", c.anh_chup.len() + 1);
                        if let Err(e) = ghi_png(&duong, &anh, w, h) {
                            println!("{tag} không ghi được {duong}: {e}");
                            continue;
                        }
                        let nhan = if nhan.is_empty() { duong.clone() } else { nhan.join(" ") };
                        c.anh_chup.push((w, h, anh, nhan));
                        println!("{tag} Đã chụp {duong} ({w}x{h})");
                    }
                    ["thoat"] => cs.lock().unwrap().dung = true,
                    _ => println!("{tag} lệnh không hợp lệ"),
                }
            }
        });
    }

    // ---- Vòng gửi khung theo fps cho phép; mỗi giây in thống kê
    let mut rng = Rng::new(ma as u64 + 1000);
    let _ = udp.send(&goi_giu(ma)); // đăng ký địa chỉ UDP với server
    let (mut so_khung, mut gui_khung, mut gui_manh, mut bo_gia_lap) = (0u32, 0u64, 0u64, 0u64);
    let (mut dem_gui, mut byte_gui) = (0u64, 0u64);
    let mut han_gui = bay_gio();
    let mut moc_do = han_gui;
    let mut han_giu = moc_do + 1.0;
    let mut giay = 0;
    log("Mỗi giây: fps và kbit/s gửi | nhận, số mảnh mất");
    while bay_gio() - t0 < thoi_gian {
        let now = bay_gio();
        let (mo, fps, chia) = {
            let c = cs.lock().unwrap();
            if c.dung {
                break;
            }
            (c.mo, fps_cam.min(c.fps), c.chia)
        };
        if mo && now >= han_gui {
            han_gui = (han_gui + 1.0 / fps as f64).max(now);
            let anh = camera.lay_khung(so_khung);
            let (anh, w, h) = thu_nho(&anh, RONG, CAO, chia); // giảm độ phân giải trước khi nén
            let goi_nen = nen(&anh, MUC_NHANH);
            for g in chia_manh(ma, so_khung, w, h, &goi_nen) {
                gui_manh += 1;
                if rng.random() < mat_goi {
                    bo_gia_lap += 1; // "mạng" làm rơi mảnh
                } else {
                    let _ = udp.send(&g);
                }
            }
            so_khung += 1;
            gui_khung += 1;
            dem_gui += 1;
            byte_gui += goi_nen.len() as u64;
        } else if !mo && now >= han_giu {
            let _ = udp.send(&goi_giu(ma)); // giữ kết nối khi tắt camera
            han_giu = now + 1.0;
        }
        if now - moc_do >= 1.0 {
            moc_do += 1.0;
            giay += 1;
            let (kn, bn, mat) = {
                let mut c = cs.lock().unwrap();
                let r = (c.khung_nhan, c.byte_nhan, c.ghep.manh_mat);
                c.khung_nhan = 0;
                c.byte_nhan = 0;
                r
            };
            log(&format!(
                "{giay:2}s gửi {dem_gui:2} fps {:4.0} | nhận {kn:2} fps {:5.0}, mất {mat}",
                byte_gui as f64 * 8.0 / 1000.0,
                bn as f64 * 8.0 / 1000.0
            ));
            dem_gui = 0;
            byte_gui = 0;
        }
        thread::sleep(Duration::from_millis(2));
    }

    // ---- Kết thúc
    let tong_giay = bay_gio() - t0;
    let _ = tcp.shutdown(std::net::Shutdown::Both);
    log(&format!("Kết thúc sau {tong_giay:.1} s"));
    log(&format!("Gửi {gui_khung} khung = {gui_manh} mảnh, giả lập mất {bo_gia_lap} mảnh"));
    let c = cs.lock().unwrap();
    let g = &c.ghep;
    log(&format!("Nhận {} khung đủ, hỏng {}, mất hẳn {}, mảnh mất {}", g.khung_du, g.khung_hong, g.khung_mat_han, g.manh_mat));
    if let Some(ra) = ghep_ra.filter(|_| !c.anh_chup.is_empty()) {
        match ghep_anh(&ra, &c.anh_chup) {
            Ok((w, h)) => log(&format!("Đã ghép {} ảnh chụp vào {ra} ({w}x{h})", c.anh_chup.len())),
            Err(e) => log(&format!("Không ghi được {ra}: {e}")),
        }
    }
}
