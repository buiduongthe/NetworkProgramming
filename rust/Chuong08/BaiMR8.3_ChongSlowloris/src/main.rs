//! Bài mở rộng MR8.3 – Phòng thủ tấn công từ chối dịch vụ mức ứng dụng kiểu Slowloris.
//!
//! Bài mở rộng, KHÔNG có trong giáo trình; nối tiếp Chương 6 (server nhiều client, Bài 6.1/6.2)
//! và Chương 8 (vận hành an toàn). Mục đích DUY NHẤT là dạy cách *phòng thủ*: so sánh một server
//! HTTP/1.1 nhỏ ở hai chế độ và đo xem một yêu cầu bình thường còn được phục vụ hay không.
//!
//!   server vulnerable <host> <port>   – nhóm luồng cố định, KHÔNG đặt hạn đọc tiêu đề (dễ bị đánh gục)
//!   server protected  <host> <port>   – có hạn đọc tiêu đề, cỡ tiêu đề tối đa, hạn chót rỗi,
//!                                        giới hạn kết nối theo IP và trần kết nối toàn cục (trả 503)
//!   slow   <host> <port> [N] [nguồn]  – CLIENT THÍ NGHIỆM: mở N kết nối tới 127.0.0.1 và nhỏ giọt
//!                                        từng byte tiêu đề (chỉ cho loopback – từ chối mọi đích khác).
//!   probe  <host> <port>              – gửi MỘT yêu cầu HTTP bình thường, đo thành công/độ trễ
//!
//! CẢNH BÁO ĐẠO ĐỨC: vai "slow" chỉ dùng trong phòng thí nghiệm cô lập, chỉ nhắm 127.0.0.1 của
//! chính máy bạn, để kiểm chứng biện pháp phòng thủ. Tuyệt đối không nhắm máy/dịch vụ của người khác.

mod client;
mod server;

use std::process;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("server") if a.len() >= 5 => server::chay_server(&a[2], &a[3], &a[4]),
        Some("slow") if a.len() >= 4 => {
            let n: usize = a.get(4).and_then(|v| v.parse().ok()).unwrap_or(150);
            let nguon = a.get(5).map(String::as_str).unwrap_or("127.0.0.2");
            client::chay_slow(&a[2], &a[3], n, nguon);
        }
        Some("probe") if a.len() >= 4 => client::chay_probe(&a[2], &a[3]),
        _ => {
            println!("Cách dùng:");
            println!("  mr8_3_chong_slowloris server vulnerable|protected <host> <port>");
            println!("  mr8_3_chong_slowloris slow  <host> <port> [N]   (chỉ 127.0.0.1)");
            println!("  mr8_3_chong_slowloris probe <host> <port>");
            process::exit(1);
        }
    }
}
