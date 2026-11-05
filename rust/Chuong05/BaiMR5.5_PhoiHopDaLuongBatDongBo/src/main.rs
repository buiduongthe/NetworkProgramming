//! Bài mở rộng MR5.5 – Phối hợp đa luồng và bất đồng bộ trong server mạng (Rust, tokio).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1–5.6), Chương 6 (Bài 6.1, 6.2), MR5.4.
//! Server tokio một luồng phải:
//!   (a) đẩy việc CPU (băm SHA-256) sang nhóm std::thread riêng mà không chặn vòng lặp;
//!   (b) gọi hàm thư viện cũ kiểu CHẶN an toàn (tokio::task::spawn_blocking);
//!   (c) nhận kết quả từ luồng nền (cảm biến): mpsc::Sender::blocking_send, oneshot từ luồng thường;
//!   (d) truyền hủy, hạn chót qua ranh giới (tokio::time::timeout + cờ AtomicBool);
//!   (e) áp lực ngược giữa vòng lặp và nhóm luồng (tokio::sync::Semaphore);
//!   (f) cách làm SAI – việc CPU ngay trên vòng lặp – đo bằng nhịp tim 10 ms.
//!
//! Chạy:  cargo run -q --release -- server <host> <port>
//!        cargo run -q --release -- tai    <host> <port>

mod chung;
mod may_chu;

use chung::*;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// Gửi một dòng (kèm dữ liệu) rồi đọc một dòng trả lời.
async fn gui(host: &str, port: u16, line: &str, data: &[u8]) -> String {
    let Ok(mut s) = TcpStream::connect((host, port)).await else {
        return "LOI".into();
    };
    let _ = s.write_all(format!("{line}\n").as_bytes()).await;
    let _ = s.write_all(data).await;
    let mut tra_loi = String::new();
    let _ = BufReader::new(s).read_line(&mut tra_loi).await;
    tra_loi.trim_end().to_string()
}

async fn tai(host: String, port: u16) {
    let data = Arc::new(du_lieu(SO_BYTE));
    say(&format!(
        "Máy tạo tải: {N_YC} yêu cầu đồng thời mỗi đợt, mỗi yêu cầu {} KB, băm SHA-256 {SO_VONG} vòng (≈ {:.0} MB dữ liệu băm)",
        SO_BYTE / 1024,
        (SO_BYTE * SO_VONG) as f64 / 1e6
    ));
    let che_do = [
        ("chan", "A. CPU ngay trên vòng lặp"),
        ("tach", "B. đẩy sang nhóm luồng"),
        ("gioihan", "C. nhóm luồng + áp lực ngược"),
    ];
    let mut bang = Vec::new();
    let mut mau_hex = String::new();
    for (cd, nhan) in che_do {
        gui(&host, port, "DATLAI", &[]).await;
        let t = Instant::now();
        let hs: Vec<_> = (1..=N_YC)
            .map(|k| {
                let (h, d) = (host.clone(), Arc::clone(&data));
                tokio::spawn(async move {
                    gui(
                        &h,
                        port,
                        &format!("BAM {k} {cd} {SO_BYTE} {SO_VONG} 10000"),
                        &d,
                    )
                    .await
                })
            })
            .collect();
        let mut kq = Vec::new();
        for h in hs {
            kq.push(h.await.unwrap());
        }
        let dt = t.elapsed().as_secs_f64();
        let ok: Vec<String> = kq
            .iter()
            .filter(|x| x.starts_with("OK"))
            .map(|x| x.split(' ').nth(2).unwrap_or("").to_string())
            .collect();
        if mau_hex.is_empty() {
            mau_hex = ok.first().cloned().unwrap_or_default();
        }
        let khac: HashSet<&String> = ok.iter().collect();
        let tk: Vec<String> = gui(&host, port, &format!("THONGKE {nhan}"), &[])
            .await
            .split(' ')
            .map(String::from)
            .collect();
        say(&format!(
            "Đợt {nhan}: {}/{N_YC} OK, {dt:.2} s, {:.1} yc/s, {} giá trị băm khác nhau",
            ok.len(),
            N_YC as f64 / dt,
            khac.len()
        ));
        bang.push((nhan, dt, tk));
    }
    say("");
    say(&format!(
        "  {:<18}{:>10}{:>7}{:>10}{:>10}{:>6}",
        "Chế độ", "Thời gian", "yc/s", "Trễ nhịp", "Cảm biến", "Nhóm"
    ));
    let ngan = ["A. chặn vòng lặp", "B. nhóm luồng", "C. + áp lực ngược"]; // nhãn ngắn trong bảng
    for (ten, (_, dt, tk)) in ngan.iter().zip(&bang) {
        let (tn, tm, nm): (f64, f64, usize) = (
            tk[1].parse().unwrap_or(0.0),
            tk[3].parse().unwrap_or(0.0),
            tk[4].parse().unwrap_or(0),
        );
        say(&format!(
            "  {ten:<18}{dt:>8.2} s{:>7.1}{tn:>7.1} ms{tm:>7.1} ms{nm:>6}",
            N_YC as f64 / dt
        ));
    }
    say("  (Trễ nhịp, Cảm biến: độ trễ tối đa của nhịp tim 10 ms và của");
    say("   mẫu cảm biến; Nhóm: số việc ở nhóm tính cùng lúc, tối đa)");
    say(&format!(
        "Băm kiểm tra (16 hex đầu): {mau_hex} – giống nhau ở mọi bản ngôn ngữ"
    ));
    say("Nhận xét: A chặn vòng lặp → nhịp tim, mẫu cảm biến và mọi kết nối khác đều đứng chờ; B, C giữ");
    say(&format!("  vòng lặp thông suốt; C chỉ cho {GIOI_HAN} việc vào nhóm {NHOM_TINH} luồng, phần còn lại chờ"));
    say("  bất đồng bộ ở cổng vào – hàng đợi của nhóm luồng không phình theo tải.");
    say("");
    say("(d) Hạn chót và hủy xuyên ranh giới async → luồng (chế độ tach):");
    for (ma, vong, han) in [(101, SO_VONG, 1000), (102, SO_VONG * 8, 30)] {
        let t = Instant::now();
        let tl = gui(
            &host,
            port,
            &format!("BAM {ma} tach {SO_BYTE} {vong} {han}"),
            &data,
        )
        .await;
        say(&format!(
            "  yêu cầu {ma} ({vong} vòng, hạn {han} ms): {} sau {:.0} ms",
            tl.split(' ').next().unwrap_or(""),
            t.elapsed().as_secs_f64() * 1000.0
        ));
    }
    say("");
    say("(c) Luồng nền giao kết quả vào vòng lặp sự kiện:");
    for ma in [201, 202] {
        let tl = gui(&host, port, &format!("CAMBIEN {ma}"), &[]).await;
        let p: Vec<&str> = tl.split(' ').collect();
        say(&format!(
            "  CAMBIEN {ma} → giá trị {}, future do luồng {} hoàn tất",
            p[2], p[3]
        ));
    }
}

fn main() {
    let _ = bay_gio_ms(); // khởi động đồng hồ
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 3 || !(a[0] == "server" || a[0] == "tai") {
        eprintln!("Cách dùng: cargo run -q --release -- server|tai <host> <port>");
        std::process::exit(1);
    }
    let port: u16 = a[2].parse().expect("cổng không hợp lệ");
    if a[0] == "server" {
        may_chu::run(&a[1], port);
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(tai(a[1].clone(), port));
    }
}
