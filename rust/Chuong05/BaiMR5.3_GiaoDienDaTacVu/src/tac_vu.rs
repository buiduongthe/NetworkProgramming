//! Các tác vụ mạng (tác vụ tokio) và bộ giới hạn đồng thời đổi được khi đang chạy.
//!
//! Hủy trong Rust async: một future chỉ chạy khi được poll; khi người dùng bấm Hủy, ui.rs dùng
//! tokio::select! giữa future của tác vụ và kênh hủy – nhánh hủy thắng thì future tác vụ bị BỎ (drop),
//! kéo theo mọi socket nó đang giữ bị đóng ngay. Không cần cờ kiểm tra thủ công như luồng.

use crate::http;
use std::collections::VecDeque;
use std::io;
use std::sync::Mutex;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::oneshot;

/// Hàm báo tiến trình: (phần trăm, chữ mô tả).
pub type Progress = Box<dyn Fn(i64, String) + Send + Sync>;

struct LimState {
    limit: usize,
    running: usize,
    queue: VecDeque<(u64, oneshot::Sender<()>)>, // (mã tác vụ, "vé" để báo tới lượt)
}

/// Giới hạn số tác vụ chạy cùng lúc, cấp lượt theo đúng thứ tự xếp hàng (FIFO).
/// enter() được gọi ngay khi tạo tác vụ (theo thứ tự bấm nút) và trả về một đầu nhận "vé";
/// vé được gửi khi tác vụ được vào chạy. Giới hạn đổi được khi đang chạy (thanh trượt).
pub struct Limiter {
    st: Mutex<LimState>,
}

impl Limiter {
    pub fn new(n: usize) -> Limiter {
        Limiter { st: Mutex::new(LimState { limit: n, running: 0, queue: VecDeque::new() }) }
    }

    /// Cấp lượt cho các vé đầu hàng đợi khi còn chỗ.
    fn grant(st: &mut LimState) {
        while st.running < st.limit {
            let Some((_, tx)) = st.queue.pop_front() else { break };
            st.running += 1;
            let _ = tx.send(());
        }
    }

    pub fn get(&self) -> usize {
        self.st.lock().unwrap().limit
    }

    pub fn set(&self, n: usize) {
        let mut st = self.st.lock().unwrap();
        st.limit = n;
        Self::grant(&mut st);
    }

    pub fn enter(&self, id: u64) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let mut st = self.st.lock().unwrap();
        st.queue.push_back((id, tx));
        Self::grant(&mut st);
        rx
    }

    /// Gọi khi tác vụ bị hủy lúc đang chờ lượt: rời hàng đợi, hoặc trả lại lượt nếu vừa được cấp.
    pub fn leave(&self, id: u64) {
        let mut st = self.st.lock().unwrap();
        if let Some(i) = st.queue.iter().position(|(q, _)| *q == id) {
            st.queue.remove(i); // vẫn đang xếp hàng: rời hàng
        } else {
            st.running -= 1; // vừa được cấp lượt đúng lúc bị hủy: trả lại lượt
            Self::grant(&mut st);
        }
    }

    pub fn release(&self) {
        let mut st = self.st.lock().unwrap();
        st.running -= 1;
        Self::grant(&mut st);
    }
}

/// Tải /tep?kb=..&tre=.. qua HTTP, đọc từng phần, báo tiến trình theo Content-Length.
pub async fn download(addr: &str, kb: u64, delay: u64, progress: &Progress) -> io::Result<String> {
    let (mut r, total) = http::get(addr, &format!("/tep?kb={kb}&tre={delay}")).await?;
    let mut got = 0u64;
    let mut buf = vec![0u8; 16 * 1024];
    while got < total {
        let n = r.read(&mut buf).await?;
        if n == 0 {
            return Err(io::Error::other("máy chủ đóng kết nối sớm"));
        }
        got += n as u64;
        progress((got * 100 / total) as i64, format!("{}/{} KB", got / 1024, total / 1024));
    }
    Ok(format!("xong {} KB", total / 1024))
}

/// Kiểm tra lần lượt từng cổng TCP (như Bài 1.2): kết nối được = mở.
pub async fn check_ports(host: &str, ports: &[u16], progress: &Progress) -> io::Result<String> {
    let mut opened = 0;
    for (i, p) in ports.iter().enumerate() {
        let ok = matches!(tokio::time::timeout(Duration::from_millis(300), TcpStream::connect((host, *p))).await, Ok(Ok(_)));
        if ok {
            opened += 1;
        }
        tokio::time::sleep(Duration::from_millis(300)).await; // quét chậm cho dễ quan sát
        progress(((i + 1) * 100 / ports.len()) as i64, format!("cổng {p} {}", if ok { "mở" } else { "đóng" }));
    }
    Ok(format!("{opened} mở, {} đóng", ports.len() - opened))
}

/// Tra cứu tên qua máy chủ "DNS" giả lập trên UDP, mỗi tên chờ tối đa 1 giây.
pub async fn lookup_names(addr: &str, names: &[&str], progress: &Progress) -> io::Result<String> {
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.connect(addr).await?;
    let mut found = 0;
    let mut buf = [0u8; 512];
    for (i, name) in names.iter().enumerate() {
        sock.send(format!("A {name}").as_bytes()).await?;
        let answer = match tokio::time::timeout(Duration::from_secs(1), sock.recv(&mut buf)).await {
            Ok(Ok(n)) => String::from_utf8_lossy(&buf[..n]).into_owned(),
            _ => format!("{name} (hết thời gian chờ)"),
        };
        if !answer.contains("NXDOMAIN") {
            found += 1;
        }
        progress(((i + 1) * 100 / names.len()) as i64, answer);
    }
    Ok(format!("{found}/{} tên có địa chỉ", names.len()))
}
