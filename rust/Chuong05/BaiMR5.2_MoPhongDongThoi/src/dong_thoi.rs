//! Ba khối việc và các mô hình đồng thời:
//!   mang  – 20 yêu cầu mạng tới máy chủ thử (mỗi yêu cầu trễ 200 ms): việc chờ I/O;
//!   cpu   – 8 tác vụ tính toán (mỗi tác vụ ≈ 100 ms): việc nặng CPU;
//!   tron  – 20 tác vụ = yêu cầu mạng 200 ms + xử lý CPU ≈ 30 ms: bất đồng bộ + nhóm thực thi.
//! Mỗi mô hình in biểu đồ Gantt (tác vụ nào chạy trên worker nào, khi nào) và tổng thời gian.

use crate::dong_ho::{axis, gantt, table, Names, Recorder};
use crate::nhom_luong::NhomLuong;
use std::hint::black_box;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

const N_NET: usize = 20;
const N_CPU: usize = 8;
const PAUSE: Duration = Duration::from_millis(500); // nghỉ giữa hai mô hình để máy chủ thử báo từng "đợt"

/// Kết quả của việc CPU được cộng vào đây để trình biên dịch không bỏ vòng lặp.
static SINK: AtomicU64 = AtomicU64::new(0);

// ---------------- việc mạng (chặn và bất đồng bộ) và việc CPU ----------------
/// Một yêu cầu mạng kiểu chặn (blocking): kết nối, gửi GET, chờ dòng trả lời.
fn fetch_blocking(host: &str, port: u16, k: usize) -> String {
    let mut s = TcpStream::connect((host, port)).expect("không kết nối được máy chủ thử");
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    s.write_all(format!("GET {k}\n").as_bytes()).unwrap();
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).unwrap();
    line.trim().to_string()
}

/// Cùng yêu cầu nhưng bất đồng bộ: trong lúc chờ, tokio chạy tác vụ khác trên cùng luồng.
async fn fetch_async(host: &str, port: u16, k: usize) -> String {
    let s = tokio::net::TcpStream::connect((host, port)).await.expect("không kết nối được máy chủ thử");
    let (r, mut w) = s.into_split();
    w.write_all(format!("GET {k}\n").as_bytes()).await.unwrap();
    let mut line = String::new();
    tokio::io::BufReader::new(r).read_line(&mut line).await.unwrap();
    line.trim().to_string()
}

/// Việc tính toán (bộ sinh đồng dư tuyến tính, cùng công thức với bản Python và Go).
fn cpu_work(n: u64) -> u64 {
    let mut x: u64 = 1;
    for _ in 0..black_box(n) {
        x = (x.wrapping_mul(1103515245) + 12345) & 0x7FFF_FFFF;
    }
    SINK.fetch_add(x, Ordering::Relaxed);
    x
}

/// Tìm số vòng lặp để cpu_work chạy ≈ ms mili giây trên máy này.
fn calibrate(ms: u64) -> u64 {
    let n = 50_000_000u64;
    let mut dt = f64::MAX;
    for _ in 0..3 {
        // đo 3 lần, lấy lần nhanh nhất
        let t = Instant::now();
        cpu_work(n);
        dt = dt.min(t.elapsed().as_secs_f64());
    }
    (n as f64 * ms as f64 / 1000.0 / dt) as u64
}

fn ncpu() -> usize {
    thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

fn show(results: &[(String, Arc<Recorder>, &str)]) {
    let scale = results.iter().map(|r| r.1.total()).fold(0.0, f64::max);
    println!();
    for (name, rec, kind) in results {
        gantt(name, rec, scale, kind);
    }
    axis(scale);
    println!();
    let rows: Vec<(String, f64)> = results.iter().map(|(n, r, _)| (n.clone(), r.total())).collect();
    table(&rows);
    println!();
}

// ---------------- khối việc 1: mạng ----------------
pub fn run_net(host: &str, port: u16) {
    println!("Khối việc: {N_NET} yêu cầu mạng, mỗi yêu cầu máy chủ trễ 200 ms");
    let mut results: Vec<(String, Arc<Recorder>, &str)> = Vec::new();
    let host = Arc::new(host.to_string());

    // một tác vụ chặn có ghi thời gian, chạy trên luồng hiện tại
    let timed = move |rec: &Recorder, names: &Names, k: usize, host: &str| {
        let w = names.current(); // đặt tên worker lúc bắt đầu tác vụ
        let t = Instant::now();
        fetch_blocking(host, port, k);
        rec.add(k, &w, t, Instant::now());
    };

    // 1. Tuần tự (đồng bộ)
    let rec = Arc::new(Recorder::new());
    for k in 0..N_NET {
        let t = Instant::now();
        fetch_blocking(&host, port, k);
        rec.add(k, "chính", t, Instant::now());
    }
    results.push(("1. tuần tự".into(), rec, "luồng"));
    thread::sleep(PAUSE);

    // 2. Mỗi tác vụ một luồng hệ điều hành (std::thread::scope: luồng được mượn biến cục bộ)
    let (rec, names) = (Arc::new(Recorder::new()), Names::new("luồng"));
    thread::scope(|sc| {
        for k in 0..N_NET {
            let (rec, names, host) = (&rec, &names, &host);
            sc.spawn(move || timed(rec, names, k, host));
        }
    });
    results.push(("2. mỗi tác vụ một luồng".into(), rec, "luồng"));
    thread::sleep(PAUSE);

    // 3. Nhóm luồng tự viết (5 luồng, hàng đợi việc là kênh mpsc)
    let (rec, names) = (Arc::new(Recorder::new()), Arc::new(Names::new("luồng")));
    {
        let pool = NhomLuong::new(5);
        for k in 0..N_NET {
            let (rec, names, host) = (Arc::clone(&rec), Arc::clone(&names), Arc::clone(&host));
            pool.execute(move || timed(&rec, &names, k, &host));
        }
    } // drop(pool): chờ mọi việc xong
    results.push(("3. nhóm luồng (5 luồng)".into(), rec, "luồng"));
    thread::sleep(PAUSE);

    // 4. Bất đồng bộ tokio: runtime một luồng (current_thread), 20 tác vụ async
    let rec = Arc::new(Recorder::new());
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let mut set = tokio::task::JoinSet::new();
        for k in 0..N_NET {
            let (rec, host) = (Arc::clone(&rec), Arc::clone(&host));
            set.spawn(async move {
                let t = Instant::now();
                fetch_async(&host, port, k).await;
                rec.add(k, "vòng lặp", t, Instant::now());
            });
        }
        while set.join_next().await.is_some() {}
    });
    results.push(("4. tokio (1 luồng)".into(), rec, "luồng"));
    show(&results);
    println!("Nhận xét: việc chờ I/O không tốn CPU, nên luồng hay tác vụ async đều");
    println!("  chồng được thời gian chờ; tokio làm được trên một luồng duy nhất (*).");
}

// ---------------- khối việc 2: CPU ----------------
pub fn run_cpu() {
    let n = calibrate(100);
    let ncpu = ncpu();
    println!("Khối việc: {N_CPU} tác vụ CPU, mỗi tác vụ ≈ 100 ms");
    println!("Máy có {ncpu} CPU; Rust không có GIL: luồng chạy song song thật");
    let mut results: Vec<(String, Arc<Recorder>, &str)> = Vec::new();

    let rec = Arc::new(Recorder::new());
    for k in 0..N_CPU {
        let t = Instant::now();
        cpu_work(n);
        rec.add(k, "chính", t, Instant::now());
    }
    results.push(("1. tuần tự".into(), rec, "luồng"));

    let job = move |rec: &Recorder, names: &Names, k: usize| {
        let w = names.current();
        let t = Instant::now();
        cpu_work(n);
        rec.add(k, &w, t, Instant::now());
    };

    let (rec, names) = (Arc::new(Recorder::new()), Names::new("luồng"));
    thread::scope(|sc| {
        for k in 0..N_CPU {
            let (rec, names) = (&rec, &names);
            sc.spawn(move || job(rec, names, k));
        }
    });
    results.push((format!("2. mỗi tác vụ một luồng ({N_CPU})"), rec, "luồng"));

    let (rec, names) = (Arc::new(Recorder::new()), Arc::new(Names::new("luồng")));
    {
        let pool = NhomLuong::new(ncpu);
        for k in 0..N_CPU {
            let (rec, names) = (Arc::clone(&rec), Arc::clone(&names));
            pool.execute(move || job(&rec, &names, k));
        }
    }
    results.push((format!("3. nhóm luồng ({ncpu} = số CPU)"), rec, "luồng"));
    show(&results);
    println!("Nhận xét: không có GIL, các luồng chạy song song thật trên mọi CPU");
    println!("  (khác nhóm luồng Python); mô hình 2 tạo {N_CPU} luồng nhưng chỉ {ncpu} luồng");
    println!("  chạy cùng lúc (mỗi tác vụ bị kéo dài vì chia lượt); nhóm luồng bằng");
    println!("  số CPU cho cùng tổng thời gian mà mỗi tác vụ chạy liền một mạch.");
}

// ---------------- khối việc 3: mạng + CPU ----------------
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Inline,        // A: CPU ngay trong tác vụ async (runtime 1 luồng)
    SpawnBlocking, // B: tokio::task::spawn_blocking
    Pool,          // C: nhóm luồng tự viết 2 luồng + oneshot
    MultiThread,   // D: runtime đa luồng, CPU ngay trong tác vụ
}

/// Chạy một kịch bản; trả về độ trễ lớn nhất của tác vụ "nhịp tim" (giây).
async fn scenario(mode: Mode, rec: Arc<Recorder>, host: Arc<String>, port: u16, n: u64, pool: Option<Arc<NhomLuong>>) -> f64 {
    let stop = Arc::new(AtomicBool::new(false));
    let names = Arc::new(Names::new(if mode == Mode::MultiThread { "worker" } else { "luồng" }));
    let net_row = if mode == Mode::MultiThread { "chờ mạng" } else { "vòng lặp" };
    let hb = {
        let stop = Arc::clone(&stop);
        tokio::spawn(async move {
            let mut lag: f64 = 0.0;
            while !stop.load(Ordering::SeqCst) {
                let t = Instant::now();
                tokio::time::sleep(Duration::from_millis(10)).await;
                lag = lag.max(t.elapsed().as_secs_f64() - 0.01);
            }
            lag
        })
    };
    let mut set = tokio::task::JoinSet::new();
    for k in 0..N_NET {
        let (rec, host, names, pool) = (Arc::clone(&rec), Arc::clone(&host), Arc::clone(&names), pool.clone());
        set.spawn(async move {
            let t = Instant::now();
            fetch_async(&host, port, k).await;
            rec.add(k, net_row, t, Instant::now());
            let job = {
                let rec = Arc::clone(&rec);
                move || {
                    let w = names.current();
                    let t1 = Instant::now();
                    cpu_work(n);
                    rec.add(k, &w, t1, Instant::now());
                }
            };
            match mode {
                Mode::Inline => {
                    // CPU chạy ngay trong vòng lặp: chặn mọi tác vụ khác của runtime
                    let t = Instant::now();
                    cpu_work(n);
                    rec.add(k, "vòng lặp", t, Instant::now());
                }
                Mode::MultiThread => job(), // chặn một worker của runtime đa luồng
                Mode::SpawnBlocking => tokio::task::spawn_blocking(job).await.unwrap(),
                Mode::Pool => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    pool.unwrap().execute(move || {
                        job();
                        let _ = tx.send(());
                    });
                    rx.await.unwrap(); // chờ không chặn: vòng lặp vẫn chạy tác vụ khác
                }
            }
        });
    }
    while set.join_next().await.is_some() {}
    stop.store(true, Ordering::SeqCst);
    hb.await.unwrap()
}

pub fn run_mixed(host: &str, port: u16) {
    let n = calibrate(30);
    let ncpu = ncpu();
    let host = Arc::new(host.to_string());
    println!("Khối việc: {N_NET} tác vụ = yêu cầu mạng 200 ms + xử lý CPU ≈ 30 ms");
    println!("Đo thêm độ trễ vòng lặp sự kiện: tác vụ \"nhịp tim\" ngủ 10 ms và");
    println!("  đo xem thực tế bị đánh thức muộn bao nhiêu (trễ lớn = ứng dụng đứng).");
    let one = || tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut results: Vec<(String, Arc<Recorder>, &str)> = Vec::new();
    let mut lags = Vec::new();

    let rec = Arc::new(Recorder::new());
    lags.push(one().block_on(scenario(Mode::Inline, Arc::clone(&rec), Arc::clone(&host), port, n, None)));
    results.push(("A. CPU ngay trong vòng lặp".into(), rec, "luồng"));
    thread::sleep(PAUSE);

    let rec = Arc::new(Recorder::new());
    lags.push(one().block_on(scenario(Mode::SpawnBlocking, Arc::clone(&rec), Arc::clone(&host), port, n, None)));
    results.push(("B. spawn_blocking".into(), rec, "luồng"));
    thread::sleep(PAUSE);

    let rec = Arc::new(Recorder::new());
    let pool = Arc::new(NhomLuong::new(2));
    lags.push(one().block_on(scenario(Mode::Pool, Arc::clone(&rec), Arc::clone(&host), port, n, Some(pool))));
    results.push(("C. nhóm luồng tự viết, 2 luồng".into(), rec, "luồng"));
    thread::sleep(PAUSE);

    let rec = Arc::new(Recorder::new());
    let multi = tokio::runtime::Builder::new_multi_thread().worker_threads(ncpu).enable_all().build().unwrap();
    lags.push(multi.block_on(scenario(Mode::MultiThread, Arc::clone(&rec), Arc::clone(&host), port, n, None)));
    results.push((format!("D. runtime đa luồng ({ncpu} worker)"), rec, "worker"));

    let scale = results.iter().map(|r| r.1.total()).fold(0.0, f64::max);
    println!();
    for (name, rec, kind) in &results {
        gantt(name, rec, scale, kind);
    }
    axis(scale);
    println!();
    println!("  {:<30} {:>9} {:>13}", "Mô hình", "Thời gian", "Trễ vòng lặp");
    for ((name, rec, _), lag) in results.iter().zip(&lags) {
        println!("  {name:<30} {:>7.2} s {:>10.0} ms", rec.total(), lag * 1000.0);
    }
    println!();
    println!("Nhận xét: A – xử lý CPU chặn vòng lặp sự kiện, nhịp tim trễ hàng trăm");
    println!("  ms (giống asyncio của Python). B, C – đẩy CPU sang luồng khác: vòng");
    println!("  lặp thông suốt và, vì Rust không có GIL, CPU chạy song song; C giới");
    println!("  hạn số việc CPU đồng thời bằng nhóm 2 luồng. D – runtime đa luồng:");
    println!("  CPU chạy ngay trong tác vụ vẫn chặn worker đang chạy nó, nhịp tim chỉ");
    println!("  chạy được khi có worker rảnh – nên vẫn dùng spawn_blocking cho việc nặng.");
}
