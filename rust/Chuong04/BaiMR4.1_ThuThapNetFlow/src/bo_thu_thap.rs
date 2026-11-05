//! Bộ thu thập NetFlow v5 (collector) trên UDP.
//!
//! Server UDP nhận datagram, mỗi datagram là trọn một gói xuất (UDP giữ ranh giới thông điệp),
//! giải mã nhị phân theo thứ tự byte mạng như Bài 4.3 (truy vấn DNS). Bộ thu:
//!   - kiểm tra tính hợp lệ (phiên bản, độ dài = 24 + 48 x count) và loại gói hỏng;
//!   - phát hiện mất gói nhờ trường flow_sequence (giống số thứ tự ở Bài 4.4);
//!   - tổng hợp: nguồn dùng nhiều băng thông nhất, dịch vụ (cổng/giao thức) phổ biến.
//! Dừng khi nhàn rỗi quá --nhan-roi giây sau gói đầu tiên (hoặc Ctrl+C) và in báo cáo.

use crate::netflow_v5::{proto_name, Header, Record, HEADER_SIZE, MAX_RECORDS, RECORD_SIZE};
use crate::{die, split_args};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::UdpSocket;
use std::time::Duration;

/// Trả về (tiêu_đề, danh_sách_bản_ghi) hoặc chuỗi lý do nếu gói không hợp lệ.
fn parse_packet(data: &[u8]) -> Result<(Header, Vec<Record>), String> {
    if data.len() < HEADER_SIZE {
        return Err(format!("quá ngắn ({} byte)", data.len()));
    }
    let h = Header::unpack(data);
    if h.version != 5 {
        return Err(format!("phiên bản {}, không phải 5", h.version));
    }
    let count = h.count as usize;
    if !(1..=MAX_RECORDS).contains(&count) || data.len() != HEADER_SIZE + count * RECORD_SIZE {
        return Err(format!("độ dài {} không khớp count={count}", data.len()));
    }
    let records = (0..count).map(|i| Record::unpack(data, HEADER_SIZE + i * RECORD_SIZE)).collect();
    Ok((h, records))
}

/// In số thực giống Python: 1.5 -> "1.5", 2.0 -> "2.0".
fn py_float(x: f64) -> String {
    if x.fract() == 0.0 {
        format!("{x:.1}")
    } else {
        format!("{x}")
    }
}

pub fn run(args: &[String]) {
    let (pos, opt) = split_args(args);
    let host = pos.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let port: u16 = pos.get(1).map(|p| p.parse().unwrap_or_else(|_| die(&format!("Cổng sai: {p}")))).unwrap_or(2055);
    let idle: f64 = opt
        .iter()
        .find(|(k, _)| k == "nhan-roi")
        .map(|(_, v)| v.parse().unwrap_or_else(|_| die(&format!("--nhan-roi sai: {v}"))))
        .unwrap_or(1.5);
    let sock = UdpSocket::bind((host.as_str(), port)).unwrap_or_else(|e| die(&format!("[thu-thap] Không mở được {host}:{port}: {e}")));
    println!("[thu-thap] Nghe NetFlow v5 tại UDP {host}:{port}");
    let mut by_src: HashMap<String, u64> = HashMap::new();
    let mut by_service: HashMap<(u16, u8), (u64, u64)> = HashMap::new(); // (cổng, giao thức) -> (số luồng, byte)
    let mut expected: HashMap<(String, u8), i64> = HashMap::new(); // (địa chỉ máy xuất, engine) -> seq mong đợi
    let (mut ok, mut bad, mut flows, mut lost, mut total_bytes) = (0, 0, 0u64, 0i64, 0u64);
    let mut buf = [0u8; 2048]; // gói v5 lớn nhất 24 + 30*48 = 1464 byte
    loop {
        let (n, addr) = match sock.recv_from(&mut buf) {
            Ok(v) => v,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                println!("[thu-thap] Nhàn rỗi {} s, kết thúc", py_float(idle));
                break;
            }
            Err(e) => {
                println!("[thu-thap] Lỗi nhận: {e}");
                continue;
            }
        };
        sock.set_read_timeout(Some(Duration::from_secs_f64(idle))).ok(); // sau gói đầu tiên mới đếm nhàn rỗi
        let (head, records) = match parse_packet(&buf[..n]) {
            Ok(v) => v,
            Err(e) => {
                bad += 1;
                println!("[thu-thap] Loại gói từ {addr}: {e}");
                continue;
            }
        };
        let key = (addr.ip().to_string(), head.engine_id);
        let seq = head.flow_sequence as i64;
        if let Some(&want) = expected.get(&key) {
            if seq != want {
                let gap = seq - want;
                lost += gap.max(0);
                println!("[thu-thap] MẤT GÓI: chờ seq={want}, nhận seq={seq}, thiếu {gap} luồng");
            }
        }
        expected.insert(key, seq + head.count as i64);
        ok += 1;
        flows += head.count as u64;
        println!("[thu-thap] Gói seq={seq}: {} luồng", head.count);
        for r in &records {
            *by_src.entry(r.src.to_string()).or_default() += r.octets as u64;
            let svc = by_service.entry((r.dport, r.proto)).or_default();
            svc.0 += 1;
            svc.1 += r.octets as u64;
            total_bytes += r.octets as u64;
        }
    }
    println!("[thu-thap] Hợp lệ {ok} gói, loại {bad} gói, mất {lost} luồng");
    println!("[thu-thap] Đã thu {flows} luồng, tổng {total_bytes} byte");
    if total_bytes == 0 {
        return;
    }
    println!("[thu-thap] Nguồn dùng nhiều byte nhất:");
    let mut srcs: Vec<(String, u64)> = by_src.into_iter().collect();
    srcs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0))); // nhiều byte trước; bằng nhau theo địa chỉ
    for (src, b) in srcs.iter().take(3) {
        println!("  {src:<12} {b:>9} byte ({:.1}%)", *b as f64 * 100.0 / total_bytes as f64);
    }
    println!("[thu-thap] Dịch vụ theo số luồng:");
    let mut svcs: Vec<((u16, u8), (u64, u64))> = by_service.into_iter().collect();
    svcs.sort_by(|a, b| (b.1).0.cmp(&(a.1).0).then(a.0.cmp(&b.0)));
    for ((port, proto), (n, b)) in svcs {
        let name = format!("{port}/{}", proto_name(proto));
        println!("  {name:<9} {n:>3} luồng {b:>9} byte");
    }
}
