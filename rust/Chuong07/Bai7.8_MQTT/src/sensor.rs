//! Bài 7.8 – Thiết bị cảm biến phát số đo nhiệt độ qua MQTT (Mã giả 7.9, PROCEDURE SENSOR; Mục 7.5.5).
//!
//! Kết nối với di chúc (Last Will) "offline" giữ lại, phát "online" (QoS 1, giữ lại), sau đó phát số đo
//! JSON {v, ts} QoS 0 mỗi chu kỳ. Khi đủ số lần: tắt đúng thủ tục (phát "offline" rồi DISCONNECT), hoặc
//! với tùy chọn --mat-ket-noi thì đóng socket đột ngột để broker tự phát di chúc.
//!
//! Chạy:  cargo run -- sensor <mã_thiết_bị> [host] [port] [số_lần] [chu_kỳ_ms] [--mat-ket-noi]
//!        (mặc định localhost 1883, số_lần 0 = chạy mãi, chu kỳ 5000 ms)
use crate::mqtt_mini::MqttClient;
use crate::{int_arg, port_arg};
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// READ_SENSOR(): giá trị mô phỏng 24,0–32,0 °C (số ngẫu nhiên lấy từ khóa băm của RandomState).
fn read_sensor() -> f64 {
    let r = std::collections::hash_map::RandomState::new().build_hasher().finish();
    24.0 + (r % 81) as f64 / 10.0
}

fn sensor(args: &[String], lose: bool) -> io::Result<()> {
    let device_id = args.first().map(String::as_str).unwrap_or("day1");
    let host = args.get(1).map(String::as_str).unwrap_or("localhost");
    let port = port_arg(args, 2, 1883);
    let times = int_arg(args, 3, 0);
    let period_ms = int_arg(args, 4, 5000);
    let st = format!("nhamay/{device_id}/trangthai");
    let mut c = MqttClient::connect(host, port, device_id, 30, Some((&st, "offline", 1, true)))?;
    println!("[{device_id}] Đã kết nối broker {host}:{port} (MQTT 5.0, keep alive 30 s, di chúc '{st}' = offline)");
    c.publish(&st, "online", 1, true)?;
    println!("[{device_id}] PUBLISH {st} = online (QoS 1, giữ lại) -> đã nhận PUBACK");
    let mut n = 0;
    while times == 0 || n < times {
        n += 1;
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let msg = format!("{{\"v\":{:.1},\"ts\":{ts}}}", read_sensor()); // JSON_ENCODE({v: v, ts: NOW()})
        c.publish(&format!("nhamay/{device_id}/nhietdo"), &msg, 0, false)?;
        println!("[{device_id}] PUBLISH nhamay/{device_id}/nhietdo = {msg} (QoS 0)");
        thread::sleep(Duration::from_millis(period_ms));
    }
    if lose {
        drop(c); // mô phỏng mất điện: đóng socket, không gửi DISCONNECT
        println!("[{device_id}] Mô phỏng mất kết nối: đóng socket, không gửi DISCONNECT");
    } else {
        c.publish(&st, "offline", 1, true)?;
        c.disconnect()?;
        println!("[{device_id}] Tắt đúng thủ tục: phát offline rồi gửi DISCONNECT");
    }
    Ok(())
}

pub fn run(args: &[String]) {
    let lose = args.iter().any(|a| a == "--mat-ket-noi");
    let args: Vec<String> = args.iter().filter(|a| *a != "--mat-ket-noi").cloned().collect();
    if let Err(e) = sensor(&args, lose) {
        println!("[sensor] Lỗi: {e}");
        std::process::exit(1);
    }
}
