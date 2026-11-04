//! Bài 7.8 – Client MQTT 5.0 tối giản trên socket TCP (cài đặt các lệnh mức thư viện của Mã giả 7.9).
//!
//! Hỗ trợ: CONNECT (kèm di chúc – Last Will) / CONNACK, PUBLISH QoS 0 và QoS 1 (PUBACK), SUBSCRIBE / SUBACK,
//! PINGREQ / PINGRESP, DISCONNECT. Không hỗ trợ QoS 2, TLS, thuộc tính MQTT 5.0 (luôn gửi độ dài thuộc tính 0).
//! Định khung MQTT: 1 byte loại gói + cờ, "độ dài còn lại" mã hóa varint 1–4 byte, rồi đúng chừng ấy byte
//! (kỹ thuật tiền tố độ dài của Mục 3.4, đọc bằng RECV_EXACT – Mã giả 3.4). Đặc tả: OASIS MQTT 5.0.
use std::io::{self, ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

pub const CONNECT: u8 = 1;
pub const CONNACK: u8 = 2;
pub const PUBLISH: u8 = 3;
pub const PUBACK: u8 = 4;
pub const SUBSCRIBE: u8 = 8;
pub const SUBACK: u8 = 9;
pub const PINGREQ: u8 = 12;
pub const PINGRESP: u8 = 13;
pub const DISCONNECT: u8 = 14;

/// Hàm xử lý tin đến: (chủ đề, nội dung, qos, cờ giữ lại).
pub type Handler = fn(&str, &str, u8, bool);

/// Chuỗi UTF-8 của MQTT: 2 byte độ dài + dữ liệu.
fn mqtt_str(s: &str) -> Vec<u8> {
    let mut out = (s.len() as u16).to_be_bytes().to_vec();
    out.extend_from_slice(s.as_bytes());
    out
}

/// Mã hóa varint: 7 bit mỗi byte, bit cao = còn byte tiếp.
pub fn varint(mut n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let d = (n & 0x7F) as u8;
        n >>= 7;
        out.push(d | if n > 0 { 0x80 } else { 0 });
        if n == 0 {
            return out;
        }
    }
}

/// Bộ lọc chủ đề: '+' khớp đúng một cấp, '#' khớp mọi cấp còn lại (Mục 7.5.5).
pub fn topic_matches(flt: &str, topic: &str) -> bool {
    let f: Vec<&str> = flt.split('/').collect();
    let t: Vec<&str> = topic.split('/').collect();
    for (i, part) in f.iter().enumerate() {
        if *part == "#" {
            return true;
        }
        if i >= t.len() || (*part != "+" && *part != t[i]) {
            return false;
        }
    }
    f.len() == t.len()
}

/// Lỗi "hết thời gian chờ" của socket (Linux: WouldBlock, Windows: TimedOut).
pub fn is_timeout(e: &io::Error) -> bool {
    e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut
}

pub struct MqttClient {
    pub sock: TcpStream,
    next_id: u16,
    handlers: Vec<(String, Handler)>,
    keep_alive: u64,
    last_send: Instant, // keep alive tính theo gói client GỬI đi
}

impl MqttClient {
    fn send(&mut self, ptype: u8, flags: u8, body: &[u8]) -> io::Result<()> {
        let mut out = vec![(ptype << 4) | flags];
        out.extend(varint(body.len()));
        out.extend_from_slice(body);
        self.sock.write_all(&out)?; // SEND_ALL
        self.last_send = Instant::now();
        Ok(())
    }

    fn recv_exact(&mut self, n: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; n];
        let mut got = 0;
        while got < n {
            let k = match self.sock.read(&mut buf[got..]) {
                Ok(k) => k,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue, // bị tín hiệu (Ctrl+C) ngắt: đọc tiếp
                Err(e) => return Err(e),
            };
            if k == 0 {
                return Err(io::Error::other("Broker đóng kết nối"));
            }
            got += k;
        }
        Ok(buf)
    }

    /// Đọc một gói; chỉ chờ byte đầu với first_timeout, phần còn lại chờ tối đa 5 giây.
    fn read_packet(&mut self, first_timeout: Duration) -> io::Result<(u8, u8, Vec<u8>)> {
        self.sock.set_read_timeout(Some(first_timeout))?;
        let b0 = self.recv_exact(1)?[0]; // hết giờ ở đây: chưa mất byte nào của gói
        self.sock.set_read_timeout(Some(Duration::from_secs(5)))?;
        let (mut n, mut shift) = (0usize, 0);
        loop {
            // giải mã varint "độ dài còn lại"
            let d = self.recv_exact(1)?[0];
            n |= ((d & 0x7F) as usize) << shift;
            shift += 7;
            if d & 0x80 == 0 {
                break;
            }
        }
        Ok((b0 >> 4, b0 & 0x0F, self.recv_exact(n)?))
    }

    /// Gói PUBLISH đến: tách chủ đề, mã gói (QoS > 0), bỏ thuộc tính, gọi hàm xử lý khớp bộ lọc.
    fn dispatch(&mut self, flags: u8, body: &[u8]) -> io::Result<()> {
        let (qos, retain) = ((flags >> 1) & 3, flags & 1 == 1);
        let tlen = u16::from_be_bytes([body[0], body[1]]) as usize;
        let topic = String::from_utf8_lossy(&body[2..2 + tlen]).into_owned();
        let mut i = 2 + tlen;
        if qos > 0 {
            let pid = body[i..i + 2].to_vec();
            i += 2;
            self.send(PUBACK, 0, &pid)?; // QoS 1: xác nhận cho broker
        }
        let (mut plen, mut shift) = (0usize, 0);
        loop {
            // độ dài thuộc tính (varint) rồi bỏ qua thuộc tính
            let d = body[i];
            i += 1;
            plen |= ((d & 0x7F) as usize) << shift;
            shift += 7;
            if d & 0x80 == 0 {
                break;
            }
        }
        let payload = String::from_utf8_lossy(&body[i + plen..]).into_owned();
        for (flt, f) in &self.handlers {
            if topic_matches(flt, &topic) {
                f(&topic, &payload, qos, retain);
            }
        }
        Ok(())
    }

    /// Chờ gói loại ptype; gói PUBLISH đến trong lúc chờ vẫn được xử lý.
    fn wait(&mut self, ptype: u8) -> io::Result<Vec<u8>> {
        loop {
            let (t, flags, body) = self.read_packet(Duration::from_secs(5))?;
            if t == ptype {
                return Ok(body);
            }
            if t == PUBLISH {
                self.dispatch(flags, &body)?;
            }
        }
    }

    /// MQTT_CONNECT; will = (chủ_đề, nội_dung, qos, retain).
    pub fn connect(host: &str, port: u16, client_id: &str, keep_alive: u16, will: Option<(&str, &str, u8, bool)>) -> io::Result<MqttClient> {
        let addr = (host, port).to_socket_addrs()?.next().ok_or(io::Error::other("không phân giải được tên"))?;
        let sock = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
        let mut c = MqttClient { sock, next_id: 1, handlers: Vec::new(), keep_alive: keep_alive as u64, last_send: Instant::now() };
        let mut flags = 0x02; // Clean Start
        let mut payload = mqtt_str(client_id);
        if let Some((topic, msg, qos, retain)) = will {
            flags |= 0x04 | (qos << 3) | if retain { 0x20 } else { 0 };
            payload.extend(varint(0)); // thuộc tính di chúc (rỗng)
            payload.extend(mqtt_str(topic));
            payload.extend(mqtt_str(msg));
        }
        let mut body = mqtt_str("MQTT");
        body.extend_from_slice(&[5, flags]); // mức giao thức 5 (MQTT 5.0), cờ kết nối
        body.extend_from_slice(&keep_alive.to_be_bytes());
        body.extend(varint(0)); // thuộc tính CONNECT (rỗng)
        body.extend(payload);
        c.send(CONNECT, 0, &body)?;
        let ack = c.wait(CONNACK)?;
        if ack[1] != 0 {
            // mã lý do 0x00: thành công
            return Err(io::Error::other(format!("CONNACK mã lý do 0x{:02X}", ack[1])));
        }
        Ok(c)
    }

    pub fn publish(&mut self, topic: &str, payload: &str, qos: u8, retain: bool) -> io::Result<()> {
        let mut body = mqtt_str(topic);
        if qos > 0 {
            body.extend_from_slice(&self.next_id.to_be_bytes()); // mã gói
            self.next_id += 1;
        }
        body.extend(varint(0)); // thuộc tính PUBLISH (rỗng)
        body.extend_from_slice(payload.as_bytes());
        self.send(PUBLISH, (qos << 1) | retain as u8, &body)?;
        if qos == 1 {
            self.wait(PUBACK)?; // QoS 1: chờ broker xác nhận
        }
        Ok(())
    }

    pub fn subscribe(&mut self, flt: &str, qos: u8, handler: Handler) -> io::Result<u8> {
        self.handlers.push((flt.to_string(), handler)); // đăng ký trước: tin giữ lại có thể đến ngay
        let mut body = self.next_id.to_be_bytes().to_vec();
        self.next_id += 1;
        body.extend(varint(0));
        body.extend(mqtt_str(flt));
        body.push(qos);
        self.send(SUBSCRIBE, 0x2, &body)?;
        let ack = self.wait(SUBACK)?;
        // mã kết quả nằm sau mã gói (2 byte) và thuộc tính (thường rỗng)
        let code = if ack.len() > 3 { ack[3 + ack[2] as usize] } else { *ack.last().unwrap_or(&0x80) };
        if code >= 0x80 {
            return Err(io::Error::other(format!("SUBACK từ chối {flt}: 0x{code:02X}")));
        }
        Ok(code)
    }

    /// RUN_EVENT_LOOP: nhận gói, gọi hàm xử lý; chưa gửi gì trong keep_alive/2 giây thì gửi PINGREQ.
    pub fn run_event_loop(&mut self, on_ping: fn()) -> io::Result<()> {
        loop {
            let half = Duration::from_millis(self.keep_alive * 500);
            let since = self.last_send.elapsed();
            if since >= half {
                self.send(PINGREQ, 0, b"")?;
                continue;
            }
            match self.read_packet(half - since) {
                Ok((PUBLISH, flags, body)) => self.dispatch(flags, &body)?,
                Ok((PINGRESP, _, _)) => on_ping(),
                Ok(_) => {}
                Err(e) if is_timeout(&e) => continue,
                Err(e) => return Err(e),
            }
        }
    }

    pub fn disconnect(&mut self) -> io::Result<()> {
        self.send(DISCONNECT, 0, b"") // mã lý do 0 (đóng bình thường): broker bỏ di chúc
        // socket đóng khi client bị hủy
    }
}
