//! Bài mở rộng MR6.11 – Bộ mô phỏng tải: n người dùng ảo trên k kết nối, rào xuất phát, pipelining.
//!
//! Tệp này là phần KHUNG, GIỐNG NHAU ở 9 bài tranh chấp; phần NGHIỆP VỤ (NguCanh, NguoiAo,
//! SO_LAN_MAC_DINH, tham_so_dat_lai) ở nguoi_ao.rs.
//!
//! Một lượt: kết nối k socket (người i đi trên kết nối i mod k; kết nối chào kèm "so_nguoi"), mọi luồng
//! đứng chờ ở RÀO XUẤT PHÁT -> cùng lúc mỗi kết nối gửi yêu cầu đầu của mọi người mình chở trong MỘT lần
//! ghi; trả lời tới đâu gửi yêu cầu kế tiếp của người đó tới đó. Đo thời gian tổng, yc/s, khứ hồi
//! p50/p95/p99, rồi hỏi thong_ke của máy chủ (sai phạm theo sổ kiểm toán).
//!
//! Chạy:  mo_phong <host> <cổng> [--n 200] [--k 200] [--lan 5] [--che-do khoa] [--dat-lai]
//!                               [--so-sanh] [--tre-ms 2]

use crate::chung::*;
use crate::khach::{KetNoiKhach, KhiKq, QuanTri};
use crate::nguoi_ao::{tham_so_dat_lai, NguCanh, NguoiAo, SO_LAN_MAC_DINH};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

/// Rào xuất phát dùng một lần, có hạn chờ và có thể bị "phá" (như threading.Barrier của Python:
/// một kết nối lỗi trước khi bắn thì phá rào để luồng chính biết).
pub struct Rao {
    k: Mutex<(usize, bool)>, // (số bên đã tới, đã vỡ)
    cv: Condvar,
    n: usize,
}

impl Rao {
    pub fn new(n: usize) -> Rao {
        Rao { k: Mutex::new((0, false)), cv: Condvar::new(), n }
    }

    pub fn cho(&self, han: Option<Duration>) -> Result<(), String> {
        let het = han.map(|h| Instant::now() + h);
        let mut g = self.k.lock().unwrap();
        if g.1 {
            return Err("rào đã bị phá".into());
        }
        g.0 += 1;
        if g.0 >= self.n {
            self.cv.notify_all();
            return Ok(());
        }
        loop {
            if g.1 {
                return Err("rào đã bị phá".into());
            }
            if g.0 >= self.n {
                return Ok(());
            }
            match het {
                Some(h) => {
                    let con = h.saturating_duration_since(Instant::now());
                    if con.is_zero() {
                        g.1 = true;
                        self.cv.notify_all();
                        return Err("hết thời gian chờ".into());
                    }
                    g = self.cv.wait_timeout(g, con).unwrap().0;
                }
                None => g = self.cv.wait(g).unwrap(),
            }
        }
    }

    pub fn pha(&self) {
        self.k.lock().unwrap().1 = true;
        self.cv.notify_all();
    }
}

#[derive(Default)]
struct KetQuaKetNoi {
    rtt: Vec<f64>,
    ok: u64,
    tu_choi: u64,
    ly_do: Vec<(String, u64)>,
    loi: Option<String>,
    t_xong: Option<Instant>,
}

/// Những gì các hàm gọi lại của MỘT kết nối dùng chung (cùng chạy trên luồng đọc của kết nối đó).
struct BoiCanh {
    kn: Weak<KetNoiKhach>,
    nc: Arc<Mutex<NguCanh>>,
    nguoi: Mutex<Vec<NguoiAo>>,
    con: AtomicUsize, // số người chưa xong
    kq: Arc<Mutex<KetQuaKetNoi>>,
}

/// Yêu cầu kế tiếp của người thứ i (None: người đó xong) -> mục gửi (loai, truong, khi_kq).
fn gui(bc: &Arc<BoiCanh>, i: usize, yc: Option<Obj>) -> Option<(String, Obj, Option<KhiKq>)> {
    let Some(mut yc) = yc else {
        bc.con.fetch_sub(1, Ordering::SeqCst);
        return None;
    };
    let loai = yc.remove("loai").map(|v| py_str(&v)).unwrap_or_default();
    let b = Arc::clone(bc);
    Some((loai, yc, Some(Box::new(move |tin: &Obj, rtt: f64| nhan(&b, i, tin, rtt)))))
}

/// Có trả lời cho người thứ i: ghi số liệu, cập nhật ngữ cảnh, gửi tiếp yêu cầu kế tiếp của người đó.
fn nhan(bc: &Arc<BoiCanh>, i: usize, tin: &Obj, rtt: f64) {
    {
        let mut kq = bc.kq.lock().unwrap();
        kq.rtt.push(rtt);
        if dung(tin, "ok") {
            kq.ok += 1;
        } else {
            kq.tu_choi += 1;
            let ma = lay_str(tin, "ma_loi").unwrap_or("khac").to_string();
            match kq.ly_do.iter_mut().find(|(m, _)| *m == ma) {
                Some(x) => x.1 += 1,
                None => kq.ly_do.push((ma, 1)),
            }
        }
    }
    let tiep = {
        let mut nc = bc.nc.lock().unwrap();
        nc.nhan_kq(tin);
        let yc = bc.nguoi.lock().unwrap()[i].sau_kq(&nc, tin);
        yc
    };
    if let Some((loai, truong, khi_kq)) = gui(bc, i, tiep) {
        if let Some(kn) = bc.kn.upgrade() {
            let _ = kn.gui_yc(&loai, truong, khi_kq);
        }
    }
}

/// Luồng của một kết nối: chào -> chờ rào -> gửi lô yêu cầu đầu -> đọc, gửi tiếp tới khi xong.
fn chay_ket_noi(host: String, cong: u16, i: usize, ds_ten: Vec<String>, lan: u32, rao: Arc<Rao>, kq: Arc<Mutex<KetQuaKetNoi>>) {
    let kn = Arc::new(KetNoiKhach::default());
    let r: Result<(), String> = (|| {
        kn.ket_noi(&host, cong, &format!("mp{}", i + 1), "nguoi_dung", Some(ds_ten.len()), 5.0)?;
        let tt = kn.trang_thai().ok_or("thiếu cap_nhat đầu tiên")?;
        let nc = Arc::new(Mutex::new(NguCanh::moi(&tt)));
        let n2 = Arc::clone(&nc);
        kn.dat_khi_cap_nhat(Box::new(move |tin| n2.lock().unwrap().cap_nhat(tin)));
        let bc = Arc::new(BoiCanh {
            kn: Arc::downgrade(&kn),
            nc,
            nguoi: Mutex::new(ds_ten.iter().map(|t| NguoiAo::moi(t, lan)).collect()),
            con: AtomicUsize::new(ds_ten.len()),
            kq: Arc::clone(&kq),
        });
        rao.cho(None)?; // RÀO XUẤT PHÁT: mọi kết nối sẵn sàng mới bắn
        let mut lo = Vec::new();
        for j in 0..ds_ten.len() {
            let yc = {
                let nc = bc.nc.lock().unwrap();
                let yc = bc.nguoi.lock().unwrap()[j].yeu_cau_dau(&nc);
                yc
            };
            if let Some(x) = gui(&bc, j, yc) {
                lo.push(x);
            }
        }
        if !lo.is_empty() {
            kn.gui_lo(lo)?;
        }
        kn.vong_doc(Some(&|| bc.con.load(Ordering::SeqCst) == 0));
        if bc.con.load(Ordering::SeqCst) > 0 {
            kq.lock().unwrap().loi = Some("máy chủ đóng kết nối giữa chừng".into());
        }
        Ok(())
    })();
    if let Err(e) = r {
        // lỗi trước rào: phá rào để luồng chính biết
        kq.lock().unwrap().loi = Some(e);
        rao.pha();
    }
    kq.lock().unwrap().t_xong = Some(Instant::now());
    kn.dong_im();
}

fn so(tk: &Obj, k: &str) -> i64 {
    tk.get(k).and_then(Value::as_i64).unwrap_or(0)
}

/// Một lượt mô phỏng; trả về một hàng của bảng so sánh, hoặc None nếu không mở được đủ kết nối.
#[allow(clippy::too_many_arguments)]
pub fn chay_luot(host: &str, cong: u16, n: usize, k: usize, lan: u32, bao: &dyn Fn(&str), che_do: Option<&str>, dat_lai: bool, tre_ms: Option<f64>) -> Result<Option<HangSoSanh>, String> {
    let k = k.min(n).max(1);
    let qt = QuanTri::new(host, cong, "mo_phong")?;
    let kq = (|| {
        if let Some(t) = tre_ms {
            qt.tre(t)?;
        }
        if dat_lai {
            // đặt lại trước (máy chủ in tổng kết lượt trước đúng chế độ)
            qt.dat_lai(tham_so_dat_lai())?;
        }
        if let Some(cd) = che_do {
            qt.che_do(cd)?;
        }
        let che_do = lay_str(&qt.thong_ke()?, "che_do").unwrap_or("?").to_string();
        bao(&format!("=== {che_do}: {n} người × {lan} lần, {k} kết nối ==="));
        let mut nhom: Vec<Vec<String>> = vec![Vec::new(); k];
        for i in 0..n {
            nhom[i % k].push(format!("u{}", i + 1));
        }
        let rao = Arc::new(Rao::new(k + 1));
        let ket_qua: Vec<Arc<Mutex<KetQuaKetNoi>>> = (0..k).map(|_| Arc::new(Mutex::new(KetQuaKetNoi::default()))).collect();
        let luong: Vec<_> = nhom
            .into_iter()
            .enumerate()
            .map(|(i, ds)| {
                let (h, r, kq) = (host.to_string(), Arc::clone(&rao), Arc::clone(&ket_qua[i]));
                thread::Builder::new().stack_size(256 * 1024).spawn(move || chay_ket_noi(h, cong, i, ds, lan, r, kq)).expect("không tạo được luồng")
            })
            .collect();
        if rao.cho(Some(Duration::from_secs(60))).is_err() {
            let loi = ket_qua.iter().find_map(|r| r.lock().unwrap().loi.clone()).unwrap_or_else(|| "hết thời gian chờ".into());
            bao(&format!("Không mở được đủ {k} kết nối: {loi}"));
            return Ok(None);
        }
        let t0 = Instant::now();
        bao(&format!("Rào xuất phát: {k}/{k} kết nối sẵn sàng -> bắn"));
        for t in luong {
            let _ = t.join();
        }
        let kqs: Vec<KetQuaKetNoi> = ket_qua.iter().map(|r| std::mem::take(&mut *r.lock().unwrap())).collect();
        let t1 = kqs.iter().filter_map(|r| r.t_xong).max().unwrap_or(t0);
        let loi: Vec<&String> = kqs.iter().filter_map(|r| r.loi.as_ref()).collect();
        if let Some(l) = loi.first() {
            bao(&format!("{} kết nối lỗi: {l}", loi.len()));
        }
        let mut rtt: Vec<f64> = kqs.iter().flat_map(|r| r.rtt.iter().copied()).collect();
        rtt.sort_by(f64::total_cmp);
        let ok: u64 = kqs.iter().map(|r| r.ok).sum();
        let tu_choi: u64 = kqs.iter().map(|r| r.tu_choi).sum();
        let mut ly_do: Vec<(String, u64)> = Vec::new();
        for r in &kqs {
            for (m, s) in &r.ly_do {
                match ly_do.iter_mut().find(|(x, _)| x == m) {
                    Some(x) => x.1 += s,
                    None => ly_do.push((m.clone(), *s)),
                }
            }
        }
        ly_do.sort_by(|a, b| b.1.cmp(&a.1)); // most_common(): giảm dần, ổn định
        let tong = ok + tu_choi;
        let giay = t1.saturating_duration_since(t0).as_secs_f64().max(1e-6);
        let yc_s = round_py(tong as f64 / giay);
        let p: Vec<(u32, Option<f64>)> = [50, 95, 99].iter().map(|&x| (x, phan_vi(&rtt, x as f64))).collect();
        bao(&format!("{tong} yc xong trong {giay:.2} s = {yc_s} yc/s"));
        bao(&format!("Khứ hồi ms: {}", p.iter().map(|(x, v)| format!("p{x} {}", chu_p99(*v))).collect::<Vec<_>>().join(" ")));
        bao(&format!("Thành công {ok}, từ chối {tu_choi}"));
        if !ly_do.is_empty() {
            bao(&format!("  lý do: {}", ly_do.iter().map(|(m, s)| format!("{m} {s}")).collect::<Vec<_>>().join(", ")));
        }
        let tk = qt.thong_ke()?;
        bao(&format!("Máy chủ: xung đột {}, thử lại {}, hàng đợi max {}", so_us(tk.get("xung_dot")), so_us(tk.get("thu_lai")), so_us(tk.get("hang_doi_max"))));
        bao(&format!("Máy chủ xử lý µs: p50 {} p95 {} p99 {}", so_us(tk.get("p50_us")), so_us(tk.get("p95_us")), so_us(tk.get("p99_us"))));
        bao(&dong_sai_pham(&tk));
        for v in tk.get("vi_du_sai_pham").and_then(Value::as_array).into_iter().flatten().take(2) {
            bao(&format!("  ví dụ: {}", py_str(v)));
        }
        let tom_tat = lay_str(&tk, "tom_tat").unwrap_or("").to_string();
        bao(&tom_tat);
        Ok(Some(HangSoSanh { che_do, thanh_cong: ok, sai_pham: so(&tk, "sai_pham_tong"), xung_dot: so(&tk, "xung_dot"), yc_s, p99_ms: p[2].1, tu_choi, tom_tat }))
    })();
    qt.dong();
    kq
}

/// Chạy lần lượt 4 chế độ (đặt lại trạng thái trước mỗi lần); trả về các hàng; trả lại chế độ cũ.
pub fn so_sanh(host: &str, cong: u16, n: usize, k: usize, lan: u32, bao: &dyn Fn(&str), tre_ms: Option<f64>) -> Result<Vec<HangSoSanh>, String> {
    let qt = QuanTri::new(host, cong, "so_sanh")?;
    let kq = (|| {
        let goc = lay_str(&qt.thong_ke()?, "che_do").unwrap_or("khoa").to_string();
        let mut hang = Vec::new();
        for cd in CHE_DO {
            match chay_luot(host, cong, n, k, lan, bao, Some(cd), true, tre_ms)? {
                Some(h) => hang.push(h),
                None => break,
            }
        }
        qt.che_do(&goc)?;
        Ok(hang)
    })();
    qt.dong();
    kq
}

pub fn run(args: &[String]) {
    let a = Args::parse(args, &["dat-lai", "so-sanh"]);
    let (host, cong) = a.host_cong(crate::may_chu::CONG);
    let n = a.i64("n", 200).max(1) as usize;
    let k = match a.i64("k", 0) {
        x if x > 0 => x as usize,
        _ => n.min(200),
    };
    let lan = a.i64("lan", SO_LAN_MAC_DINH as i64).max(1) as u32;
    let tre_ms = a.opt.get("tre-ms").and_then(|v| v.parse().ok());
    let bao = |text: &str| in_dong("mo_phong", text);
    let kq = if a.co("so-sanh") {
        so_sanh(&host, cong, n, k, lan, &bao, tre_ms).map(|hang| {
            bao(&format!("Bảng so sánh ({n} người × {lan} lần, {} kết nối):", k.min(n)));
            for d in bang_so_sanh(&hang) {
                in_tho(&d);
            }
        })
    } else {
        let che_do = a.opt.get("che-do").cloned();
        chay_luot(&host, cong, n, k, lan, &bao, che_do.as_deref(), a.co("dat-lai"), tre_ms).map(|_| ())
    };
    if let Err(e) = kq {
        bao(&format!("lỗi: {e}"));
        std::process::exit(1);
    }
}
