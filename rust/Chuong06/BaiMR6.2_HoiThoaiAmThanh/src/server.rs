//! Server phòng hội thoại âm thanh nhiều client (trộn tiếng "mix-minus").
//!
//! Hai kênh:
//!   - TCP (kênh điều khiển, tin cậy): mỗi dòng một thông điệp JSON – vào phòng, tự tắt/bật mic,
//!     báo âm lượng nghe; server gửi trạng thái mic, độ lợi, sự kiện, lời từ chối.
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
//! Mọi trạng thái dùng chung nằm trong MỘT Mutex (MayChu::phong); bốn luồng: nhận kết nối, mỗi
//! client một luồng điều khiển, nhận UDP, trộn 20 ms; luồng chính đọc lệnh bảng điều khiển từ stdin.
//!
//! Phần lõi (MayChu: khởi động, trộn, xử lý lệnh) dùng chung cho bản dòng lệnh và ứng dụng giao
//! diện (server_gui.rs). Bản giao diện thêm: server tự NÓI (micro của máy server là một nguồn trong
//! bộ trộn với SSRC_SERVER, mọi client đều nghe) và NGHE (phát ra loa máy server tổng tiếng các
//! client, không gồm tiếng của chính server). Mỗi khi đổi độ lợi của client (và một lần sau "chao"),
//! server gửi {"loai":"do_loi","gia_tri":g} để client hiển thị "Server Vol".
//!
//! Lệnh bảng điều khiển (stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ khi server chạy):
//!   tat_mic <tên|tat_ca>    bat_mic <tên|tat_ca>    do_loi <tên> <hệ số>
//!   am_luong_chung <hệ số>  trang_thai               thoat

use crate::chung::*;
use crate::thiet_bi::{day_ra_loa, HangMau, NguonMic};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub struct Khach {
    pub ma: u32,
    pub ten: String,
    conn: TcpStream, // để gửi tin điều khiển
    ip: IpAddr,
    pub udp: Option<SocketAddr>, // địa chỉ UDP học được từ gói đầu tiên
    pub tu_tat: bool,            // client tự tắt mic
    pub server_khoa: bool,       // server khóa mic
    pub do_loi: f64,             // độ lợi đầu vào (gain)
    pub am_luong_nghe: f64,      // client báo (chỉ để hiển thị)
    pub bo_dem: BoDemJitter,
    seq_ra: u32, // số thứ tự, nhãn thời gian của luồng trộn gửi cho client
    ts_ra: u32,
    pub muc_vao: f64,
    pub bi_chan: u64,   // gói bị bỏ vì mic đang tắt
    pub sai_nguon: u64, // gói bị bỏ vì sai địa chỉ nguồn
    pub cat_ngon: u64,  // số mẫu bị cắt ngọn trong luồng gửi cho client này
}

impl Khach {
    pub fn mic_mo(&self) -> bool {
        !self.tu_tat && !self.server_khoa // quy tắc 1
    }

    pub fn ly_do(&self) -> &'static str {
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

    /// Thông điệp mới (tương thích ngược – client cũ bỏ qua loại không biết): độ lợi server đặt.
    fn gui_do_loi(&self) {
        self.gui(json!({"loai": "do_loi", "gia_tri": self.do_loi}));
    }
}

#[derive(Default)]
pub struct Phong {
    pub khach: BTreeMap<u32, Khach>, // mã client (SSRC) -> Khach, theo thứ tự vào phòng
    pub da_roi: Vec<Khach>,          // client đã rời phòng (giữ lại để in tổng kết)
    ma_moi: u32,
    pub am_luong_chung: f64,
    // ---- chỉ bản giao diện dùng (bản dòng lệnh để None)
    pub mic_server: Option<NguonMic>, // Some = server đang nói (nguồn giả lập hoặc micro thật)
    pub mic_server_vol: f64,
    pub muc_mic_server: f64,          // dBFS tiếng server sau khi nhân Mic Volume
    pub loa_server: Option<HangMau>,  // Some = server đang nghe (đẩy tổng tiếng client ra loa)
    pub muc_nghe_server: f64,         // dBFS tổng tiếng các client (cái server nghe)
}

/// Lõi server: trạng thái phòng + hàm ghi nhật ký + cờ đang chạy.
pub struct MayChu {
    pub phong: Mutex<Phong>,
    t0: Instant,
    ghi_log: Box<dyn Fn(&str) + Send + Sync>,
    chay: AtomicBool,
    pub cong_udp: u16,
}

impl MayChu {
    pub fn log(&self, text: &str) {
        (self.ghi_log)(text);
    }

    pub fn giay(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    pub fn dang_chay(&self) -> bool {
        self.chay.load(Ordering::SeqCst)
    }

    /// Báo sự kiện cho mọi client (họ hiển thị và đánh mốc trong bản ghi).
    fn phat_su_kien(&self, p: &Phong, text: &str) {
        self.log(&format!("  => {text}"));
        for k in p.khach.values() {
            k.gui(json!({"loai": "su_kien", "noi_dung": text}));
        }
    }

    /// Server khóa (tat_mic) hoặc gỡ khóa (bat_mic) mic của các client `ds`.
    pub fn khoa_mic(&self, p: &mut Phong, ds: &[u32], khoa: bool, ai_txt: &str) {
        for ma in ds {
            if let Some(k) = p.khach.get_mut(ma) {
                k.server_khoa = khoa;
                k.gui_mic();
            }
        }
        if khoa {
            self.phat_su_kien(p, &format!("server tắt mic {ai_txt}"));
        } else {
            // quy tắc 3: gỡ khóa nhưng không bật hộ mic người dùng đã tự tắt
            let van_tat: Vec<String> = ds.iter().filter_map(|m| p.khach.get(m)).filter(|k| k.tu_tat).map(|k| k.ten.clone()).collect();
            let them = if van_tat.is_empty() { String::new() } else { format!(" ({} vẫn tự tắt)", van_tat.join(", ")) };
            self.phat_su_kien(p, &format!("server bật mic {ai_txt}{them}"));
        }
    }

    /// Đặt độ lợi đầu vào (quy tắc 4) và báo cho từng client liên quan bằng thông điệp "do_loi".
    pub fn dat_do_loi(&self, p: &mut Phong, ds: &[u32], g: f64, ai: &str) {
        for ma in ds {
            if let Some(k) = p.khach.get_mut(ma) {
                k.do_loi = g;
                k.gui_do_loi();
            }
        }
        self.phat_su_kien(p, &format!("server đặt độ lợi {ai} = {g:.2}"));
    }

    pub fn dat_am_luong_chung(&self, p: &mut Phong, g: f64) {
        p.am_luong_chung = g;
        self.phat_su_kien(p, &format!("server đặt âm lượng chung {g:.2}"));
    }

    /// Dừng server: các luồng nền thấy cờ và thoát; đóng mọi kết nối TCP để client biết.
    pub fn dung_lai(&self) {
        self.chay.store(false, Ordering::SeqCst);
        let p = self.phong.lock().unwrap();
        for k in p.khach.values() {
            let _ = k.conn.shutdown(Shutdown::Both);
        }
    }
}

// ---------------------------------------------------------------- Kênh điều khiển TCP
fn phuc_vu(mc: Arc<MayChu>, conn: TcpStream) {
    let addr = conn.peer_addr().unwrap();
    let mut lines = BufReader::new(conn.try_clone().unwrap()).lines();
    let tin: Value = lines.next().and_then(|l| l.ok()).and_then(|l| serde_json::from_str(&l).ok()).unwrap_or(Value::Null);
    let ten: String = match (tin["loai"].as_str(), tin["ten"].as_str()) {
        (Some("vao"), Some(t)) if !t.is_empty() => t.chars().take(8).collect(),
        _ => return,
    };
    let ma = {
        let mut p = mc.phong.lock().unwrap();
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
        k.gui(json!({"loai": "chao", "ma": ma, "cong_udp": mc.cong_udp, "tan_so_mau": 16000, "khung_ms": KHUNG_MS}));
        k.gui_do_loi(); // một lần sau "chao": client biết độ lợi hiện hành
        mc.log(&format!("#{ma} {ten} vào phòng từ {addr}"));
        p.khach.insert(ma, k);
        p.khach.get_mut(&ma).unwrap().gui_mic();
        mc.phat_su_kien(&p, &format!("{ten} vào phòng"));
        ma
    };
    for line in lines.map_while(Result::ok) {
        let Ok(tin) = serde_json::from_str::<Value>(&line) else { break };
        let mut p = mc.phong.lock().unwrap();
        xu_ly_tin_client(&mc, &mut p, ma, &tin);
    }
    let mut p = mc.phong.lock().unwrap();
    if let Some(k) = p.khach.remove(&ma) {
        mc.log(&format!("#{ma} {} rời phòng", k.ten));
        p.da_roi.push(k);
        mc.phat_su_kien(&p, &format!("{ten} rời phòng"));
    }
}

fn xu_ly_tin_client(mc: &MayChu, p: &mut Phong, ma: u32, tin: &Value) {
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
                mc.log(&format!("{ten} xin bật mic -> từ chối (server đang khóa)"));
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
            mc.log(&format!("{ten} tự đặt âm lượng nghe {:.2}", k.am_luong_nghe));
            None
        }
        _ => None,
    };
    if let Some(s) = su_kien {
        mc.phat_su_kien(p, &s);
    }
}

// ---------------------------------------------------------------- Kênh dữ liệu UDP
fn nhan_udp(mc: Arc<MayChu>, sock: Arc<UdpSocket>) {
    let mut buf = [0u8; 2048];
    while mc.dang_chay() {
        // hết thời gian chờ (100 ms) cũng trả về Err: quay lại kiểm tra cờ dừng
        let Ok((n, addr)) = sock.recv_from(&mut buf) else { continue };
        let Some(r) = tach_rtp(&buf[..n]) else { continue };
        let mut p = mc.phong.lock().unwrap();
        let Some(k) = p.khach.get_mut(&r.ssrc) else { continue };
        if k.udp.is_none() && addr.ip() == k.ip {
            // đăng ký địa chỉ UDP lần đầu
            k.udp = Some(addr);
            mc.log(&format!("#{} {} đăng ký UDP {addr}", k.ma, k.ten));
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
        k.bo_dem.nhan_goi(r.seq, r.ts, r.payload, mc.giay());
    }
}

/// Mỗi 20 ms: trộn và gửi cho từng người nghe. Hẹn giờ tuyệt đối để không trôi nhịp.
fn vong_tron(mc: Arc<MayChu>, sock: Arc<UdpSocket>) {
    let mut ke_tiep = Instant::now();
    while mc.dang_chay() {
        {
            let mut guard = mc.phong.lock().unwrap();
            let p = &mut *guard; // mượn riêng từng trường của Phong
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
            // 1b. (bản giao diện) server NGHE: tổng tiếng các client – chưa gồm tiếng server
            if p.loa_server.is_some() || p.mic_server.is_some() {
                let nghe: Vec<i16> = tong.iter().map(|&x| cat_ngon(x)).collect();
                p.muc_nghe_server = muc_dbfs(&nghe);
                if let Some(loa) = &p.loa_server {
                    day_ra_loa(loa, &nghe);
                }
            }
            // 1c. (bản giao diện) server NÓI: thêm một nguồn vào tổng T, mọi client đều nghe
            if let Some(nguon) = p.mic_server.as_mut() {
                let mau = crate::thiet_bi::nhan_he_so(&nguon.lay_khung(), p.mic_server_vol);
                p.muc_mic_server = muc_dbfs(&mau);
                for (t, &x) in tong.iter_mut().zip(&mau) {
                    *t += x as f64;
                }
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

/// Mở cổng TCP/UDP, khởi động các luồng nền, trả về lõi server.
/// `ghi_log` quyết định nhật ký đi đâu: bản dòng lệnh in ra stdout, bản giao diện đưa vào khung nhật ký.
pub fn khoi_dong(host: &str, cong_tcp: u16, cong_udp: u16, ghi_log: Box<dyn Fn(&str) + Send + Sync>) -> io::Result<Arc<MayChu>> {
    let srv = TcpListener::bind((host, cong_tcp))?;
    let udp = Arc::new(UdpSocket::bind((host, cong_udp))?);
    // không chặn mãi: để luồng nền thấy được lệnh dừng (Stop Server)
    srv.set_nonblocking(true)?;
    udp.set_read_timeout(Some(Duration::from_millis(100)))?;
    let mc = Arc::new(MayChu {
        phong: Mutex::new(Phong { am_luong_chung: 1.0, mic_server_vol: 1.0, muc_mic_server: f64::NEG_INFINITY, muc_nghe_server: f64::NEG_INFINITY, ..Default::default() }),
        t0: Instant::now(),
        ghi_log,
        chay: AtomicBool::new(true),
        cong_udp,
    });
    mc.log(&format!("Điều khiển: TCP {host}:{cong_tcp}; âm thanh RTP: UDP {host}:{cong_udp}"));
    mc.log(&format!("PCM 16 bit mono 16 kHz; khung {KHUNG_MS} ms = {MAU_MOI_KHUNG} mẫu = {BYTE_MOI_KHUNG} B"));
    let m = Arc::clone(&mc);
    thread::spawn(move || {
        while m.dang_chay() {
            match srv.accept() {
                Ok((conn, _)) => {
                    // Windows: socket nhận được kế thừa chế độ không chặn của socket nghe -> đặt lại
                    conn.set_nonblocking(false).ok();
                    conn.set_nodelay(true).ok(); // tin điều khiển đi ngay
                    let m2 = Arc::clone(&m);
                    thread::spawn(move || phuc_vu(m2, conn));
                }
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        }
    });
    let (m1, u1) = (Arc::clone(&mc), Arc::clone(&udp));
    thread::spawn(move || nhan_udp(m1, u1));
    let m2 = Arc::clone(&mc);
    thread::spawn(move || vong_tron(m2, udp));
    Ok(mc)
}

// ---------------------------------------------------------------- Bảng điều khiển (dòng lệnh)
/// Mã các client theo tên, hoặc tất cả nếu ten = tat_ca.
fn chon(p: &Phong, ten: &str) -> Vec<u32> {
    p.khach.values().filter(|k| ten == "tat_ca" || k.ten == ten).map(|k| k.ma).collect()
}

fn lenh_server(mc: &MayChu, dong: &str) -> bool {
    let w: Vec<&str> = dong.split_whitespace().collect();
    if w.is_empty() {
        return true;
    }
    mc.log(&format!("t={:.1}s > {dong}", mc.giay()));
    let mut p = mc.phong.lock().unwrap();
    match w.as_slice() {
        [lenh @ ("tat_mic" | "bat_mic"), ai] => {
            let ds = chon(&p, ai);
            if ds.is_empty() {
                mc.log(&format!("không có client {ai}"));
                return true;
            }
            let ai_txt = if *ai == "tat_ca" { "tất cả".to_string() } else { ai.to_string() };
            mc.khoa_mic(&mut p, &ds, *lenh == "tat_mic", &ai_txt);
        }
        ["do_loi", ai, g] => {
            let Ok(g) = g.parse::<f64>() else {
                mc.log("lệnh không hợp lệ");
                return true;
            };
            let ds = chon(&p, ai);
            mc.dat_do_loi(&mut p, &ds, g, ai);
        }
        ["am_luong_chung", g] => {
            let Ok(g) = g.parse::<f64>() else {
                mc.log("lệnh không hợp lệ");
                return true;
            };
            mc.dat_am_luong_chung(&mut p, g);
        }
        ["trang_thai"] => in_trang_thai(mc, &p),
        ["thoat"] => return false,
        _ => mc.log("lệnh không hợp lệ"),
    }
    true
}

fn in_trang_thai(mc: &MayChu, p: &Phong) {
    mc.log(&format!("Trạng thái: {} client, âm lượng chung {:.2}", p.khach.len(), p.am_luong_chung));
    mc.log(" #  tên mic          lợi   mức vào            mất  jit.");
    for k in p.khach.values() {
        mc.log(&format!(
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

pub fn in_tong_ket(mc: &MayChu, p: &Phong) {
    mc.log("Tổng kết (gói vào: từ client tới server):");
    mc.log(" #  tên gói vào mất trễ thiếu cạn bị chặn cắt ngọn");
    let mut ds: Vec<&Khach> = p.khach.values().chain(p.da_roi.iter()).collect();
    ds.sort_by_key(|k| k.ma);
    for k in ds {
        let b = &k.bo_dem;
        mc.log(&format!(
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

fn dung(mc: &MayChu) -> ! {
    mc.log("Dừng server (Ctrl+C)");
    in_tong_ket(mc, &mc.phong.lock().unwrap());
    std::process::exit(0)
}

pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let cong_tcp: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(41620);
    let cong_udp: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(41621);
    let mc = khoi_dong(&host, cong_tcp, cong_udp, Box::new(|text| println!("[server] {text}"))).unwrap_or_else(|e| {
        println!("[server] lỗi: {e}");
        std::process::exit(1)
    });
    let m = Arc::clone(&mc);
    ctrlc::set_handler(move || dung(&m)).expect("Không cài được hàm bắt Ctrl+C");
    // bảng điều khiển: đọc lệnh từ bàn phím/kịch bản
    for dong in io::stdin().lock().lines().map_while(Result::ok) {
        let mut dong = dong.split('#').next().unwrap_or("").trim().to_string();
        if let Some(r) = dong.strip_prefix('@') {
            // "@t lệnh": chờ tới giây thứ t
            let (moc, lenh) = r.split_once(' ').unwrap_or((r, ""));
            let cho = moc.parse::<f64>().unwrap_or(0.0) - mc.giay();
            if cho > 0.0 {
                thread::sleep(Duration::from_secs_f64(cho));
            }
            dong = lenh.trim().to_string();
        }
        if !dong.is_empty() && !lenh_server(&mc, &dong) {
            dung(&mc);
        }
    }
    loop {
        thread::park(); // hết stdin: tiếp tục phục vụ tới khi Ctrl+C
    }
}
