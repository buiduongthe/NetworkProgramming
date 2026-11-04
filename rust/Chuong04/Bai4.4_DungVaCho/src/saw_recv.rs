//! Bên nhận của giao thức dừng và chờ trên UDP.
//!
//! Cài đặt Mã giả 4.7 SAW_RECEIVE (Mục 4.4.1). Kế thừa: Mã giả 4.1 (vòng RECVFROM/SENDTO).
//! Luôn gửi ACK cho datagram nhận được; datagram trùng bị bỏ dữ liệu nhưng vẫn được ACK;
//! sau FIN chờ thêm 2 × MAX_RTO (như TIME-WAIT của TCP) để trả lời FIN lặp lại khi ACK bị mất;
//! mỗi FIN lặp lại đặt lại thời gian chờ này.

use crate::endpoint::parse_endpoint;
use crate::saw_common::*;
use std::fs::File;
use std::io::{self, ErrorKind, Write};
use std::net::UdpSocket;
use std::time::Duration;

/// Mã giả 4.7 – SAW_RECEIVE; trả về (số byte đã ghi, số datagram trùng).
fn saw_receive(s: &UdpSocket, out: &mut File, loss: &mut LossSim, wait_ms: u64) -> io::Result<(u64, u32)> {
    let (mut expected, mut written, mut dups) = (0u32, 0u64, 0u32); // dòng 2
    let mut buf = vec![0u8; 65535];
    loop {
        // dòng 3: LOOP
        let (n, peer) = match s.recv_from(&mut buf) {
            // dòng 5
            Ok(v) => v,
            // dòng 6: hết thời gian chờ – chỉ xảy ra sau FIN (trước đó socket chờ không giới hạn)
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                println!("[nhận] Hết {wait_ms} ms không có datagram mới: kết thúc");
                return Ok((written, dups)); // dòng 7
            }
            Err(e) if e.kind() == ErrorKind::ConnectionReset => continue, // Windows: ICMP port unreachable
            Err(e) => return Err(e),
        };
        let d = &buf[..n];
        if n < 5 {
            continue; // dòng 9: datagram lỗi: bỏ
        }
        let seq = u32::from_be_bytes([d[0], d[1], d[2], d[3]]); // dòng 10
        let typ = d[4];
        let dropped = loss.drop_next();
        if !dropped {
            s.send_to(&header(seq, ACK), peer)?; // dòng 11: luôn ACK
        }
        let note = if dropped { " – mô phỏng mất ACK" } else { "" };
        if seq == expected && typ == FIN {
            // dòng 12
            s.set_read_timeout(Some(Duration::from_millis(wait_ms)))?; // dòng 13: 2 × MAX_RTO, như TIME-WAIT
            println!("[nhận] seq={seq} FIN: gửi ACK {seq}{note}; chờ {wait_ms} ms (như TIME-WAIT)");
        } else if seq == expected {
            // dòng 14
            out.write_all(&d[5..])?; // dòng 15
            written += (n - 5) as u64;
            expected += 1; // dòng 16
            println!("[nhận] seq={seq} DATA {} byte: ghi tệp, gửi ACK {seq}{note}", n - 5);
        } else {
            // seq < expected: trùng
            dups += 1;
            println!("[nhận] seq={seq} trùng: bỏ dữ liệu, vẫn gửi ACK {seq}{note}");
        }
    }
}

pub fn run(args: &[String]) -> i32 {
    let (host, port) = parse_endpoint(&args[..args.len().min(2)], "0.0.0.0", 9000);
    let path = args.get(2).map(|s| s.as_str()).unwrap_or("tep_nhan.bin");
    let mut loss = LossSim::new(args.get(3).map(|s| s.as_str()).unwrap_or(""));
    // thời gian chờ sau FIN: mặc định 2 × MAX_RTO = 120 000 ms như mã giả; tham số thứ năm (thêm cho
    // kịch bản minh họa) cho phép rút ngắn khi biết RTO thực tế nhỏ, ví dụ 4000 ms trên mạng nội bộ
    let wait_ms: u64 = args.get(4).and_then(|t| t.trim().parse().ok()).unwrap_or((2.0 * MAX_RTO) as u64);
    let s = match UdpSocket::bind((host.as_str(), port)) {
        Ok(s) => s,
        Err(e) => {
            println!("[nhận] Không mở được {host}:{port}: {e}");
            return 1;
        }
    };
    println!("[nhận] Đang chờ dữ liệu UDP tại {host}:{port}, ghi vào {path}");
    println!("[nhận] Mô phỏng mất ACK: {}", loss.describe());
    let mut out = match File::create(path) {
        Ok(f) => f,
        Err(e) => {
            println!("[nhận] Không tạo được tệp {path}: {e}");
            return 1;
        }
    };
    let (written, dups) = match saw_receive(&s, &mut out, &mut loss, wait_ms) {
        Ok(v) => v,
        Err(e) => {
            println!("[nhận] Lỗi: {e}");
            return 1;
        }
    };
    drop(out); // đóng tệp trước khi tính mã băm
    drop(s);
    println!("[nhận] Hoàn tất: {written} byte, {dups} datagram trùng; SHA-256:");
    println!("[nhận]   {}", sha256_file(path).unwrap_or_default());
    0
}
