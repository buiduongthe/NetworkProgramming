//! Client phòng hội thoại âm thanh.
//!
//! Mỗi 20 ms client làm hai việc trong cùng một vòng lặp có nhịp cố định:
//!   1. Thu: lấy 320 mẫu từ nguồn âm (mặc định sóng sin tổng hợp – mỗi client một tần số khác nhau
//!      để phân tích được ai đang nói; hoặc tệp WAV), đóng gói RTP, gửi UDP tới server nếu mic mở.
//!   2. Phát: lấy một khung từ bộ đệm jitter (luồng trộn server gửi về), nhân âm lượng nghe của chính
//!      client, ghi vào bản ghi (--ghi tệp.wav).
//! Mỗi giây in thanh đo mức (dBFS). Sự kiện từ server được đánh mốc theo vị trí trong bản ghi
//! (tệp <tên>_moc.json) để `phan_tich` chia đoạn và chứng minh lệnh điều khiển có hiệu lực.
//! Thư viện chuẩn Rust không truy cập được micro/loa; bản Rust không có --mic/--loa (cần crate cpal).
//!
//! Lệnh (stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ khi client chạy):
//!   tat_mic   bat_mic   am_luong <hệ số>   thoat

use crate::chung::*;
use crate::Args;
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader};
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Nguồn âm: sin tổng hợp (pha liên tục giữa các khung) hoặc tệp WAV phát lặp.
enum Nguon {
    Sin { f: f64, a: f64, n: u64 },
    Wav { mau: Vec<i16>, vt: usize },
}

impl Nguon {
    fn lay_khung(&mut self) -> Vec<i16> {
        match self {
            Nguon::Sin { f, a, n } => {
                let w = 2.0 * std::f64::consts::PI * *f / TAN_SO_MAU as f64;
                let k = (0..MAU_MOI_KHUNG).map(|i| (*a * (w * (*n + i as u64) as f64).sin()).round_ties_even() as i16).collect();
                *n += MAU_MOI_KHUNG as u64;
                k
            }
            Nguon::Wav { mau, vt } => {
                let k = (0..MAU_MOI_KHUNG).map(|i| mau[(*vt + i) % mau.len()]).collect();
                *vt += MAU_MOI_KHUNG;
                k
            }
        }
    }
}

/// Bộ sinh số giả ngẫu nhiên nhỏ (xorshift64*) để giả lập mất gói tất định theo mã client.
struct Rng(u64);

impl Rng {
    fn random(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Trạng thái dùng chung giữa luồng chính (vòng 20 ms) và các luồng TCP, UDP, bàn phím.
struct ChiaSe {
    mic_mo: bool,
    am_luong: f64,
    dung: bool,
    bo_dem: BoDemJitter,
    ban_ghi: Vec<i16>,
    moc: Vec<Value>, // {"t": giây trong bản ghi, "su_kien": ...}
}

fn so_g(x: f64) -> String {
    // in số như định dạng {:g} của Python: 440.0 -> "440", 0.25 -> "0.25"
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

pub fn run(a: Args) {
    if a.pos.len() < 3 {
        eprintln!("Cách dùng: khach <tên> <host> <cổng_tcp> [--tan-so 440] [--wav tệp] [--ghi tệp.wav] [--thoi-gian 10] [--mat-goi 0] [--dem 3]");
        std::process::exit(2);
    }
    let (ten, host) = (a.pos[0].clone(), a.pos[1].clone());
    let cong: u16 = a.pos[2].parse().unwrap_or(41620);
    let tag = format!("[khach {ten}]");
    let log = |text: &str| println!("{tag} {text}");
    let (tan_so, bien_do) = (a.f64("tan-so", 440.0), a.f64("bien-do", 0.25));
    let (thoi_gian, mat_goi) = (a.f64("thoi-gian", 10.0), a.f64("mat-goi", 0.0));
    let dem = a.f64("dem", 3.0) as usize;
    let ghi = a.opt.get("ghi").cloned();

    // ---- Nguồn âm
    let mut nguon = Nguon::Sin { f: tan_so, a: bien_do * 32767.0, n: 0 };
    let mut mo_ta = format!("sin {} Hz, biên độ {}", so_g(tan_so), so_g(bien_do));
    if let Some(w) = a.opt.get("wav") {
        match doc_wav(w) {
            Ok(mau) if !mau.is_empty() => {
                nguon = Nguon::Wav { mau, vt: 0 };
                mo_ta = format!("tệp {w}");
            }
            _ => log(&format!("Không đọc được {w} (cần WAV PCM 16 bit, mono, 16000 Hz); dùng sóng sin")),
        }
    }
    if a.opt.contains_key("mic") || a.opt.contains_key("loa") {
        log("Bản Rust không hỗ trợ micro/loa thật (cần crate cpal); dùng nguồn tổng hợp/WAV, không phát loa");
    }

    // ---- Kênh điều khiển TCP: vào phòng
    let addr = (host.as_str(), cong).to_socket_addrs().ok().and_then(|mut x| x.next()).expect("không phân giải được host");
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).unwrap_or_else(|e| {
        log(&format!("Không kết nối được: {e}"));
        std::process::exit(1)
    });
    tcp.set_nodelay(true).ok();
    let mut lines = BufReader::new(tcp.try_clone().unwrap()).lines();
    gui_json(&tcp, &json!({"loai": "vao", "ten": ten})).expect("gửi vào phòng");
    let chao: Value = lines.next().and_then(|l| l.ok()).and_then(|l| serde_json::from_str(&l).ok()).unwrap_or(Value::Null);
    let ma = chao["ma"].as_u64().unwrap_or(0) as u32;
    log(&format!("Vào phòng, mã {ma}; nguồn: {mo_ta}"));
    log(&format!("Gửi {MAU_MOI_KHUNG} mẫu/{KHUNG_MS} ms -> 50 gói/s, 256 kbit/s PCM"));

    // ---- Kênh dữ liệu UDP: connect() để chỉ nhận gói từ đúng địa chỉ server
    let udp = Arc::new(UdpSocket::bind("0.0.0.0:0").expect("udp"));
    udp.connect((host.as_str(), chao["cong_udp"].as_u64().unwrap_or(0) as u16)).expect("udp connect");

    let cs = Arc::new(Mutex::new(ChiaSe { mic_mo: false, am_luong: 1.0, dung: false, bo_dem: BoDemJitter::new(dem), ban_ghi: Vec::new(), moc: Vec::new() }));
    let t0 = Instant::now();
    let danh_moc = |cs: &Mutex<ChiaSe>, text: &str| {
        let mut c = cs.lock().unwrap();
        let t = (c.ban_ghi.len() as f64 / TAN_SO_MAU as f64 * 1000.0).round_ties_even() / 1000.0;
        c.moc.push(json!({"t": t, "su_kien": text}));
    };

    // luồng đọc kênh điều khiển
    {
        let (cs, tag) = (Arc::clone(&cs), tag.clone());
        thread::spawn(move || {
            for line in lines.map_while(Result::ok) {
                let Ok(tin) = serde_json::from_str::<Value>(&line) else { break };
                match tin["loai"].as_str() {
                    Some("mic") => {
                        cs.lock().unwrap().mic_mo = tin["mo"].as_bool().unwrap_or(false);
                        println!("{tag} Mic: {}", tin["ly_do"].as_str().unwrap_or(""));
                    }
                    Some("su_kien") => {
                        let nd = tin["noi_dung"].as_str().unwrap_or("");
                        println!("{tag} Sự kiện: {nd}");
                        danh_moc(&cs, nd);
                    }
                    Some("tu_choi") => println!("{tag} Bị từ chối: {}", tin["ly_do"].as_str().unwrap_or("")),
                    _ => {}
                }
            }
            cs.lock().unwrap().dung = true; // server đóng kết nối -> client dừng
        });
    }
    // luồng nhận âm thanh trộn từ server
    {
        let (cs, udp) = (Arc::clone(&cs), Arc::clone(&udp));
        thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while let Ok(n) = udp.recv(&mut buf) {
                if let Some(r) = tach_rtp(&buf[..n]).filter(|r| r.pt == PT_AM_THANH) {
                    cs.lock().unwrap().bo_dem.nhan_goi(r.seq, r.ts, r.payload, t0.elapsed().as_secs_f64());
                }
            }
        });
    }
    // luồng đọc lệnh bàn phím
    {
        let (cs, tag, ten, tcp) = (Arc::clone(&cs), tag.clone(), ten.clone(), tcp.try_clone().unwrap());
        thread::spawn(move || {
            for dong in io::stdin().lock().lines().map_while(Result::ok) {
                let mut dong = dong.split('#').next().unwrap_or("").trim().to_string();
                if let Some(r) = dong.strip_prefix('@') {
                    let (moc, lenh) = r.split_once(' ').unwrap_or((r, ""));
                    let cho = moc.parse::<f64>().unwrap_or(0.0) - t0.elapsed().as_secs_f64();
                    if cho > 0.0 {
                        thread::sleep(Duration::from_secs_f64(cho));
                    }
                    dong = lenh.trim().to_string();
                }
                let w: Vec<&str> = dong.split_whitespace().collect();
                match w.as_slice() {
                    [] => {}
                    ["tat_mic"] => {
                        // tự tắt: có hiệu lực ngay, không cần server đồng ý
                        cs.lock().unwrap().mic_mo = false;
                        println!("{tag} > tat_mic");
                        let _ = gui_json(&tcp, &json!({"loai": "tu_tat_mic"}));
                    }
                    ["bat_mic"] => {
                        // tự bật: phải chờ server cho phép
                        println!("{tag} > bat_mic (xin server)");
                        let _ = gui_json(&tcp, &json!({"loai": "tu_bat_mic"}));
                    }
                    ["am_luong", g] if g.parse::<f64>().is_ok() => {
                        let g: f64 = g.parse().unwrap();
                        cs.lock().unwrap().am_luong = g;
                        println!("{tag} > am_luong {g:.2} (chỉ áp dụng tại client)");
                        danh_moc(&cs, &format!("{ten} tự đặt âm lượng nghe {g:.2}"));
                        let _ = gui_json(&tcp, &json!({"loai": "am_luong_nghe", "gia_tri": g}));
                    }
                    ["thoat"] => cs.lock().unwrap().dung = true,
                    _ => println!("{tag} lệnh không hợp lệ: {dong}"),
                }
            }
        });
    }

    // ---- Vòng lặp 20 ms: thu + gửi, nhận + phát
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ ma as u64); // giả lập mất gói tất định (tái lập được)
    let (mut seq, mut ts, mut da_gui, mut bo_gia_lap, mut tong_byte) = (0u32, 0u32, 0u64, 0u64, 0u64);
    let so_khung = (thoi_gian * 1000.0 / KHUNG_MS as f64) as u64;
    let giu = dong_goi_rtp(PT_GIU_KET_NOI, 0, 0, ma, &[]);
    let _ = udp.send(&giu); // đăng ký địa chỉ UDP với server ngay từ đầu
    let mut cua_so: Vec<i16> = Vec::new(); // mẫu của 1 giây gần nhất (để đo mức)
    let mut ke_tiep = Instant::now();
    for n in 1..=so_khung {
        let (mic_mo, am_luong) = {
            let c = cs.lock().unwrap();
            if c.dung {
                break;
            }
            (c.mic_mo, c.am_luong)
        };
        // 1. Thu và gửi
        let khung = nguon.lay_khung();
        if mic_mo {
            if rng.random() < mat_goi {
                bo_gia_lap += 1; // "mạng" làm rơi gói: số thứ tự vẫn tăng
            } else {
                let goi = dong_goi_rtp(PT_AM_THANH, seq, ts, ma, &pcm_sang_byte(&khung));
                let _ = udp.send(&goi);
                da_gui += 1;
                tong_byte += goi.len() as u64 + 28; // + header UDP (8) và IPv4 (20)
            }
            seq = seq.wrapping_add(1);
        } else if n % 50 == 0 {
            let _ = udp.send(&giu); // mic tắt: mỗi giây một gói giữ kết nối
        }
        ts = ts.wrapping_add(MAU_MOI_KHUNG as u32); // nhãn thời gian tăng theo thời gian thực, kể cả khi tắt mic
        // 2. Nhận và phát
        let mut c = cs.lock().unwrap();
        let p = c.bo_dem.lay();
        let mut mau = p.map(|p| byte_sang_pcm(&p)).unwrap_or_else(|| vec![0; MAU_MOI_KHUNG]);
        if am_luong != 1.0 {
            mau = mau.iter().map(|&x| cat_ngon(x as f64 * am_luong)).collect();
        }
        c.ban_ghi.extend_from_slice(&mau);
        drop(c);
        cua_so.extend_from_slice(&mau);
        if n % 50 == 0 {
            let db = muc_dbfs(&cua_so);
            log(&format!(
                "{:4.1}s nghe {} {} dBFS mic {}",
                (n * KHUNG_MS) as f64 / 1000.0,
                thanh_muc(db, 16),
                chu_db(db, 5),
                if mic_mo { "mở" } else { "tắt" }
            ));
            cua_so.clear();
        }
        ke_tiep += Duration::from_millis(KHUNG_MS);
        thread::sleep(ke_tiep.saturating_duration_since(Instant::now()));
    }

    // ---- Kết thúc: thống kê, lưu bản ghi
    let giay = t0.elapsed().as_secs_f64();
    let _ = tcp.shutdown(std::net::Shutdown::Both);
    log(&format!("Kết thúc sau {giay:.1} s"));
    log(&format!("Gửi {da_gui} gói ({:.0} kbit/s gồm header), giả lập mất {bo_gia_lap}", tong_byte as f64 * 8.0 / giay / 1000.0));
    let c = cs.lock().unwrap();
    let b = &c.bo_dem;
    log(&format!("Nhận {} gói, mất {}, trễ {}, jitter {:.2} ms", b.nhan, b.mat(), b.tre, b.jitter_ms()));
    log(&format!("Phát: thiếu {} khung, cạn {} lần, đệm {dem} khung = {} ms", b.thieu, b.can, dem as u64 * KHUNG_MS));
    if let Some(ghi) = ghi {
        if let Some(d) = Path::new(&ghi).parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(d);
        }
        ghi_wav(&ghi, &c.ban_ghi).expect("ghi WAV");
        let duong_moc = Path::new(&ghi).with_extension("").to_string_lossy().into_owned() + "_moc.json";
        // JSON thụt lề 1 dấu cách như json.dump(indent=1) của bản Python
        let mut out = Vec::new();
        let fmt = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut ser = serde_json::Serializer::with_formatter(&mut out, fmt);
        serde::Serialize::serialize(&json!({"ten": ten, "moc": c.moc}), &mut ser).unwrap();
        std::fs::write(&duong_moc, out).expect("ghi tệp mốc");
        log(&format!("Đã ghi {ghi} ({:.2} s), {} mốc sự kiện", c.ban_ghi.len() as f64 / TAN_SO_MAU as f64, c.moc.len()));
    }
}
