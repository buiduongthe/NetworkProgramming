//! Bài 7.8 – Bảng theo dõi đăng ký bộ lọc có ký tự đại diện (Mã giả 7.9, PROCEDURE DASHBOARD; Mục 7.5.5).
//!
//! Đăng ký nhamay/+/nhietdo (QoS 0, ON_TEMP) và nhamay/+/trangthai (QoS 1, ON_STATUS), rồi chạy vòng sự kiện;
//! gửi PINGREQ khi im lặng quá nửa thời gian keep alive. Ctrl+C: gửi DISCONNECT rồi thoát.
//!
//! Chạy:  cargo run -- dashboard [host] [port] [keep_alive_s] [client_id]   (mặc định localhost 1883 60 dashboard-01)
//!        Hai client cùng client_id thì broker ngắt client cũ, nên bảng thứ hai cần client_id khác.
use crate::mqtt_mini::{MqttClient, DISCONNECT};
use crate::{int_arg, port_arg};
use std::io::{self, Write};

fn on_temp(topic: &str, payload: &str, _qos: u8, _retain: bool) {
    println!("[dashboard] Nhiệt độ  {topic}: {payload}");
}

fn on_status(topic: &str, payload: &str, qos: u8, retain: bool) {
    let r = if retain { ", tin giữ lại" } else { "" };
    println!("[dashboard] Trạng thái {topic}: {payload} (QoS {qos}{r})");
}

fn dashboard(args: &[String]) -> io::Result<()> {
    let host = args.first().map(String::as_str).unwrap_or("localhost");
    let port = port_arg(args, 1, 1883);
    let keep_alive = int_arg(args, 2, 60).min(65535) as u16;
    let client_id = args.get(3).map(String::as_str).unwrap_or("dashboard-01");
    let mut c = MqttClient::connect(host, port, client_id, keep_alive, None)?;
    println!("[dashboard] Đã kết nối broker {host}:{port} là '{client_id}' (MQTT 5.0, keep alive {keep_alive} s)");
    // Ctrl+C: hàm xử lý chạy ở luồng riêng, gửi gói DISCONNECT qua bản sao socket rồi kết thúc chương trình
    let mut s2 = c.sock.try_clone()?;
    ctrlc::set_handler(move || {
        let _ = s2.write_all(&[DISCONNECT << 4, 0]); // DISCONNECT, độ dài còn lại 0
        println!("[dashboard] Đã gửi DISCONNECT");
        std::process::exit(0);
    })
    .map_err(io::Error::other)?;
    let subs: [(&str, u8, crate::mqtt_mini::Handler); 2] =
        [("nhamay/+/nhietdo", 0, on_temp), ("nhamay/+/trangthai", 1, on_status)];
    for (flt, qos, f) in subs {
        let granted = c.subscribe(flt, qos, f)?;
        println!("[dashboard] SUBSCRIBE {flt} (QoS {qos}) -> SUBACK, QoS được cấp {granted}");
    }
    c.run_event_loop(|| println!("[dashboard] PINGREQ -> PINGRESP")) // thư viện gọi ON_TEMP, ON_STATUS
}

pub fn run(args: &[String]) {
    if let Err(e) = dashboard(args) {
        println!("[dashboard] {e}");
    }
}
