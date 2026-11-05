//! Server phòng hội thoại video nhiều client (ghép lưới kiểu MCU).
//!
//! Hai kênh giống MR6.2:
//!   - TCP: thông điệp JSON điều khiển (vào phòng, bật/tắt camera, tham số gửi/nhận, sự kiện);
//!   - UDP: khung hình nén zlib, chia mảnh ≤ 1200 byte mỗi datagram.
//! Kiến trúc MCU (Multipoint Control Unit): server giải nén khung của mọi client, ghép thành một
//! lưới (mỗi ô 160×120), nén lại và gửi cho từng người nhận theo tốc độ khung/độ phân giải mà
//! người đó yêu cầu.
//!
//! Quy tắc quyền điều khiển (giống mic ở MR6.2):
//!   1. Camera của X mở khi và chỉ khi X không tự tắt VÀ server không tắt camera của X.
//!   2. Server tắt thắng client: đang bị server tắt thì client xin bật bị từ chối.
//!   3. Server bật = gỡ lệnh tắt của server, KHÔNG bật hộ camera mà người dùng tự tắt.
//!   4. Giới hạn gửi của từng client (fps tối đa, độ phân giải 1/1, 1/2, 1/4) và fps chung do server đặt.
//!   5. Tham số nhận (fps, độ phân giải lưới) do từng client tự chọn; fps nhận không vượt fps chung.
//!   6. Server tự thực thi: bỏ khung của camera đang tắt, bỏ khung đến nhanh hơn fps cho phép,
//!      bỏ datagram sai địa chỉ nguồn.
//!
//! Lệnh bảng điều khiển (stdin; "@t lệnh" = chờ tới giây thứ t):
//!   tat_camera <tên|tat_ca>   bat_camera <tên|tat_ca>   fps_toi_da <tên|tat_ca> <n>
//!   do_phan_giai <tên|tat_ca> <1|2|4>   fps_chung <n>   trang_thai   thoat

use crate::chung::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::Duration;

const FPS_MAC_DINH: u32 = 10;

fn log(text: &str) {
    println!("[server] {text}");
}

struct Khach {
    ma: u16,
    ten: String,
    conn: TcpStream,
    ip: IpAddr,
    udp: Option<SocketAddr>,
    tu_tat: bool,     // client tự tắt camera
    server_tat: bool, // server tắt camera
    fps_toi_da: u32,  // giới hạn gửi do server đặt
    chia: usize,      // độ phân giải gửi = 1/chia mỗi chiều
    nhan_fps: u32,    // tham số nhận do client chọn
    nhan_chia: usize,
    ghep: GhepKhung,
    o: Option<Vec<u8>>, // ô 160×120 đã phóng to, có nhãn – dùng khi ghép lưới
    luc_nhan_cuoi: f64,
    bo_vuot_fps: u64,
    bo_tat: u64,
    sai_nguon: u64,
    dem_khung: u64, // cửa sổ 1 giây để đo fps, kbit/s vào
    dem_byte: u64,
    fps_vao: f64,
    kbps_vao: f64,
    so_khung_ra: u32, // số khung lưới đã gửi cho client này
    han_gui_ra: f64,
    dem_ra: u64,
    byte_ra: u64,
    fps_ra: f64,
    kbps_ra: f64,
}

impl Khach {
    fn cam_mo(&self) -> bool {
        !self.tu_tat && !self.server_tat // quy tắc 1
    }

    fn ly_do(&self) -> &'static str {
        match (self.cam_mo(), self.tu_tat, self.server_tat) {
            (true, _, _) => "mở",
            (_, true, true) => "tắt (cả hai)",
            (_, _, true) => "tắt (server)",
            _ => "tắt (tự tắt)",
        }
    }

    fn gui(&self, obj: Value) {
        let _ = gui_json(&self.conn, &obj);
    }

    /// Báo cho client tham số gửi hiện hành.
    fn gui_camera(&mut self) {
        self.gui(json!({"loai": "camera", "mo": self.cam_mo(), "ly_do": self.ly_do(), "fps": self.fps_toi_da, "chia": self.chia}));
        self.ve_o(None);
    }

    fn gui_nhan(&self, fps_chung: u32) {
        self.gui(json!({"loai": "nhan", "fps": self.nhan_fps.min(fps_chung), "chia": self.nhan_chia}));
    }

    /// Vẽ ô của client trong lưới: hình đã phóng về 160×120 + dải nhãn trạng thái.
    fn ve_o(&mut self, khung: Option<(usize, usize, Vec<u8>)>) {
        let mut o = match (khung, self.cam_mo(), self.o.take()) {
            (Some((w, h, anh)), _, _) => phong_to(&anh, w, h, RONG / w),
            (None, true, Some(o)) => o,
            _ => {
                let mut o: Vec<u8> = [40u8, 40, 40].iter().copied().cycle().take(RONG * CAO * 3).collect();
                if !self.cam_mo() {
                    ve_chu(&mut o, RONG, CAO, 20, 44, "TAT CAMERA", 2, [255, 255, 255]);
                    ve_chu(&mut o, RONG, CAO, 44, 66, if self.server_tat { "(SERVER)" } else { "(TU TAT)" }, 2, [255, 255, 255]);
                }
                o
            }
        };
        to_mau(&mut o, RONG, 0, CAO - 11, RONG, 11, [0, 0, 0]);
        let nhan = format!("{} {}FPS{}", self.ten, self.fps_toi_da, if self.chia > 1 { format!(" 1/{}", self.chia) } else { String::new() });
        ve_chu(&mut o, RONG, CAO, 2, CAO - 9, &nhan, 1, [255, 255, 0]);
        self.o = Some(o);
    }
}

struct Phong {
    khach: BTreeMap<u16, Khach>, // theo mã = thứ tự vào phòng
    da_roi: Vec<Khach>,
    ma_moi: u16,
    fps_chung: u32, // giới hạn fps đầu ra chung cho mọi người nhận
}

static PHONG: LazyLock<Mutex<Phong>> = LazyLock::new(|| Mutex::new(Phong { khach: BTreeMap::new(), da_roi: Vec::new(), ma_moi: 0, fps_chung: FPS_MAC_DINH }));

fn phat_su_kien(p: &Phong, text: &str) {
    log(&format!("  => {text}"));
    for k in p.khach.values() {
        k.gui(json!({"loai": "su_kien", "noi_dung": text}));
    }
}

// ---------------------------------------------------------------- Kênh điều khiển TCP
fn phuc_vu(conn: TcpStream, cong_udp: u16) {
    let addr = conn.peer_addr().unwrap();
    let mut lines = BufReader::new(conn.try_clone().unwrap()).lines();
    let tin: Value = lines.next().and_then(|l| l.ok()).and_then(|l| serde_json::from_str(&l).ok()).unwrap_or(Value::Null);
    let ten: String = match (tin["loai"].as_str(), tin["ten"].as_str()) {
        (Some("vao"), Some(t)) if !t.is_empty() => t.chars().take(8).collect(),
        _ => return,
    };
    let ma = {
        let mut p = PHONG.lock().unwrap();
        p.ma_moi += 1;
        let ma = p.ma_moi;
        let k = Khach {
            ma,
            ten: ten.clone(),
            conn,
            ip: addr.ip(),
            udp: None,
            tu_tat: false,
            server_tat: false,
            fps_toi_da: FPS_MAC_DINH,
            chia: 1,
            nhan_fps: FPS_MAC_DINH,
            nhan_chia: 1,
            ghep: GhepKhung::new(),
            o: None,
            luc_nhan_cuoi: 0.0,
            bo_vuot_fps: 0,
            bo_tat: 0,
            sai_nguon: 0,
            dem_khung: 0,
            dem_byte: 0,
            fps_vao: 0.0,
            kbps_vao: 0.0,
            so_khung_ra: 0,
            han_gui_ra: 0.0,
            dem_ra: 0,
            byte_ra: 0,
            fps_ra: 0.0,
            kbps_ra: 0.0,
        };
        k.gui(json!({"loai": "chao", "ma": ma, "cong_udp": cong_udp, "rong": RONG, "cao": CAO}));
        log(&format!("#{ma} {ten} vào phòng từ {addr}"));
        let fc = p.fps_chung;
        p.khach.insert(ma, k);
        let k = p.khach.get_mut(&ma).unwrap();
        k.gui_camera();
        k.gui_nhan(fc);
        phat_su_kien(&p, &format!("{ten} vào phòng"));
        ma
    };
    for line in lines.map_while(Result::ok) {
        let Ok(tin) = serde_json::from_str::<Value>(&line) else { break };
        xu_ly_tin_client(&mut PHONG.lock().unwrap(), ma, &tin);
    }
    let mut p = PHONG.lock().unwrap();
    if let Some(k) = p.khach.remove(&ma) {
        log(&format!("#{ma} {} rời phòng", k.ten));
        p.da_roi.push(k);
        phat_su_kien(&p, &format!("{ten} rời phòng"));
    }
}

fn xu_ly_tin_client(p: &mut Phong, ma: u16, tin: &Value) {
    let fc = p.fps_chung;
    let Some(k) = p.khach.get_mut(&ma) else { return };
    let ten = k.ten.clone();
    let su_kien = match tin["loai"].as_str() {
        Some("tu_tat_camera") => {
            k.tu_tat = true;
            k.gui_camera();
            Some(format!("{ten} tự tắt camera"))
        }
        Some("tu_bat_camera") => {
            k.tu_tat = false;
            if k.server_tat {
                // quy tắc 2
                log(&format!("{ten} xin bật camera -> từ chối (server đang tắt)"));
                k.gui(json!({"loai": "tu_choi", "ly_do": "server đang tắt camera của bạn"}));
                None
            } else {
                k.gui_camera();
                Some(format!("{ten} tự bật camera"))
            }
        }
        Some("nhan") => {
            // quy tắc 5
            k.nhan_fps = tin["fps"].as_i64().map_or(k.nhan_fps, |f| f.clamp(1, 30) as u32);
            k.nhan_chia = match tin["chia"].as_i64() {
                Some(c) => if c == 2 { 2 } else { 1 },
                None => k.nhan_chia,
            };
            log(&format!("{ten} chọn nhận {} fps, độ phân giải 1/{}", k.nhan_fps, k.nhan_chia));
            k.gui_nhan(fc);
            None
        }
        _ => None,
    };
    if let Some(s) = su_kien {
        phat_su_kien(p, &s);
    }
}

// ---------------------------------------------------------------- Kênh dữ liệu UDP
fn nhan_udp(sock: Arc<UdpSocket>) {
    let mut buf = [0u8; 2048];
    loop {
        let Ok((n, addr)) = sock.recv_from(&mut buf) else { continue };
        let Some(m) = tach_header(&buf[..n]) else { continue };
        let now = bay_gio();
        let mut p = PHONG.lock().unwrap();
        let Some(k) = p.khach.get_mut(&m.ma) else { continue };
        if k.udp.is_none() && addr.ip() == k.ip {
            k.udp = Some(addr);
            log(&format!("#{} {} đăng ký UDP {addr}", k.ma, k.ten));
        }
        if Some(addr) != k.udp {
            k.sai_nguon += 1; // quy tắc 6
            continue;
        }
        if m.loai != LOAI_MANH {
            continue;
        }
        let Some((w, h, du_lieu)) = k.ghep.them(&m, now) else { continue };
        if !k.cam_mo() {
            k.bo_tat += 1; // quy tắc 6: camera tắt
            continue;
        }
        if now - k.luc_nhan_cuoi < 0.8 / k.fps_toi_da as f64 {
            k.bo_vuot_fps += 1; // quy tắc 6: vượt fps (dung sai 20%)
            continue;
        }
        let Some(anh) = giai_nen(&du_lieu) else { continue };
        let (w, h) = (w as usize, h as usize);
        if w == 0 || h == 0 || w * h * 3 != anh.len() || RONG % w != 0 || w * CAO != h * RONG {
            continue; // kích thước không hợp lệ
        }
        k.luc_nhan_cuoi = now;
        k.dem_khung += 1;
        k.dem_byte += du_lieu.len() as u64;
        k.ve_o(Some((w, h, anh)));
    }
}

// ---------------------------------------------------------------- Ghép lưới và gửi
/// Ghép ô của mọi client thành lưới c cột × r hàng (c = ⌈√n⌉).
fn ghep_luoi(ds: &[&Vec<u8>]) -> (Vec<u8>, usize, usize) {
    let n = ds.len().max(1);
    let cot = (n as f64).sqrt().ceil() as usize;
    let hang = n.div_ceil(cot);
    let trong: Vec<u8> = [15u8, 15, 15].iter().copied().cycle().take(RONG * 3).collect();
    let mut ra = Vec::with_capacity(cot * RONG * hang * CAO * 3);
    for r in 0..hang {
        for y in 0..CAO {
            for c in 0..cot {
                match ds.get(r * cot + c) {
                    Some(o) => ra.extend_from_slice(&o[y * RONG * 3..(y + 1) * RONG * 3]),
                    None => ra.extend_from_slice(&trong),
                }
            }
        }
    }
    (ra, cot * RONG, hang * CAO)
}

/// Mỗi 5 ms kiểm tra: người nhận nào tới hạn (theo fps nhận) thì ghép lưới, thu nhỏ theo độ phân
/// giải nhận, nén, chia mảnh và gửi. Mỗi giây tính lại fps, kbit/s.
fn vong_gui(sock: Arc<UdpSocket>) {
    let mut moc_do = bay_gio();
    loop {
        let now = bay_gio();
        {
            let mut p = PHONG.lock().unwrap();
            let fc = p.fps_chung;
            let mut viec = Vec::new(); // (mã người nhận, độ phân giải nhận)
            for k in p.khach.values() {
                if k.udp.is_none() || now < k.han_gui_ra {
                    continue;
                }
                viec.push((k.ma, k.nhan_chia));
            }
            let co_o = p.khach.values().any(|k| k.o.is_some());
            if co_o {
                for (ma, nhan_chia) in viec {
                    let (anh, w, h) = {
                        let ds: Vec<&Vec<u8>> = p.khach.values().filter_map(|k| k.o.as_ref()).collect();
                        ghep_luoi(&ds)
                    };
                    let (anh, w, h) = thu_nho(&anh, w, h, nhan_chia);
                    let goi_nen = nen(&anh, MUC_NHANH);
                    let k = p.khach.get_mut(&ma).unwrap();
                    let fps = k.nhan_fps.min(fc) as f64;
                    k.han_gui_ra = (k.han_gui_ra + 1.0 / fps).max(now);
                    for g in chia_manh(MA_SERVER, k.so_khung_ra, w, h, &goi_nen) {
                        let _ = sock.send_to(&g, k.udp.unwrap());
                    }
                    k.so_khung_ra = k.so_khung_ra.wrapping_add(1);
                    k.dem_ra += 1;
                    k.byte_ra += goi_nen.len() as u64;
                }
            }
            if now - moc_do >= 1.0 {
                for k in p.khach.values_mut() {
                    k.fps_vao = k.dem_khung as f64;
                    k.kbps_vao = k.dem_byte as f64 * 8.0 / 1000.0;
                    k.fps_ra = k.dem_ra as f64;
                    k.kbps_ra = k.byte_ra as f64 * 8.0 / 1000.0;
                    (k.dem_khung, k.dem_byte, k.dem_ra, k.byte_ra) = (0, 0, 0, 0);
                }
                moc_do += 1.0;
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}

// ---------------------------------------------------------------- Bảng điều khiển
fn chon(p: &Phong, ten: &str) -> Vec<u16> {
    p.khach.values().filter(|k| ten == "tat_ca" || k.ten == ten).map(|k| k.ma).collect()
}

fn lenh_server(dong: &str) -> bool {
    let w: Vec<&str> = dong.split_whitespace().collect();
    if w.is_empty() {
        return true;
    }
    log(&format!("t={:.1}s > {dong}", bay_gio()));
    let mut p = PHONG.lock().unwrap();
    let ai = match w.get(1) {
        Some(&"tat_ca") => "tất cả".to_string(),
        Some(x) => x.to_string(),
        None => String::new(),
    };
    let fc = p.fps_chung;
    match w.as_slice() {
        [lenh @ ("tat_camera" | "bat_camera"), ten] => {
            let ds = chon(&p, ten);
            for ma in &ds {
                let k = p.khach.get_mut(ma).unwrap();
                k.server_tat = *lenh == "tat_camera";
                k.gui_camera();
            }
            if *lenh == "tat_camera" {
                phat_su_kien(&p, &format!("server tắt camera {ai}"));
            } else {
                // quy tắc 3
                let van_tat: Vec<String> = ds.iter().filter_map(|m| p.khach.get(m)).filter(|k| k.tu_tat).map(|k| k.ten.clone()).collect();
                let them = if van_tat.is_empty() { String::new() } else { format!(" ({} vẫn tự tắt)", van_tat.join(", ")) };
                phat_su_kien(&p, &format!("server bật camera {ai}{them}"));
            }
        }
        ["fps_toi_da", ten, n] if n.parse::<i64>().is_ok() => {
            for ma in chon(&p, ten) {
                let k = p.khach.get_mut(&ma).unwrap();
                k.fps_toi_da = n.parse::<i64>().unwrap().clamp(1, 30) as u32;
                k.gui_camera();
            }
            phat_su_kien(&p, &format!("server giới hạn {ai} gửi tối đa {n} fps"));
        }
        ["do_phan_giai", ten, d @ ("1" | "2" | "4")] => {
            for ma in chon(&p, ten) {
                let k = p.khach.get_mut(&ma).unwrap();
                k.chia = d.parse().unwrap();
                k.gui_camera();
            }
            phat_su_kien(&p, &format!("server đặt độ phân giải {ai} = 1/{d}"));
        }
        ["fps_chung", n] if n.parse::<i64>().is_ok() => {
            p.fps_chung = n.parse::<i64>().unwrap().clamp(1, 30) as u32;
            for k in p.khach.values() {
                k.gui_nhan(p.fps_chung);
            }
            phat_su_kien(&p, &format!("server đặt fps chung tối đa {}", p.fps_chung));
        }
        ["trang_thai"] => in_trang_thai(&p, fc),
        ["thoat"] => return false,
        _ => log("lệnh không hợp lệ"),
    }
    true
}

fn in_trang_thai(p: &Phong, fc: u32) {
    log(&format!("Trạng thái: {} client, fps chung {fc}", p.khach.len()));
    log("                     giới hạn vào        ra (lưới)");
    log(" #  tên camera       fps 1/n  fps  kbit  fps  kbit");
    for k in p.khach.values() {
        log(&format!(
            "{:2}  {:<3} {:<12} {:3} {:3} {:4.0} {:5.0} {:4.0} {:5.0}",
            k.ma,
            k.ten,
            k.ly_do(),
            k.fps_toi_da,
            k.chia,
            k.fps_vao,
            k.kbps_vao,
            k.fps_ra,
            k.kbps_ra
        ));
    }
}

fn in_tong_ket(p: &Phong) {
    log("Tổng kết luồng vào (từ client tới server):");
    log(" #  tên mảnh khung hỏng mảnh mất  bỏ:tắt vượt fps");
    log("        nhận    đủ        mất hẳn");
    let mut ds: Vec<&Khach> = p.khach.values().chain(p.da_roi.iter()).collect();
    ds.sort_by_key(|k| k.ma);
    for k in ds {
        let g = &k.ghep;
        log(&format!(
            "{:2}  {:<3} {:4} {:5} {:4} {:4} {:4} {:6} {:6}",
            k.ma, k.ten, g.manh_nhan, g.khung_du, g.khung_hong, g.manh_mat, g.khung_mat_han, k.bo_tat, k.bo_vuot_fps
        ));
    }
}

fn dung() -> ! {
    log("Dừng server (Ctrl+C)");
    in_tong_ket(&PHONG.lock().unwrap());
    std::process::exit(0)
}

pub fn run(args: &[String]) {
    LazyLock::force(&T0);
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong_tcp: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(41630);
    let cong_udp: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(41631);
    let fail = |e: io::Error| -> ! {
        log(&format!("lỗi: {e}"));
        std::process::exit(1)
    };
    let srv = TcpListener::bind((host.as_str(), cong_tcp)).unwrap_or_else(|e| fail(e));
    let dia_chi = (host.as_str(), cong_udp).to_socket_addrs().unwrap_or_else(|e| fail(e)).next().unwrap();
    let udp = Arc::new(udp_dem_lon(dia_chi).unwrap_or_else(|e| fail(e))); // đệm nhận lớn: mảnh đến dồn dập
    ctrlc::set_handler(|| dung()).expect("Không cài được hàm bắt Ctrl+C");
    log(&format!("TCP {host}:{cong_tcp} điều khiển; UDP :{cong_udp} video"));
    log(&format!("Khung {RONG}x{CAO} RGB {} B, nén zlib, mảnh ≤ 1200 B", RONG * CAO * 3));
    thread::spawn(move || {
        for conn in srv.incoming().flatten() {
            conn.set_nodelay(true).ok();
            thread::spawn(move || phuc_vu(conn, cong_udp));
        }
    });
    let u1 = Arc::clone(&udp);
    thread::spawn(move || nhan_udp(u1));
    thread::spawn(move || vong_gui(udp));
    for dong in io::stdin().lock().lines().map_while(Result::ok) {
        let mut dong = dong.split('#').next().unwrap_or("").trim().to_string();
        if let Some(r) = dong.strip_prefix('@') {
            let (moc, lenh) = r.split_once(' ').unwrap_or((r, ""));
            let cho = moc.parse::<f64>().unwrap_or(0.0) - bay_gio();
            if cho > 0.0 {
                thread::sleep(Duration::from_secs_f64(cho));
            }
            dong = lenh.trim().to_string();
        }
        if !dong.is_empty() && !lenh_server(&dong) {
            dung();
        }
    }
    loop {
        thread::park();
    }
}
