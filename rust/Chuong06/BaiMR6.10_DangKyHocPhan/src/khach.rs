//! Bài mở rộng MR6.10 – Lõi client dùng chung (giao diện người dùng, bộ mô phỏng, quản trị) + dòng lệnh quản trị.
//!
//! Tệp này GIỐNG NHAU ở cả 8 bài tranh chấp (MR6.6–MR6.13). Không có gì về nghiệp vụ.
//!
//! KetNoiKhach – một kết nối TCP:
//!   ket_noi(...)      chào, đọc trả lời chào và cap_nhat đầu tiên (đồng bộ) -> trang_thai
//!   gui_yc(loai, truong, khi_kq)   gửi một yêu cầu (tự đánh ma_yc), KHÔNG chờ: khi_kq(tin, rtt_ms) được
//!                     gọi khi có trả lời cùng ma_yc (pipelining: gửi tiếp được ngay)
//!   gui_lo([...])     gửi nhiều yêu cầu trong MỘT lần ghi (bộ mô phỏng "bắn đồng loạt")
//!   hoi(loai, truong) gửi rồi CHỜ trả lời (cần luồng đọc chạy nền) – dùng cho quản trị
//!   bat_dau_doc()     luồng đọc chạy nền (giao diện);  vong_doc(xong)  đọc ngay trên luồng gọi (mô phỏng)
//! Gọi lại (từ luồng đọc): khi_cap_nhat(tin), khi_tin(tin) (su_kien, loi, trả lời lạ), khi_dong().
//! QuanTri – kết nối vai_tro "quan_tri": che_do(), tre(), dat_lai(ts), thong_ke().
//!
//! Dòng lệnh quản trị:  khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke

use crate::chung::*;
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub type KhiKq = Box<dyn FnOnce(&Obj, f64) + Send>;
pub type GoiLai = Box<dyn FnMut(&Obj) + Send>;

struct PhanGui {
    sock: Option<TcpStream>,
    ma: i64,
    cho: HashMap<i64, (Option<KhiKq>, Instant)>, // ma_yc -> (khi_kq, thời điểm gửi)
}

pub struct KetNoiKhach {
    gui: Mutex<PhanGui>, // bảo vệ _cho và việc gửi
    doc: Mutex<Option<BufReader<TcpStream>>>,
    pub mo: AtomicBool,
    pub chao: Mutex<Option<Obj>>,
    pub trang_thai: Mutex<Option<Obj>>,
    khi_cap_nhat: Mutex<GoiLai>,
    khi_tin: Mutex<GoiLai>,
    khi_dong: Mutex<Box<dyn FnMut() + Send>>,
}

impl Default for KetNoiKhach {
    fn default() -> Self {
        KetNoiKhach {
            gui: Mutex::new(PhanGui { sock: None, ma: 0, cho: HashMap::new() }),
            doc: Mutex::new(None),
            mo: AtomicBool::new(false),
            chao: Mutex::new(None),
            trang_thai: Mutex::new(None),
            khi_cap_nhat: Mutex::new(Box::new(|_| {})),
            khi_tin: Mutex::new(Box::new(|_| {})),
            khi_dong: Mutex::new(Box::new(|| {})),
        }
    }
}

impl KetNoiKhach {
    pub fn dat_khi_cap_nhat(&self, f: GoiLai) {
        *self.khi_cap_nhat.lock().unwrap() = f;
    }
    pub fn dat_khi_tin(&self, f: GoiLai) {
        *self.khi_tin.lock().unwrap() = f;
    }
    pub fn dat_khi_dong(&self, f: Box<dyn FnMut() + Send>) {
        *self.khi_dong.lock().unwrap() = f;
    }
    pub fn dang_mo(&self) -> bool {
        self.mo.load(Ordering::SeqCst)
    }
    pub fn trang_thai(&self) -> Option<Obj> {
        self.trang_thai.lock().unwrap().clone()
    }

    // ------------------------------------------------------------ kết nối
    pub fn ket_noi(&self, host: &str, cong: u16, ten: &str, vai_tro: &str, so_nguoi: Option<usize>, cho_s: f64) -> Result<Obj, String> {
        let han = Duration::from_secs_f64(cho_s);
        let mut loi = String::from("không phân giải được địa chỉ");
        let mut sock = None;
        for dc in (host, cong).to_socket_addrs().map_err(|e| e.to_string())? {
            match TcpStream::connect_timeout(&dc, han) {
                Ok(s) => {
                    sock = Some(s);
                    break;
                }
                Err(e) => loi = e.to_string(),
            }
        }
        let mut sock = sock.ok_or(loi)?;
        let _ = sock.set_nodelay(true);
        let _ = sock.set_read_timeout(Some(han));
        let mut chao = obj(json!({"loai": "chao", "ten": ten, "vai_tro": vai_tro}));
        if let Some(m) = so_nguoi {
            chao.insert("so_nguoi".into(), json!(m));
        }
        gui_json(&mut sock, &chao).map_err(|e| e.to_string())?;
        let mut tep = BufReader::new(sock.try_clone().map_err(|e| e.to_string())?);
        let tin = doc_dong(&mut tep).map_err(|e| e.to_string())?;
        let tin = match tin {
            Some(t) if lay_str(&t, "loai") == Some("chao") => t,
            Some(t) => return Err(lay_str(&t, "ly_do").unwrap_or("máy chủ đóng kết nối").to_string()),
            None => return Err("máy chủ đóng kết nối".into()),
        };
        *self.chao.lock().unwrap() = Some(tin.clone());
        *self.trang_thai.lock().unwrap() = doc_dong(&mut tep).map_err(|e| e.to_string())?; // máy chủ gửi ngay một cap_nhat sau lời chào
        let _ = sock.set_read_timeout(None);
        let _ = tep.get_ref().set_read_timeout(None); // Windows: bản sao (try_clone) giữ thời hạn riêng, phải gỡ cả ở đây
        self.gui.lock().unwrap().sock = Some(sock);
        *self.doc.lock().unwrap() = Some(tep);
        self.mo.store(true, Ordering::SeqCst);
        Ok(tin)
    }

    pub fn dong(&self) {
        self.mo.store(false, Ordering::SeqCst);
        if let Some(s) = self.gui.lock().unwrap().sock.as_ref() {
            let _ = s.shutdown(Shutdown::Both);
        }
    }

    /// Đóng mà không gọi khi_dong (người dùng tự rời).
    pub fn dong_im(&self) {
        self.dat_khi_dong(Box::new(|| {}));
        self.dong();
    }

    // ------------------------------------------------------------ gửi
    fn dang_ky(g: &mut PhanGui, loai: &str, truong: Obj, khi_kq: Option<KhiKq>) -> (i64, Obj) {
        g.ma += 1;
        let ma = g.ma;
        g.cho.insert(ma, (khi_kq, Instant::now()));
        let mut tin = obj(json!({"loai": loai, "ma_yc": ma}));
        tin.extend(truong);
        (ma, tin)
    }

    pub fn gui_yc(&self, loai: &str, truong: Obj, khi_kq: Option<KhiKq>) -> Result<i64, String> {
        let mut g = self.gui.lock().unwrap();
        let (ma, tin) = Self::dang_ky(&mut g, loai, truong, khi_kq);
        let s = g.sock.as_mut().ok_or("chưa kết nối")?;
        gui_json(s, &tin).map_err(|e| e.to_string())?;
        Ok(ma)
    }

    /// ds: danh sách (loai, truong, khi_kq). Một lần ghi cho cả lô.
    pub fn gui_lo(&self, ds: Vec<(String, Obj, Option<KhiKq>)>) -> Result<(), String> {
        let mut g = self.gui.lock().unwrap();
        let mut du_lieu = Vec::new();
        for (loai, truong, khi_kq) in ds {
            du_lieu.extend(ma_hoa(&Self::dang_ky(&mut g, &loai, truong, khi_kq).1));
        }
        g.sock.as_mut().ok_or("chưa kết nối")?.write_all(&du_lieu).map_err(|e| e.to_string())
    }

    pub fn hoi(&self, loai: &str, truong: Obj, cho_s: f64) -> Result<Obj, String> {
        let (tx, rx) = mpsc::channel();
        self.gui_yc(
            loai,
            truong,
            Some(Box::new(move |tin: &Obj, _| {
                let _ = tx.send(tin.clone());
            })),
        )?;
        rx.recv_timeout(Duration::from_secs_f64(cho_s)).map_err(|_| format!("không có trả lời cho {loai}"))
    }

    // ------------------------------------------------------------ đọc
    pub fn bat_dau_doc(self: &Arc<Self>) {
        let kn = Arc::clone(self);
        std::thread::spawn(move || kn.vong_doc(None));
    }

    /// Đọc và phân phát tới khi kết nối đóng, hoặc (nếu có) xong() đúng sau một thông điệp.
    pub fn vong_doc(&self, xong: Option<&dyn Fn() -> bool>) {
        let Some(mut tep) = self.doc.lock().unwrap().take() else { return };
        loop {
            if xong.is_some_and(|f| f()) {
                return; // đủ việc: kết nối vẫn mở, người gọi tự đóng
            }
            match doc_dong(&mut tep) {
                Ok(Some(tin)) => self.phan_phat(tin),
                _ => break,
            }
        }
        self.mo.store(false, Ordering::SeqCst);
        (self.khi_dong.lock().unwrap())();
    }

    fn phan_phat(&self, tin: Obj) {
        let ma = lay_i64(&tin, "ma_yc");
        let cho = match ma {
            Some(m) if !dung(&tin, "lap_lai") => self.gui.lock().unwrap().cho.remove(&m),
            _ => None,
        };
        if let Some((khi_kq, t0)) = cho {
            if let Some(f) = khi_kq {
                f(&tin, t0.elapsed().as_secs_f64() * 1000.0);
            }
        } else if lay_str(&tin, "loai") == Some("cap_nhat") {
            *self.trang_thai.lock().unwrap() = Some(tin.clone());
            (self.khi_cap_nhat.lock().unwrap())(&tin);
        } else {
            (self.khi_tin.lock().unwrap())(&tin);
        }
    }
}

/// Kết nối quản trị (đồng bộ): mỗi lệnh chờ trả lời; bị từ chối -> Err(ly_do).
pub struct QuanTri {
    pub kn: Arc<KetNoiKhach>,
}

impl QuanTri {
    pub fn new(host: &str, cong: u16, ten: &str) -> Result<QuanTri, String> {
        let kn = Arc::new(KetNoiKhach::default());
        kn.ket_noi(host, cong, ten, "quan_tri", None, 5.0)?;
        kn.bat_dau_doc();
        Ok(QuanTri { kn })
    }

    fn lenh(&self, loai: &str, truong: Obj) -> Result<Obj, String> {
        let kq = self.kn.hoi(loai, truong, 30.0)?;
        if kq.contains_key("ok") && !dung(&kq, "ok") {
            // như kq.get("ok", True) của Python: thong_ke không có "ok" -> thành công
            return Err(lay_str(&kq, "ly_do").unwrap_or("bị từ chối").to_string());
        }
        Ok(kq)
    }

    pub fn che_do(&self, gia_tri: &str) -> Result<Obj, String> {
        self.lenh("che_do", obj(json!({"gia_tri": gia_tri})))
    }
    pub fn tre(&self, ms: f64) -> Result<Obj, String> {
        self.lenh("tre", obj(json!({"ms": ms})))
    }
    pub fn dat_lai(&self, tham_so: Obj) -> Result<Obj, String> {
        self.lenh("dat_lai", tham_so)
    }
    pub fn thong_ke(&self) -> Result<Obj, String> {
        self.lenh("thong_ke", Obj::new())
    }
    pub fn dong(&self) {
        self.kn.dong_im();
    }
}

/// khach <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke
pub fn run(args: &[String]) {
    if args.len() < 3 {
        println!("Dùng: khach <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke");
        return;
    }
    let bao = |text: &str| in_dong("quan_tri", text);
    let (host, cong, lenh, ts) = (&args[0], args[1].parse::<u16>().unwrap_or(0), args[2].as_str(), &args[3..]);
    let qt = match QuanTri::new(host, cong, "quan_tri") {
        Ok(q) => q,
        Err(e) => {
            bao(&format!("Không kết nối được: {e}"));
            std::process::exit(1);
        }
    };
    let mut cau = vec![lenh.to_string()];
    cau.extend(ts.iter().cloned());
    bao(&format!("> {}", cau.join(" ")));
    let kq = match (lenh, ts.len()) {
        ("che_do", 1) => qt.che_do(&ts[0]).map(|_| None),
        ("tre", 1) => ts[0].parse::<f64>().map_err(|_| format!("could not convert string to float: '{}'", ts[0])).and_then(|ms| qt.tre(ms)).map(|_| None),
        ("dat_lai", _) => doc_tham_so(ts).and_then(|t| qt.dat_lai(t)).map(|_| None),
        ("thong_ke", _) => qt.thong_ke().map(Some),
        _ => {
            bao("lệnh không hợp lệ");
            qt.dong();
            return;
        }
    };
    match kq {
        Ok(tk) => {
            bao("OK");
            if let Some(tk) = tk {
                for d in dong_thong_ke(&tk) {
                    bao(&d);
                }
            }
        }
        Err(e) => bao(&format!("Bị từ chối: {e}")),
    }
    qt.dong();
}
