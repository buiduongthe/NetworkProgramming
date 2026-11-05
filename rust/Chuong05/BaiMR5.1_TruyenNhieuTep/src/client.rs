//! Client truyền nhiều tệp đồng thời, có điều khiển start/pause/resume/stop,
//! giới hạn tốc độ theo phiên và theo tệp (thùng token), tải tiếp bằng offset + kiểm SHA-256.
//!
//! Lệnh của giao diện client (mỗi dòng một lệnh):
//!   start <tên> [download|upload] [tốc_độ]   bắt đầu một lượt (mặc định download); tốc_độ = giới hạn tệp
//!   pause <tên>        tạm dừng (giữ tệp .part)        resume <tên>   tải tiếp từ offset
//!   stop  <tên>        dừng hẳn (xóa tệp .part)         speed <tốc_độ> đổi giới hạn tốc độ phiên
//!   status             xem tiến trình                   quit           thoát
//! Đơn vị tốc độ: 512K, 1M, 2MiB, hoặc số byte/giây.
//!
//! Mỗi lượt truyền chạy trong một luồng riêng; luồng chính điều khiển bằng cách đổi trạng thái
//! (Mutex) mà luồng truyền kiểm tra sau mỗi khối dữ liệu. Số byte đã truyền là AtomicU64 để luồng
//! báo cáo đọc được mà không cần khóa.

use crate::khung::{dinh_dang_byte, doc_so_byte, gui_khung, nhan_khung, sha256_tep, ThungToken, CHUNK, KHONG_GIOI_HAN};
use crate::server::phuc_vu;
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CHAY: &str = "chạy";
const TAM_DUNG: &str = "tạm dừng";
const DUNG: &str = "dừng";
const XONG: &str = "xong";
const LOI: &str = "lỗi";

/// Một lượt truyền (một tệp). Chạy trong luồng riêng, điều khiển qua trạng thái.
pub struct LuotTruyen {
    ten: String,
    huong: String, // "download" | "upload"
    cuc_bo: PathBuf, // đường tệp phía client
    trang_thai: Mutex<&'static str>,
    tong: AtomicU64,
    xong_byte: AtomicU64,
    off_dau: AtomicU64, // offset lúc bắt đầu lượt này (để đo tốc độ khi tải tiếp)
    sha_ok: Mutex<Option<bool>>,
    t0: Instant,
    bucket: ThungToken, // giới hạn riêng của tệp
}

impl LuotTruyen {
    fn new(ten: &str, huong: &str, cuc_bo: PathBuf, rate_tep: f64) -> LuotTruyen {
        LuotTruyen {
            ten: ten.into(),
            huong: huong.into(),
            cuc_bo,
            trang_thai: Mutex::new(CHAY),
            tong: AtomicU64::new(0),
            xong_byte: AtomicU64::new(0),
            off_dau: AtomicU64::new(0),
            sha_ok: Mutex::new(None),
            t0: Instant::now(),
            bucket: ThungToken::new(rate_tep),
        }
    }
    fn tt(&self) -> &'static str {
        *self.trang_thai.lock().unwrap()
    }
    fn dat(&self, s: &'static str) {
        *self.trang_thai.lock().unwrap() = s;
    }
    fn dung_khi(&self) -> bool {
        matches!(self.tt(), TAM_DUNG | DUNG)
    }
    fn xong(&self) -> u64 {
        self.xong_byte.load(Ordering::SeqCst)
    }
    fn tong(&self) -> u64 {
        self.tong.load(Ordering::SeqCst)
    }
}

/// Bộ quản lý: giữ danh sách lượt truyền (theo thứ tự bắt đầu) và thùng token của phiên.
#[derive(Clone)]
pub struct QuanLy {
    host: String,
    port: u16,
    thu_muc: PathBuf,
    kho_server: PathBuf, // để suy ra offset khi upload tiếp
    session: Arc<ThungToken>,
    luot: Arc<Mutex<Vec<(String, Arc<LuotTruyen>)>>>,
}

impl QuanLy {
    fn new(host: &str, port: u16, thu_muc_tai: &Path, kho_server: &Path) -> QuanLy {
        let _ = fs::create_dir_all(thu_muc_tai);
        QuanLy {
            host: host.into(),
            port,
            thu_muc: thu_muc_tai.into(),
            kho_server: kho_server.into(),
            session: Arc::new(ThungToken::new(KHONG_GIOI_HAN)),
            luot: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn lay(&self, ten: &str) -> Option<Arc<LuotTruyen>> {
        self.luot.lock().unwrap().iter().find(|(t, _)| t == ten).map(|(_, l)| Arc::clone(l))
    }

    /// Thêm (hoặc thay tại chỗ) một lượt truyền, giữ thứ tự như dict của Python.
    fn ghi(&self, t: Arc<LuotTruyen>) {
        let mut v = self.luot.lock().unwrap();
        match v.iter_mut().find(|(ten, _)| *ten == t.ten) {
            Some(slot) => slot.1 = t,
            None => v.push((t.ten.clone(), t)),
        }
    }

    // ------------- Điều khiển -------------
    fn dat_toc_do_phien(&self, rate: f64) {
        self.session.dat_rate(rate);
        println!("[client] Đặt tốc độ phiên = {}/s", dinh_dang_byte(rate as u64));
    }

    /// Khởi động luồng truyền cho lượt t.
    fn chay(&self, t: Arc<LuotTruyen>) {
        self.ghi(Arc::clone(&t));
        let ql = self.clone();
        thread::spawn(move || if t.huong == "download" { ql.tai_xuong(&t) } else { ql.tai_len(&t) });
    }

    fn bat_dau(&self, ten: &str, huong: &str, rate_tep: f64) {
        if self.lay(ten).is_some_and(|t| t.tt() == CHAY) {
            println!("[client] {ten} đang chạy");
            return;
        }
        let t = Arc::new(LuotTruyen::new(ten, huong, self.thu_muc.join(ten), rate_tep));
        self.chay(t);
        let gh = if rate_tep >= KHONG_GIOI_HAN { String::new() } else { format!(", giới hạn tệp {}/s", dinh_dang_byte(rate_tep as u64)) };
        println!("[client] BẮT ĐẦU {huong} {ten}{gh}");
    }

    fn tam_dung(&self, ten: &str) {
        if let Some(t) = self.lay(ten).filter(|t| t.tt() == CHAY) {
            t.dat(TAM_DUNG);
            println!("[client] TẠM DỪNG {ten} (giữ .part)");
        }
    }

    fn tai_tiep(&self, ten: &str) {
        if let Some(t) = self.lay(ten).filter(|t| t.tt() == TAM_DUNG) {
            println!("[client] TẢI TIẾP {ten}");
            self.bat_dau(ten, &t.huong, t.bucket.rate());
        }
    }

    fn dung_han(&self, ten: &str) {
        if let Some(t) = self.lay(ten).filter(|t| matches!(t.tt(), CHAY | TAM_DUNG)) {
            t.dat(DUNG);
            println!("[client] DỪNG {ten} (xóa .part)");
        }
    }

    fn trang_thai(&self) {
        println!("[client] --- Trạng thái ---");
        for (ten, t) in self.luot.lock().unwrap().iter() {
            let (x, n) = (t.xong(), t.tong());
            let pct = if n > 0 { 100.0 * x as f64 / n as f64 } else { 0.0 };
            println!(
                "[client]   {ten:12} {:8} {:8} {} {pct:5.1}%  {}/{}",
                t.huong,
                t.tt(),
                thanh_tien_do(pct),
                dinh_dang_byte(x),
                dinh_dang_byte(n)
            );
        }
    }

    fn ket_noi(&self) -> io::Result<TcpStream> {
        TcpStream::connect((self.host.as_str(), self.port))
    }

    // ------------- Thân luồng truyền -------------
    fn tai_xuong(&self, t: &LuotTruyen) {
        let part = PathBuf::from(format!("{}.part", t.cuc_bo.display()));
        let off = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        t.off_dau.store(off, Ordering::SeqCst);
        let r: io::Result<()> = (|| {
            let mut s = self.ket_noi()?;
            gui_khung(&s, &json!({"op": "download", "name": t.ten, "offset": off}))?;
            let resp = nhan_khung(&s)?;
            if resp["ok"] != true {
                t.dat(LOI);
                println!("[client] {}: lỗi {}", t.ten, resp["loi"].as_str().unwrap_or("?"));
                return Ok(());
            }
            let tong = resp["size"].as_u64().unwrap_or(0);
            let sha_server = resp["sha256"].as_str().unwrap_or("").to_string();
            t.tong.store(tong, Ordering::SeqCst);
            t.xong_byte.store(off, Ordering::SeqCst);
            if off >= tong {
                // đã đủ: chỉ cần kiểm và đổi tên
                return self.hoan_tat_tai_xuong(t, &part, &sha_server);
            }
            let mut f = if off > 0 { OpenOptions::new().write(true).open(&part)? } else { fs::File::create(&part)? };
            f.seek(SeekFrom::Start(off))?;
            let mut buf = vec![0u8; CHUNK];
            while t.xong() < tong {
                match t.tt() {
                    DUNG => {
                        drop(f);
                        let _ = fs::remove_file(&part);
                        return Ok(());
                    }
                    TAM_DUNG => return Ok(()), // f và s tự đóng khi ra khỏi hàm
                    _ => {}
                }
                let n = s.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                f.write_all(&buf[..n])?;
                t.xong_byte.fetch_add(n as u64, Ordering::SeqCst);
                self.session.tieu_thu(n, || t.dung_khi()); // điều tiết theo phiên
                t.bucket.tieu_thu(n, || t.dung_khi()); // và theo tệp
            }
            drop(f);
            drop(s);
            if t.xong() >= tong {
                self.hoan_tat_tai_xuong(t, &part, &sha_server)?;
            }
            Ok(())
        })();
        if let Err(e) = r {
            if !t.dung_khi() {
                t.dat(LOI);
                println!("[client] {}: lỗi kết nối {e}", t.ten);
            }
        }
    }

    fn hoan_tat_tai_xuong(&self, t: &LuotTruyen, part: &Path, sha_server: &str) -> io::Result<()> {
        let ok = sha256_tep(part)? == sha_server;
        *t.sha_ok.lock().unwrap() = Some(ok);
        fs::rename(part, &t.cuc_bo)?;
        let tg = t.t0.elapsed().as_secs_f64();
        t.dat(XONG);
        let tong = t.tong();
        println!(
            "[client] XONG download {}: {} trong {tg:.1}s ({}/s), SHA-256 {}",
            t.ten,
            dinh_dang_byte(tong),
            dinh_dang_byte((tong as f64 / tg.max(0.001)) as u64),
            if ok { "KHỚP" } else { "SAI" }
        );
        Ok(())
    }

    fn tai_len(&self, t: &LuotTruyen) {
        let r: io::Result<()> = (|| {
            let total = fs::metadata(&t.cuc_bo)?.len();
            let part_server = self.kho_server.join(format!("{}.part", t.ten));
            let off = fs::metadata(&part_server).map(|m| m.len()).unwrap_or(0);
            t.off_dau.store(off, Ordering::SeqCst);
            let sha = sha256_tep(&t.cuc_bo)?;
            let mut s = self.ket_noi()?;
            gui_khung(&s, &json!({"op": "upload", "name": t.ten, "offset": off, "size": total, "sha256": sha}))?;
            let resp = nhan_khung(&s)?;
            if resp["ok"] != true {
                t.dat(LOI);
                return Ok(());
            }
            t.tong.store(total, Ordering::SeqCst);
            t.xong_byte.store(off, Ordering::SeqCst);
            let mut f = fs::File::open(&t.cuc_bo)?;
            f.seek(SeekFrom::Start(off))?;
            let mut buf = vec![0u8; CHUNK];
            while t.xong() < total {
                if t.dung_khi() {
                    return Ok(());
                }
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                self.session.tieu_thu(n, || t.dung_khi());
                t.bucket.tieu_thu(n, || t.dung_khi());
                s.write_all(&buf[..n])?;
                t.xong_byte.fetch_add(n as u64, Ordering::SeqCst);
            }
            let ket = nhan_khung(&s)?;
            drop(s);
            let tg = t.t0.elapsed().as_secs_f64();
            let ok = ket["sha256_ok"] == true;
            *t.sha_ok.lock().unwrap() = Some(ok);
            t.dat(XONG);
            println!(
                "[client] XONG upload {}: {} trong {tg:.1}s ({}/s), server kiểm SHA-256 {}",
                t.ten,
                dinh_dang_byte(total),
                dinh_dang_byte((total as f64 / tg.max(0.001)) as u64),
                if ok { "KHỚP" } else { "SAI" }
            );
            Ok(())
        })();
        if let Err(e) = r {
            if !t.dung_khi() {
                t.dat(LOI);
                println!("[client] {}: lỗi kết nối {e}", t.ten);
            }
        }
    }
}

fn thanh_tien_do(pct: f64) -> String {
    let rong = 20;
    let day = ((pct / 100.0 * rong as f64) as usize).min(rong);
    format!("[{}{}]", "#".repeat(day), "-".repeat(rong - day))
}

// ----------------------------- Vai: giao diện dòng lệnh -----------------------------
pub fn chay_giao_dien(host: &str, port: u16, thu_muc: &str) {
    let ql = QuanLy::new(host, port, Path::new(thu_muc), Path::new("kho_server"));
    println!("[client] Nhập lệnh (start/pause/resume/stop/speed/status/quit).");
    for dong in io::stdin().lock().lines().map_while(Result::ok) {
        let p: Vec<&str> = dong.split_whitespace().collect();
        let Some(lenh) = p.first().map(|l| l.to_lowercase()) else { continue };
        let thieu = || println!("[client] Thiếu tham số");
        let toc_do = |s: &str| doc_so_byte(s).unwrap_or_else(|| {
            println!("[client] Tốc độ sai: {s}");
            KHONG_GIOI_HAN
        });
        match (lenh.as_str(), p.get(1)) {
            ("start", Some(ten)) => {
                let huong = p.get(2).copied().unwrap_or("download");
                let rate = p.get(3).map(|r| toc_do(r)).unwrap_or(KHONG_GIOI_HAN);
                ql.bat_dau(ten, huong, rate);
            }
            ("pause", Some(ten)) => ql.tam_dung(ten),
            ("resume", Some(ten)) => ql.tai_tiep(ten),
            ("stop", Some(ten)) => ql.dung_han(ten),
            ("speed", Some(r)) => ql.dat_toc_do_phien(toc_do(r)),
            ("status", _) => ql.trang_thai(),
            ("quit" | "exit", _) => break,
            ("start" | "pause" | "resume" | "stop" | "speed", None) => thieu(),
            _ => println!("[client] Lệnh lạ"),
        }
    }
}

// ----------------------------- Vai: demo tự động -----------------------------
fn cho_den(dieu_kien: impl Fn() -> bool, han: f64) -> bool {
    let het = Instant::now() + Duration::from_secs_f64(han);
    while Instant::now() < het {
        if dieu_kien() {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

fn sleep(s: f64) {
    thread::sleep(Duration::from_secs_f64(s));
}

pub fn chay_demo(host: &str, port: u16) {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let goc = std::env::temp_dir().join(format!("mr51_{}_{nanos:x}", std::process::id()));
    let (kho, tai, nguon) = (goc.join("kho_server"), goc.join("tai_ve"), goc.join("nguon"));
    for d in [&kho, &tai, &nguon] {
        fs::create_dir_all(d).expect("tạo thư mục tạm");
    }

    // Server cục bộ trong một luồng (cùng mã với vai server).
    let srv = TcpListener::bind((host, port)).unwrap_or_else(|e| {
        eprintln!("[demo] Không mở được {host}:{port}: {e}");
        std::process::exit(1)
    });
    let kho2 = kho.clone();
    thread::spawn(move || {
        for conn in srv.incoming().flatten() {
            let k = kho2.clone();
            thread::spawn(move || phuc_vu(conn, k));
        }
    });
    println!("[demo] Server cục bộ tại {host}:{port}, thư mục tạm {}", goc.display());

    // Tạo 3 tệp thử (3 MiB mỗi tệp) trực tiếp trong kho server để tải xuống.
    let cac_tep = ["tep1.bin", "tep2.bin", "tep3.bin"];
    for ten in cac_tep {
        let mut data = vec![0u8; 3 * 1024 * 1024];
        getrandom::fill(&mut data).expect("getrandom");
        fs::write(kho.join(ten), &data).expect("ghi tệp thử");
    }
    println!("[demo] Đã tạo {} tệp thử, mỗi tệp 3 MiB", cac_tep.len());

    let ql = QuanLy::new(host, port, &tai, &kho);

    // Luồng in tiến trình định kỳ (tốc độ = byte tăng thêm / thời gian kể từ lần in trước).
    let dang_chay = Arc::new(AtomicBool::new(true));
    {
        let (ql, dang_chay) = (ql.clone(), Arc::clone(&dang_chay));
        thread::spawn(move || {
            let mut truoc: HashMap<String, (usize, u64, Instant)> = HashMap::new(); // tên -> (lượt, byte, lúc)
            while dang_chay.load(Ordering::SeqCst) {
                sleep(0.8);
                let ds: Vec<(String, Arc<LuotTruyen>)> = ql.luot.lock().unwrap().clone();
                for (ten, t) in ds {
                    if t.tt() != CHAY || t.tong() == 0 {
                        continue;
                    }
                    let id = Arc::as_ptr(&t) as usize;
                    let now = Instant::now();
                    // lượt mới (chạy lại hoặc tải tiếp): mốc là offset và thời điểm bắt đầu của lượt đó
                    let (_, b0, t0) = truoc.get(&ten).copied().filter(|e| e.0 == id).unwrap_or((id, t.off_dau.load(Ordering::SeqCst), t.t0));
                    let x = t.xong();
                    let toc = x.saturating_sub(b0) as f64 / (now - t0).as_secs_f64().max(0.001);
                    truoc.insert(ten.clone(), (id, x, now));
                    let pct = 100.0 * x as f64 / t.tong() as f64;
                    println!("[tiến trình] {ten} {} {pct:5.1}%  {}/s", thanh_tien_do(pct), dinh_dang_byte(toc as u64));
                }
            }
        });
    }

    println!("\n===== KỊCH BẢN: tải xuống 3 tệp đồng thời, giới hạn phiên 2 MiB/s =====");
    ql.dat_toc_do_phien(doc_so_byte("2M").unwrap());
    for ten in cac_tep {
        ql.bat_dau(ten, "download", KHONG_GIOI_HAN);
    }

    sleep(1.2);
    ql.tam_dung("tep2.bin"); // tạm dừng giữa chừng
    sleep(0.8);
    ql.dung_han("tep3.bin"); // dừng hẳn, xóa .part
    sleep(0.3);
    println!("\n===== Nâng tốc độ phiên lên 8 MiB/s, tải tiếp tep2, chạy lại tep3 =====");
    ql.dat_toc_do_phien(doc_so_byte("8M").unwrap());
    ql.tai_tiep("tep2.bin"); // tải tiếp từ offset
    ql.bat_dau("tep3.bin", "download", KHONG_GIOI_HAN); // chạy lại từ đầu

    cho_den(|| cac_tep.iter().all(|t| ql.lay(t).is_some_and(|l| matches!(l.tt(), XONG | LOI))), 30.0);
    ql.trang_thai();

    println!("\n===== KỊCH BẢN upload: đẩy tep1 (đã tải) lên server, giới hạn tệp 1 MiB/s =====");
    // Dùng tệp vừa tải về làm nguồn upload; đặt tên đích khác để không đè tệp gốc.
    let mut ql2 = QuanLy::new(host, port, &tai, &kho);
    ql2.session = Arc::clone(&ql.session);
    let dich = nguon.join("tep1_tai_len.bin");
    fs::copy(tai.join("tep1.bin"), &dich).expect("chép tệp nguồn");
    let t = Arc::new(LuotTruyen::new("tep1_tai_len.bin", "upload", dich, doc_so_byte("1M").unwrap()));
    ql2.chay(Arc::clone(&t));
    println!("[client] BẮT ĐẦU upload tep1_tai_len.bin, giới hạn tệp 1.00 MiB/s");
    cho_den(|| matches!(t.tt(), XONG | LOI), 20.0);

    dang_chay.store(false, Ordering::SeqCst);
    sleep(0.2);
    println!("\n===== TỔNG KẾT =====");
    let khop = |l: &LuotTruyen| if *l.sha_ok.lock().unwrap() == Some(true) { "KHỚP" } else { "—" };
    for ten in cac_tep {
        let l = ql.lay(ten).unwrap();
        println!("[demo] {ten}: {}, {}, SHA-256 {}", l.tt(), dinh_dang_byte(l.xong()), khop(&l));
    }
    println!("[demo] tep1_tai_len.bin: {}, server kiểm SHA-256 {}", t.tt(), khop(&t));
    let mut con: Vec<String> = fs::read_dir(&kho).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    con.sort();
    let con: Vec<String> = con.iter().map(|n| format!("'{n}'")).collect();
    println!("[demo] Server còn giữ: [{}]", con.join(", "));

    let _ = fs::remove_dir_all(&goc);
    println!("[demo] Đã dọn thư mục tạm {}", goc.display());
}
