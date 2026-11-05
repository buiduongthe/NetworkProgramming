//! Server bất đồng bộ (tokio current_thread) phối hợp với nhóm luồng và luồng nền.
//!
//! Các luồng của tiến trình server:
//!   vòng-lặp   – luồng chính chạy runtime tokio MỘT LUỒNG: accept, đọc/ghi socket, nhịp tim 10 ms;
//!   chặn-k     – nhóm luồng chặn của tokio: tokio::task::spawn_blocking(hàm_chặn) cho hàm thư viện cũ;
//!   tính-1, -2 – nhóm std::thread RIÊNG cho việc băm: nhận việc qua std::sync::mpsc, trả kết quả qua
//!                tokio::sync::oneshot (gửi được từ luồng thường, phía async .await để nhận);
//!   cảm-biến   – một std::thread chạy vòng lặp chặn của riêng nó, đẩy mẫu đo VÀO vòng lặp bằng
//!                tokio::sync::mpsc::Sender::blocking_send (= Handle::block_on(tx.send(..)) – chỉ gọi được
//!                từ luồng KHÔNG chạy runtime).
//! Hủy, hạn chót: tokio::time::timeout thả (drop) future đang chờ – đó là cách hủy của async Rust; nhưng
//! luồng đang băm không bị dừng theo, nên bật thêm cờ AtomicBool mà luồng băm kiểm tra mỗi vòng.

use crate::chung::*;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Semaphore};

/// Một việc băm gửi sang nhóm luồng tính.
struct ViecTinh {
    ma: String,
    data: Arc<Vec<u8>>,
    vong: usize,
    huy: Arc<AtomicBool>,
    tra: oneshot::Sender<(String, String)>, // (hex16, tên luồng)
}

/// Thống kê. tokio::spawn đòi kiểu Send nên bọc Arc<Mutex<_>>; thực tế chỉ luồng vòng lặp chạm tới
/// (có thể dùng Rc<RefCell<_>> với tokio::task::LocalSet).
#[derive(Default)]
struct ThongKe {
    tre_nhip: f64,
    so_mau: usize,
    tre_mau: f64,
    o_nhom: usize,
    o_nhom_max: usize,
    so_yc: usize,
    nhan: BTreeSet<String>,
    tra_cuu: BTreeSet<String>,
    bam: BTreeSet<String>,
}

#[derive(Clone)]
struct MayChu {
    tk: Arc<Mutex<ThongKe>>,
    cong_vao: Arc<Semaphore>, // semaphore BẤT ĐỒNG BỘ: acquire().await không chặn vòng lặp
    viec_tinh: std_mpsc::Sender<ViecTinh>,
    yc_cam_bien: std_mpsc::Sender<oneshot::Sender<(u32, String)>>,
}

fn tom_tat(ds: &BTreeSet<String>) -> String {
    match ds.len() {
        0 => "–".into(),
        1..=8 => ds.iter().cloned().collect::<Vec<_>>().join(", "),
        n => format!(
            "{}, … ({n} luồng)",
            ds.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
        ),
    }
}

impl MayChu {
    /// B, C: hàm chặn → spawn_blocking; băm → nhóm luồng tính; vòng lặp chỉ .await
    async fn tach(
        &self,
        ma: &str,
        data: Arc<Vec<u8>>,
        vong: usize,
        huy: Arc<AtomicBool>,
    ) -> (String, String) {
        let ten_cu = tokio::task::spawn_blocking(tra_cuu_cu).await.unwrap();
        self.tk.lock().unwrap().tra_cuu.insert(ten_cu);
        {
            let mut t = self.tk.lock().unwrap();
            t.o_nhom += 1;
            t.o_nhom_max = t.o_nhom_max.max(t.o_nhom);
        }
        let (tx, rx) = oneshot::channel();
        let _ = self.viec_tinh.send(ViecTinh {
            ma: ma.to_string(),
            data,
            vong,
            huy,
            tra: tx,
        });
        rx.await.unwrap_or_default()
    }

    async fn xu_ly_bam(&self, p: &[String], data: Vec<u8>) -> String {
        let (ma, che_do) = (p[1].as_str(), p[2].as_str());
        let vong: usize = p[4].parse().unwrap_or(0);
        let han: u64 = p[5].parse().unwrap_or(0);
        let ghi = matches!(ma, "1" | "101" | "102"); // chỉ ghi dòng thời gian cho vài yêu cầu tiêu biểu
        if ghi {
            log(&format!(
                "BAM {ma} ({che_do}): nhận {} byte, {vong} vòng, hạn {han} ms",
                data.len()
            ));
        }
        self.tk.lock().unwrap().so_yc += 1;
        let (hex, ten) = if che_do == "chan" {
            // A. SAI: gọi hàm chặn và băm ngay trên luồng vòng lặp → mọi tác vụ khác đứng chờ
            let ten_cu = tra_cuu_cu();
            self.tk.lock().unwrap().tra_cuu.insert(ten_cu);
            (
                bam(&data, vong, &AtomicBool::new(false)).unwrap_or_default(),
                ten_luong(),
            )
        } else {
            let _suat = if che_do == "gioihan" {
                Some(self.cong_vao.acquire().await.unwrap()) // C. quá GIOI_HAN thì CHỜ bất đồng bộ ở đây
            } else {
                None
            };
            let huy = Arc::new(AtomicBool::new(false));
            let viec = self.tach(ma, Arc::new(data), vong, Arc::clone(&huy));
            match tokio::time::timeout(Duration::from_millis(han), viec).await {
                Ok(kq) => kq,
                Err(_) => {
                    huy.store(true, Ordering::Relaxed); // hủy xuyên ranh giới: báo luồng băm dừng
                    log(&format!(
                        "BAM {ma}: quá hạn {han} ms → trả HETGIO, bật cờ hủy cho luồng tính"
                    ));
                    return format!("HETGIO {ma}");
                }
            }
        };
        self.tk.lock().unwrap().bam.insert(ten.clone());
        if ghi {
            log(&format!("BAM {ma}: băm xong trên {ten}, trả lời OK"));
        }
        format!("OK {ma} {hex}")
    }

    async fn phuc_vu(self, s: TcpStream) {
        self.tk.lock().unwrap().nhan.insert(ten_luong());
        let (doc, mut ghi) = s.into_split();
        let mut r = BufReader::new(doc);
        let mut line = String::new();
        if r.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
        }
        let p: Vec<String> = line.split_whitespace().map(String::from).collect();
        let tra_loi = match p[0].as_str() {
            "BAM" if p.len() == 6 => {
                let mut data = vec![0u8; p[3].parse().unwrap_or(0)];
                if r.read_exact(&mut data).await.is_err() {
                    return;
                }
                self.xu_ly_bam(&p, data).await
            }
            "CAMBIEN" => {
                let (tx, rx) = oneshot::channel();
                let _ = self.yc_cam_bien.send(tx); // nhờ luồng cảm biến đọc ngay
                let (v, ten) = tokio::time::timeout(Duration::from_secs(1), rx)
                    .await
                    .unwrap()
                    .unwrap();
                log(&format!(
                    "CAMBIEN {}: future được hoàn tất từ luồng {ten}, giá trị {v}",
                    p[1]
                ));
                format!("CAMBIEN {} {v} {ten}", p[1])
            }
            "DATLAI" => {
                *self.tk.lock().unwrap() = ThongKe::default();
                "OK".into()
            }
            _ => {
                let t = self.tk.lock().unwrap();
                say(&format!(
                    "[server] đợt \"{}\": {} yêu cầu",
                    p[1..].join(" "),
                    t.so_yc
                ));
                say(&format!("  {:<15}luồng đã chạy", "giai đoạn"));
                for (gd, ds) in [
                    ("nhận/trả lời", &t.nhan),
                    ("tra cứu cũ", &t.tra_cuu),
                    ("băm SHA-256", &t.bam),
                ] {
                    say(&format!("  {gd:<15}{}", tom_tat(ds)));
                }
                say(&format!(
                    "  trễ nhịp tối đa {:.1} ms; nhóm tính tối đa {} việc",
                    t.tre_nhip, t.o_nhom_max
                ));
                say(&format!(
                    "  cảm biến {} mẫu, trễ giao tối đa {:.1} ms",
                    t.so_mau, t.tre_mau
                ));
                format!(
                    "TK {:.1} {} {:.1} {}",
                    t.tre_nhip, t.so_mau, t.tre_mau, t.o_nhom_max
                )
            }
        };
        let _ = ghi.write_all(format!("{tra_loi}\n").as_bytes()).await;
    }
}

/// Luồng tính: lấy việc từ kênh std (Receiver dùng chung qua Arc<Mutex>), băm, trả kết quả.
fn tho_tinh(
    rx: Arc<Mutex<std_mpsc::Receiver<ViecTinh>>>,
    ra_nhom: mpsc::UnboundedSender<(String, Option<usize>)>,
) {
    loop {
        let Ok(v) = rx.lock().unwrap().recv() else {
            return;
        };
        match bam(&v.data, v.vong, &v.huy) {
            Ok(hex) => {
                let _ = v.tra.send((hex, ten_luong())); // phía async có thể đã bỏ đi (quá hạn) → bỏ qua lỗi
                let _ = ra_nhom.send((v.ma, None));
            }
            Err(r) => {
                let _ = ra_nhom.send((v.ma, Some(r)));
            }
        }
    }
}

/// Luồng cảm biến: luồng HĐH thường, vòng lặp chặn riêng.
fn cam_bien(
    mau: mpsc::Sender<(u32, Instant)>,
    yc: std_mpsc::Receiver<oneshot::Sender<(u32, String)>>,
) {
    let mut k = 0u32;
    loop {
        match yc.recv_timeout(Duration::from_millis(CAM_BIEN_MS)) {
            Ok(tra) => {
                k += 1;
                let _ = tra.send(((k * 37) % 100, ten_luong())); // oneshot gửi được từ luồng thường
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                k += 1;
                let _ = mau.blocking_send((k, Instant::now())); // luồng thường → kênh tokio
            }
            Err(_) => return,
        }
    }
}

pub fn run(host: &str, port: u16) {
    // Vòng lặp sự kiện một luồng; nhóm luồng chặn tối đa 8 luồng, đặt tên chặn-1, chặn-2, ...
    let dem = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(8)
        .thread_name_fn(move || format!("chặn-{}", dem.fetch_add(1, Ordering::Relaxed) + 1))
        .build()
        .unwrap();
    rt.block_on(async move {
        let ln = TcpListener::bind((host, port)).await.expect("không mở được cổng");
        let (tx_viec, rx_viec) = std_mpsc::channel::<ViecTinh>();
        let (tx_ra, mut rx_ra) = mpsc::unbounded_channel();
        let rx_viec = Arc::new(Mutex::new(rx_viec));
        for i in 1..=NHOM_TINH {
            let (rx, ra) = (Arc::clone(&rx_viec), tx_ra.clone());
            thread::Builder::new().name(format!("tính-{i}")).spawn(move || tho_tinh(rx, ra)).unwrap();
        }
        let (tx_mau, mut rx_mau) = mpsc::channel(256);
        let (tx_yc, rx_yc) = std_mpsc::channel();
        thread::Builder::new().name("cảm-biến".into()).spawn(move || cam_bien(tx_mau, rx_yc)).unwrap();
        let mc = MayChu {
            tk: Arc::new(Mutex::new(ThongKe::default())),
            cong_vao: Arc::new(Semaphore::new(GIOI_HAN)),
            viec_tinh: tx_viec,
            yc_cam_bien: tx_yc,
        };
        // nhịp tim: đo độ trễ của vòng lặp sự kiện
        let tk = Arc::clone(&mc.tk);
        tokio::spawn(async move {
            loop {
                let t = Instant::now();
                tokio::time::sleep(Duration::from_millis(NHIP_MS as u64)).await;
                let tre = t.elapsed().as_secs_f64() * 1000.0 - NHIP_MS;
                let mut g = tk.lock().unwrap();
                g.tre_nhip = g.tre_nhip.max(tre);
            }
        });
        // nhận mẫu cảm biến (luồng → async)
        let tk = Arc::clone(&mc.tk);
        tokio::spawn(async move {
            while let Some((_, t_tao)) = rx_mau.recv().await {
                let mut g = tk.lock().unwrap();
                g.so_mau += 1;
                g.tre_mau = g.tre_mau.max(t_tao.elapsed().as_secs_f64() * 1000.0);
            }
        });
        // luồng tính báo đã xong THẬT (kể cả khi bị hủy) → giảm bộ đếm trên vòng lặp
        let tk = Arc::clone(&mc.tk);
        tokio::spawn(async move {
            while let Some((ma, dung_o)) = rx_ra.recv().await {
                tk.lock().unwrap().o_nhom -= 1;
                if let Some(r) = dung_o {
                    say(&format!("[server] BAM {ma}: luồng tính thấy cờ hủy → dừng ở vòng {r}"));
                }
            }
        });
        say(&format!("[server] lắng nghe {host}:{port}; vòng lặp sự kiện trên luồng \"vòng-lặp\", nhóm tính {NHOM_TINH} luồng,"));
        say(&format!("  nhịp tim {NHIP_MS} ms, luồng cảm biến mỗi {CAM_BIEN_MS} ms, chế độ C giới hạn {GIOI_HAN} yêu cầu"));
        say("Đặc thù Rust: tokio current_thread là vòng lặp một luồng; spawn_blocking cho hàm chặn; việc CPU sang");
        say("  nhóm std::thread riêng (không chiếm luồng chặn); hủy = thả future, luồng đang chạy phải tự xem cờ.");
        while let Ok((s, _)) = ln.accept().await {
            tokio::spawn(mc.clone().phuc_vu(s));
        }
    });
}
