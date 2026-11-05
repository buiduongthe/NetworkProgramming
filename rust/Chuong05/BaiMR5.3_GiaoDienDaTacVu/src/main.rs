//! Bài mở rộng MR5.3 – Giao diện luôn phản hồi khi nhiều tác vụ mạng chạy song song (Rust, giao diện web).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 5.3 (giao diện bất đồng bộ), Bài 5.5 (tải tệp
//! có tiến trình và hủy), Bài 5.2 (giới hạn đồng thời), Bài 1.2 (kiểm tra cổng), Bài 4.3 (tra cứu tên
//! qua UDP), Bài 7.4 (SSE). Thư viện chuẩn Rust không có thư viện giao diện đồ họa, nên giao diện là một
//! trang web cục bộ (giống bản Go): chương trình phục vụ trang và API bằng một máy chủ HTTP tối giản viết
//! trên tokio, mỗi tác vụ là một tác vụ tokio có kênh hủy, tiến trình được đẩy tới trình duyệt bằng SSE.
//!
//!   cargo run -q --release -- server <host> <port>                              máy chủ thử
//!   cargo run -q --release -- ui <host> <cổng_giao_diện> <host_thử> <port_thử>     chương trình giao diện

mod http;
mod may_chu_thu;
mod tac_vu;
mod ui;

use std::process;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let port = |s: &str| -> u16 {
        s.parse().unwrap_or_else(|_| {
            eprintln!("Cổng sai: {s}");
            process::exit(1)
        })
    };
    match a.get(1).map(String::as_str) {
        Some("server") if a.len() >= 4 => may_chu_thu::run(&a[2], port(&a[3])),
        Some("ui") if a.len() >= 6 => ui::run(&a[2], port(&a[3]), format!("{}:{}", a[4], port(&a[5]))),
        _ => {
            println!("Cách dùng:");
            println!("  mr5_3_giao_dien_da_tac_vu server <host> <port>");
            println!("  mr5_3_giao_dien_da_tac_vu ui <host> <cổng_giao_diện> <host_thử> <port_thử>");
            process::exit(1);
        }
    }
}
