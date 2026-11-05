//! Server phòng hội thoại âm thanh nhiều client (trộn tiếng "mix-minus").
//!
//! Hai kênh:
//!   - TCP (kênh điều khiển, tin cậy): mỗi dòng một thông điệp JSON – vào phòng, tự tắt/bật mic,
//!     báo âm lượng nghe; server gửi trạng thái mic, sự kiện, lời từ chối.
//!   - UDP (kênh dữ liệu, thời gian thực): gói RTP chứa khung PCM 20 ms. Mất một gói chỉ mất
//!     20 ms âm thanh; truyền lại như TCP sẽ đến quá muộn, vô ích với âm thanh trực tiếp.
//! Bộ trộn chạy mỗi 20 ms: lấy một khung từ bộ đệm jitter của từng client, nhân độ lợi đầu vào,
//! cộng tất cả (tổng T); mỗi người nghe k nhận T - (tiếng của chính k) – "mix-minus" – nhân âm
//! lượng chung, cắt ngọn về phạm vi 16 bit rồi gửi đi.
//!
//! Quy tắc quyền điều khiển:
//!   1. Mic của client X mở khi và chỉ khi X không tự tắt VÀ server không khóa mic của X.
//!   2. Server tắt (khóa) thắng client: khi đang bị khóa, client xin bật mic bị từ chối.
//!   3. Server bật = gỡ khóa, KHÔNG bật hộ mic mà người dùng đã tự tắt.
//!   4. Độ lợi đầu vào từng client và âm lượng chung do server đặt, áp dụng khi trộn.
//!   5. Âm lượng nghe do từng client tự đặt và áp dụng tại client (client chỉ báo để server hiển thị).
//!   6. Server tự thực thi, không tin client: gói âm thanh từ client đang tắt mic bị bỏ, gói UDP
//!      không khớp địa chỉ đã đăng ký bị bỏ.
//! Mọi trạng thái dùng chung nằm trong MỘT Mutex (PHONG); bốn luồng: nhận kết nối, mỗi client một
//! luồng điều khiển, nhận UDP, trộn 20 ms; luồng chính đọc lệnh bảng điều khiển từ stdin.
//!
//! Lệnh bảng điều khiển (stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ khi server chạy):
//!   tat_mic <tên|tat_ca>    bat_mic <tên|tat_ca>    do_loi <tên> <hệ số>
//!   am_luong_chung <hệ số>  trang_thai               thoat

use crate::chung::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

static T0: LazyLock<Instant> = LazyLock::new(Instant::now);

fn log(text: &str) {
    println!("[server] {text}");
}

struct Khach {
    ma: u32,
    ten: String,
    conn: TcpStream, // để gửi tin điều khiển
    ip: IpAddr,
    udp: Option<SocketAddr>, // địa chỉ UDP học được từ gói đầu tiên
    tu_tat: bool,            // client tự tắt mic
    server_khoa: bool,       // server khóa mic
    do_loi: f64,             // độ lợi đầu vào (gain)
    am_luong_nghe: f64,      // client báo (chỉ để hiển thị)
    bo_dem: BoDemJitter,
    seq_ra: u32, // số thứ tự, nhãn thời gian của luồng trộn gửi cho client
    ts_ra: u32,
    muc_vao: f64,
    bi_chan: u64,   // gói bị bỏ vì mic đang tắt
    sai_nguon: u64, // gói bị bỏ vì sai địa chỉ nguồn
    cat_ngon: u64,  // số mẫu bị cắt ngọn trong luồng gửi cho client này
}

impl Khach {
    fn mic_mo(&self) -> bool {
        !self.tu_tat && !self.server_khoa // quy tắc 1
    }

    fn ly_do(&self) -> &'static str {
        match (self.mic_mo(), self.tu_tat, self.server_khoa) {
            (true, _, _) => "mở",
            (_, true, true) => "tắt (cả hai)",
            (_, _, true) => "tắt (server)",
            _ => "tắt (tự tắt)",
        }
    }

    fn gui(&self, obj: Value) {
        let _ = gui_json(&self.conn, &obj); // client đã mất: luồng điều khiển của nó sẽ dọn
    }

    fn gui_mic(&mut self) {
        self.gui(json!({"loai": "mic", "mo": self.mic_mo(), "ly_do": self.ly_do()}));
        if !self.mic_mo() {
            self.bo_dem.xoa(); // bỏ phần âm thanh còn trong bộ đệm
        }
    }
}

#[derive(Default)]
struct Phong {
    khach: BTreeMap<u32, Khach>, // mã client (SSRC) -> Khach, theo thứ tự vào phòng
    da_roi: Vec<Khach>,          // client đã rời phòng (giữ lại để in tổng kết)
    ma_moi: u32,
    am_luong_chung: f64,
}

static PHONG: LazyLock<Mutex<Phong>> = LazyLock::new(|| Mutex::new(Phong { am_luong_chung: 1.0, ..Default::default() }));

/// Báo sự kiện cho mọi client (họ hiển thị và đánh mốc trong bản ghi).
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
            server_khoa: false,
            do_loi: 1.0,
            am_luong_nghe: 1.0,
            bo_dem: BoDemJitter::new(3),
            seq_ra: 0,
            ts_ra: 0,
            muc_vao: f64::NEG_INFINITY,
            bi_chan: 0,
            sai_nguon: 0,
            cat_ngon: 0,
        };
        k.gui(json!({"loai": "chao", "ma": ma, "cong_udp": cong_udp, "tan_so_mau": 16000, "khung_ms": KHUNG_MS}));
        log(&format!("#{ma} {ten} vào phòng từ {addr}"));
        p.khach.insert(ma, k);
        p.khach.get_mut(&ma).unwrap().gui_mic();
        phat_su_kien(&p, &format!("{ten} vào phòng"));
        ma
    };
    for line in lines.map_while(Result::ok) {
        let Ok(tin) = serde_json::from_str::<Value>(&line) else { break };
        let mut p = PHONG.lock().unwrap();
        xu_ly_tin_client(&mut p, ma, &tin);
    }
    let mut p = PHONG.lock().unwrap();
    if let Some(k) = p.khach.remove(&ma) {
        log(&format!("#{ma} {} rời phòng", k.ten));
        p.da_roi.push(k);
        phat_su_kien(&p, &format!("{ten} rời phòng"));
    }
}

fn xu_ly_tin_client(p: &mut Phong, ma: u32, tin: &Value) {
    let Some(k) = p.khach.get_mut(&ma) else { return };
    let ten = k.ten.clone();
    // sửa trạng thái của client trước, phát sự kiện cho cả phòng sau (khi đã trả lại quyền mượn k)
    let su_kien = match tin["loai"].as_str() {
        Some("tu_tat_mic") => {
            k.tu_tat = true;
            k.gui_mic();
            Some(format!("{ten} tự tắt mic"))
        }
        Some("tu_bat_mic") => {
            k.tu_tat = false;
            if k.server_khoa {
                // quy tắc 2
                log(&format!("{ten} xin bật mic -> từ chối (server đang khóa)"));
                k.gui(json!({"loai": "tu_choi", "ly_do": "server đang khóa mic của bạn"}));
                None
            } else {
                k.gui_mic();
                Some(format!("{ten} tự bật mic"))
            }
        }
        Some("am_luong_nghe") => {
            // quy tắc 5: chỉ ghi nhận
            k.am_luong_nghe = tin["gia_tri"].as_f64().unwrap_or(1.0);
            log(&format!("{ten} tự đặt âm lượng nghe {:.2}", k.am_luong_nghe));
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
        let Some(r) = tach_rtp(&buf[..n]) else { continue };
        let mut p = PHONG.lock().unwrap();
        let Some(k) = p.khach.get_mut(&r.ssrc) else { continue };
        if k.udp.is_none() && addr.ip() == k.ip {
            // đăng ký địa chỉ UDP lần đầu
            k.udp = Some(addr);
            log(&format!("#{} {} đăng ký UDP {addr}", k.ma, k.ten));
        }
        if Some(addr) != k.udp {
            k.sai_nguon += 1; // quy tắc 6: sai nguồn
            continue;
        }
        if r.pt != PT_AM_THANH || r.payload.len() != BYTE_MOI_KHUNG {
            continue; // gói giữ kết nối hoặc sai dạng
        }
        if !k.mic_mo() {
            k.bi_chan += 1; // quy tắc 6: mic tắt
            continue;
        }
        k.bo_dem.nhan_goi(r.seq, r.ts, r.payload, T0.elapsed().as_secs_f64());
    }
}

/// Mỗi 20 ms: trộn và gửi cho từng người nghe. Hẹn giờ tuyệt đối để không trôi nhịp.
fn vong_tron(sock: Arc<UdpSocket>) {
    let mut ke_tiep = Instant::now();
    loop {
        {
            let mut p = PHONG.lock().unwrap();
            let am_luong = p.am_luong_chung;
            // 1. Lấy một khung từ mỗi client đang mở mic, nhân độ lợi đầu vào, cộng vào tổng T
            let mut vao: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
            let mut tong = vec![0.0f64; MAU_MOI_KHUNG];
            for k in p.khach.values_mut() {
                let goi = if k.mic_mo() { k.bo_dem.lay() } else { None };
                let Some(goi) = goi else {
                    k.muc_vao = f64::NEG_INFINITY;
                    continue;
                };
                let mau = byte_sang_pcm(&goi);
                k.muc_vao = muc_dbfs(&mau);
                let v: Vec<f64> = mau.iter().map(|&x| k.do_loi * x as f64).collect();
                for (t, x) in tong.iter_mut().zip(&v) {
                    *t += x;
                }
                vao.insert(k.ma, v);
            }
            // 2. Mỗi người nghe: T - tiếng của chính mình, nhân âm lượng chung, cắt ngọn
            for k in p.khach.values_mut() {
                let Some(dich) = k.udp else { continue };
                let rieng = vao.get(&k.ma);
                let mut ra = vec![0i16; MAU_MOI_KHUNG];
                for i in 0..MAU_MOI_KHUNG {
                    let v = ((tong[i] - rieng.map_or(0.0, |r| r[i])) * am_luong).round_ties_even();
                    if !(-32768.0..=32767.0).contains(&v) {
                        k.cat_ngon += 1;
                    }
                    ra[i] = v.clamp(-32768.0, 32767.0) as i16;
                }
                let goi = dong_goi_rtp(PT_AM_THANH, k.seq_ra, k.ts_ra, SSRC_SERVER, &pcm_sang_byte(&ra));
                k.seq_ra = k.seq_ra.wrapping_add(1);
                k.ts_ra = k.ts_ra.wrapping_add(MAU_MOI_KHUNG as u32);
                let _ = sock.send_to(&goi, dich);
            }
        }
        ke_tiep += Duration::from_millis(KHUNG_MS);
        thread::sleep(ke_tiep.saturating_duration_since(Instant::now()));
    }
}

// ---------------------------------------------------------------- Bảng điều khiển
/// Mã các client theo tên, hoặc tất cả nếu ten = tat_ca.
fn chon(p: &Phong, ten: &str) -> Vec<u32> {
    p.khach.values().filter(|k| ten == "tat_ca" || k.ten == ten).map(|k| k.ma).collect()
}

fn lenh_server(dong: &str) -> bool {
    let w: Vec<&str> = dong.split_whitespace().collect();
    if w.is_empty() {
        return true;
    }
    log(&format!("t={:.1}s > {dong}", T0.elapsed().as_secs_f64()));
    let mut p = PHONG.lock().unwrap();
    match w.as_slice() {
        [lenh @ ("tat_mic" | "bat_mic"), ai] => {
            let ds = chon(&p, ai);
            if ds.is_empty() {
                log(&format!("không có client {ai}"));
                return true;
            }
            for ma in &ds {
                let k = p.khach.get_mut(ma).unwrap();
                k.server_khoa = *lenh == "tat_mic";
                k.gui_mic();
            }
            let ai_txt = if *ai == "tat_ca" { "tất cả".to_string() } else { ai.to_string() };
            if *lenh == "tat_mic" {
                phat_su_kien(&p, &format!("server tắt mic {ai_txt}"));
            } else {
                // quy tắc 3
                let van_tat: Vec<String> = ds.iter().filter_map(|m| p.khach.get(m)).filter(|k| k.tu_tat).map(|k| k.ten.clone()).collect();
                let them = if van_tat.is_empty() { String::new() } else { format!(" ({} vẫn tự tắt)", van_tat.join(", ")) };
                phat_su_kien(&p, &format!("server bật mic {ai_txt}{them}"));
            }
        }
        ["do_loi", ai, g] => {
            let Ok(g) = g.parse::<f64>() else {
                log("lệnh không hợp lệ");
                return true;
            };
            for ma in chon(&p, ai) {
                p.khach.get_mut(&ma).unwrap().do_loi = g;
            }
            phat_su_kien(&p, &format!("server đặt độ lợi {ai} = {g:.2}"));
        }
        ["am_luong_chung", g] => {
            let Ok(g) = g.parse::<f64>() else {
                log("lệnh không hợp lệ");
                return true;
            };
            p.am_luong_chung = g;
            phat_su_kien(&p, &format!("server đặt âm lượng chung {g:.2}"));
        }
        ["trang_thai"] => in_trang_thai(&p),
        ["thoat"] => return false,
        _ => log("lệnh không hợp lệ"),
    }
    true
}

fn in_trang_thai(p: &Phong) {
    log(&format!("Trạng thái: {} client, âm lượng chung {:.2}", p.khach.len(), p.am_luong_chung));
    log(" #  tên mic          lợi   mức vào            mất  jit.");
    for k in p.khach.values() {
        log(&format!(
            "{:2}  {:<3} {:<12} {:4.2} {} {} {:3} {:4.1}ms",
            k.ma,
            k.ten,
            k.ly_do(),
            k.do_loi,
            chu_db(k.muc_vao, 5),
            thanh_muc(k.muc_vao, 10),
            k.bo_dem.mat(),
            k.bo_dem.jitter_ms()
        ));
    }
}

fn in_tong_ket(p: &Phong) {
    log("Tổng kết (gói vào: từ client tới server):");
    log(" #  tên gói vào mất trễ thiếu cạn bị chặn cắt ngọn");
    let mut ds: Vec<&Khach> = p.khach.values().chain(p.da_roi.iter()).collect();
    ds.sort_by_key(|k| k.ma);
    for k in ds {
        let b = &k.bo_dem;
        log(&format!(
            "{:2}  {:<3} {:7} {:3} {:4} {:5} {:3} {:7} {:8}",
            k.ma,
            k.ten,
            b.nhan,
            b.mat(),
            b.tre,
            b.thieu,
            b.can,
            k.bi_chan,
            k.cat_ngon
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
    let cong_tcp: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(41620);
    let cong_udp: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(41621);
    let fail = |e: io::Error| -> ! {
        log(&format!("lỗi: {e}"));
        std::process::exit(1)
    };
    let srv = TcpListener::bind((host.as_str(), cong_tcp)).unwrap_or_else(|e| fail(e));
    let udp = Arc::new(UdpSocket::bind((host.as_str(), cong_udp)).unwrap_or_else(|e| fail(e)));
    ctrlc::set_handler(|| dung()).expect("Không cài được hàm bắt Ctrl+C");
    log(&format!("Điều khiển: TCP {host}:{cong_tcp}; âm thanh RTP: UDP {host}:{cong_udp}"));
    log(&format!("PCM 16 bit mono 16 kHz; khung {KHUNG_MS} ms = {MAU_MOI_KHUNG} mẫu = {BYTE_MOI_KHUNG} B"));
    thread::spawn(move || {
        for conn in srv.incoming().flatten() {
            conn.set_nodelay(true).ok(); // tin điều khiển đi ngay
            thread::spawn(move || phuc_vu(conn, cong_udp));
        }
    });
    let u1 = Arc::clone(&udp);
    thread::spawn(move || nhan_udp(u1));
    thread::spawn(move || vong_tron(udp));
    // bảng điều khiển: đọc lệnh từ bàn phím/kịch bản
    for dong in io::stdin().lock().lines().map_while(Result::ok) {
        let mut dong = dong.split('#').next().unwrap_or("").trim().to_string();
        if let Some(r) = dong.strip_prefix('@') {
            // "@t lệnh": chờ tới giây thứ t
            let (moc, lenh) = r.split_once(' ').unwrap_or((r, ""));
            let cho = moc.parse::<f64>().unwrap_or(0.0) - T0.elapsed().as_secs_f64();
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
        thread::park(); // hết stdin: tiếp tục phục vụ tới khi Ctrl+C
    }
}
