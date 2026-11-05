//! Bài mở rộng MR5.6 – Mô phỏng thực thi tác vụ (bản dòng lệnh; bản giao diện: ung_dung.rs).
//!
//! Bài mở rộng, không có trong giáo trình; minh họa Chương 5 (luồng, bất đồng bộ) và Chương 6 (máy chủ đa
//! luồng, nhóm luồng): cùng ba tác vụ T1, T2, T3 (CPU → chờ I/O → CPU) và một việc khẩn U, chạy theo bốn chế độ
//! đơn luồng/đa luồng × đồng bộ/bất đồng bộ trên bộ lập lịch mô phỏng (lap_lich.rs).
//!
//! Cách dùng:
//!     mo_phong <chế_độ> [--t1 4] [--t2 3] [--t3 2] [--loi 2] [--lat 0.5] [--khan 0.3]
//!              [--khong-khan] [--khong-phu-thuoc] [--khong-chia]
//!     chế_độ: don_dong_bo | da_dong_bo | don_bat_dong_bo | da_bat_dong_bo | so_sanh | tat_ca

use lap_lich::{self as l, ThamSo, CHE_DO};

fn nhan_xet(m: &str) -> &'static [&'static str] {
    match m {
        "don_dong_bo" => &[
            "Nhận xét: một luồng, lời gọi chặn: khi chờ I/O luồng đứng",
            "  yên (~), lõi CPU bỏ không; tác vụ làm lần lượt theo thứ tự",
            "  đến nên việc khẩn U phải chờ tất cả.",
        ],
        "da_dong_bo" => &[
            "Nhận xét: mỗi tác vụ một luồng: I/O của luồng này chồng lên",
            "  CPU của luồng khác; luồng nhiều hơn lõi phải chờ lõi (-);",
            "  hệ điều hành chia lát thời gian và cho U (ưu tiên cao) chen",
            "  ngang ngay; T3 chờ join (.); nửa sau của T1 chạy song song",
            "  trên luồng T1b.",
        ],
        "don_bat_dong_bo" => &[
            "Nhận xét: một luồng vẫn chồng được các lần chờ I/O (hàng",
            "  I/O chờ có nhiều việc cùng lúc) mà không cần thêm luồng;",
            "  nhưng bước CPU chạy hết mới nhường (hợp tác): U phải xếp",
            "  hàng, hai nửa của T1 chạy lần lượt – bất đồng bộ không",
            "  phải là song song.",
        ],
        _ => &[
            "Nhận xét: nhóm thợ + bất đồng bộ: I/O không giữ luồng, bước",
            "  CPU giao cho thợ rảnh nên một tác vụ có thể chạy tiếp trên",
            "  luồng khác sau khi chờ I/O; hai nửa của T1 chạy song song",
            "  trên 2 thợ; số luồng chỉ bằng số lõi.",
        ],
    }
}

fn in_tham_so(ts: &ThamSo) {
    let t: Vec<String> = ts.d.iter().map(|&d| l::giay(d)).collect();
    println!("Tác vụ: T1 {} s, T2 {} s, T3 {} s; mỗi tác vụ: CPU 1/4 →", t[0], t[1], t[2]);
    println!("  chờ I/O 1/2 → CPU phần còn lại; {} lõi CPU; lát thời gian {} s", ts.so_loi, l::giay(ts.lat));
    let mut them = Vec::new();
    if ts.phu_thuoc {
        them.push("T3 chờ T1, T2".to_string());
    }
    if ts.chia {
        them.push("CPU cuối T1 chia 2 nửa".to_string());
    }
    if ts.khan {
        them.push(format!("việc khẩn U lúc {} s", l::giay(ts.t_khan)));
    }
    println!("  {}", if them.is_empty() { "không phụ thuộc, không chia, không việc khẩn".to_string() } else { them.join("; ") });
}

fn chay_mot(che_do: &str, ts: &ThamSo) {
    println!("=== {} ===", l::ten_che_do(che_do));
    in_tham_so(ts);
    let kq = l::chay(che_do, ts);
    println!();
    println!("Ký hiệu: 1 2 3 U tác vụ đang dùng CPU; ~ luồng bị chặn chờ I/O;");
    println!("  - sẵn sàng nhưng chờ lõi; . chờ join; I/O chờ: số I/O bất đồng");
    println!("  bộ đang chờ (không giữ luồng nào)");
    for d in l::gantt(&kq, 48) {
        println!("{d}");
    }
    println!();
    println!("Nhật ký:");
    for (t, text) in &kq.nhat_ky {
        println!("  {:>4} s  {text}", l::giay(*t));
    }
    println!();
    println!("  Tác vụ   Đến  Bắt đầu   Xong  Thời gian");
    for v in &kq.viec {
        println!(
            "  {:<6} {:>5} {:>8} {:>6} {:>9} s",
            v.ten,
            l::giay(v.den),
            l::giay(v.bat_dau),
            l::giay(v.xong),
            l::giay(v.xong - v.den)
        );
    }
    let s = l::chi_so(&kq);
    println!("  Hoàn tất lúc {} s; số luồng {}; lõi bận {} %", l::giay(s.tong), s.luong, s.ban);
    let mut dong = format!("  Chen ngang {} lần; hết lát thời gian {} lần", s.chen, s.lat);
    if s.u_cho >= 0 {
        dong += &format!("; U chờ {} s", l::giay(s.u_cho));
    }
    println!("{dong}");
    println!();
    for d in nhan_xet(che_do) {
        println!("{d}");
    }
    println!();
}

fn so_sanh(ts: &ThamSo) {
    println!("=== So sánh bốn chế độ ===");
    in_tham_so(ts);
    println!();
    println!("  {:<14} {:>8} {:>5} {:>7} {:>6} {:>4} {:>7}", "Chế độ", "Hoàn tất", "Luồng", "Lõi bận", "U chờ", "Chen", "Hết lát");
    for m in CHE_DO {
        let s = l::chi_so(&l::chay(m, ts));
        let u = if s.u_cho >= 0 { format!("{} s", l::giay(s.u_cho)) } else { "–".into() };
        println!(
            "  {:<14} {:>8} {:>5} {:>7} {:>6} {:>4} {:>7}",
            l::ten_ngan(m),
            format!("{} s", l::giay(s.tong)),
            s.luong,
            format!("{} %", s.ban),
            u,
            s.chen,
            s.lat
        );
    }
    println!();
    println!("Ghi nhớ: đa luồng hệ điều hành chen ngang được (ưu tiên, lát");
    println!("  thời gian) nhưng tốn luồng; bất đồng bộ dùng ít luồng, chồng");
    println!("  được I/O nhưng chỉ nhường ở điểm await; nhóm thợ + bất đồng");
    println!("  bộ dùng hết các lõi mà số luồng vẫn bằng số lõi.");
}

const CACH_DUNG: &str = "Cách dùng: mo_phong <chế_độ> [--t1 4] [--t2 3] [--t3 2] [--loi 2] [--lat 0.5] [--khan 0.3]\n                \
                         [--khong-khan] [--khong-phu-thuoc] [--khong-chia]\n  \
                         chế_độ: don_dong_bo | da_dong_bo | don_bat_dong_bo | da_bat_dong_bo | so_sanh | tat_ca";

fn loi(thong_bao: &str) -> ! {
    eprintln!("{CACH_DUNG}\nmo_phong: lỗi: {thong_bao}");
    std::process::exit(2);
}

fn main() {
    // Đọc tham số giống argparse: tùy chọn có thể đứng trước/sau chế độ, dạng "--t1 5" hoặc "--t1=5".
    let (mut t1, mut t2, mut t3, mut lat, mut khan) = (4.0, 3.0, 2.0, 0.5, 0.3);
    let mut so_loi: i64 = 2;
    let (mut khong_khan, mut khong_phu_thuoc, mut khong_chia) = (false, false, false);
    let mut che_do: Option<String> = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-h" || a == "--help" {
            println!("{CACH_DUNG}");
            return;
        }
        if let Some(ten) = a.strip_prefix("--") {
            let (ten, gia_tri) = match ten.split_once('=') {
                Some((t, g)) => (t.to_string(), Some(g.to_string())),
                None => (ten.to_string(), None),
            };
            match ten.as_str() {
                "khong-khan" => khong_khan = true,
                "khong-phu-thuoc" => khong_phu_thuoc = true,
                "khong-chia" => khong_chia = true,
                "t1" | "t2" | "t3" | "loi" | "lat" | "khan" => {
                    let g = match gia_tri {
                        Some(g) => g,
                        None => {
                            i += 1;
                            args.get(i).cloned().unwrap_or_else(|| loi(&format!("--{ten} cần một giá trị")))
                        }
                    };
                    if ten == "loi" {
                        so_loi = g.trim().parse().unwrap_or_else(|_| loi(&format!("--loi: số nguyên không hợp lệ '{g}'")));
                    } else {
                        let x: f64 = g.trim().parse().unwrap_or_else(|_| loi(&format!("--{ten}: số không hợp lệ '{g}'")));
                        match ten.as_str() {
                            "t1" => t1 = x,
                            "t2" => t2 = x,
                            "t3" => t3 = x,
                            "lat" => lat = x,
                            _ => khan = x,
                        }
                    }
                }
                _ => loi(&format!("không có tùy chọn {a}")),
            }
        } else if che_do.is_none() {
            if !CHE_DO.contains(&a.as_str()) && a != "so_sanh" && a != "tat_ca" {
                loi(&format!("chế độ không hợp lệ '{a}'"));
            }
            che_do = Some(a.clone());
        } else {
            loi(&format!("tham số thừa '{a}'"));
        }
        i += 1;
    }
    let Some(che_do) = che_do else { loi("thiếu chế_độ") };
    let ts = ThamSo::new(t1, t2, t3, so_loi, lat, !khong_phu_thuoc, !khong_chia, !khong_khan, khan);
    match che_do.as_str() {
        "so_sanh" => so_sanh(&ts),
        "tat_ca" => {
            for m in CHE_DO {
                chay_mot(m, &ts);
            }
            so_sanh(&ts);
        }
        m => chay_mot(m, &ts),
    }
}
