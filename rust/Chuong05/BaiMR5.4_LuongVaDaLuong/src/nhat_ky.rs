//! Nhật ký theo luồng: tên luồng (thread::current().name()) + biến cục bộ luồng (thread_local!).
//!
//! Mỗi dòng: "[  0.123 s] tải-1 yc=1      | nội dung". Ngữ cảnh "yc=..." nằm trong biến cục bộ luồng:
//! mỗi luồng có bản sao riêng, đặt một lần rồi mọi lời gọi log() sau đó tự có.
//! Một Mutex bảo đảm hai luồng không in xen lẫn trên một dòng.

use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::Instant;

static T0: LazyLock<Instant> = LazyLock::new(Instant::now);
static KHOA_IN: Mutex<()> = Mutex::new(());

thread_local! {
    // biến cục bộ luồng: RefCell vì chỉ luồng sở hữu mới đọc/ghi được, không cần khóa
    static NGU_CANH: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn khoi_dong() {
    LazyLock::force(&T0);
}

pub fn dat_ngu_canh(s: &str) {
    NGU_CANH.with(|c| *c.borrow_mut() = s.to_string());
}

pub fn bay_gio() -> f64 {
    T0.elapsed().as_secs_f64()
}

/// Tên luồng hiện tại; luồng chính của Rust tên "main" → hiển thị "chính".
pub fn ten_luong() -> String {
    match thread::current().name() {
        Some("main") | None => "chính".to_string(),
        Some(t) => t.to_string(),
    }
}

pub fn log(msg: &str) {
    let ctx = NGU_CANH.with(|c| c.borrow().clone());
    let ai = format!("{} {}", ten_luong(), ctx).trim().to_string();
    let _g = KHOA_IN.lock().unwrap();
    println!("[{:7.3} s] {:<16}| {}", bay_gio(), ai, msg);
}

pub fn say(msg: &str) {
    let _g = KHOA_IN.lock().unwrap();
    println!("{msg}");
}

/// Gửi một dòng (thêm \n) qua socket.
pub fn gui_dong(s: &mut TcpStream, line: &str) -> std::io::Result<()> {
    s.write_all(format!("{line}\n").as_bytes())
}

/// Đọc một dòng (bỏ \n); trả chuỗi rỗng khi đối phương đóng kết nối.
pub fn doc_dong(r: &mut BufReader<TcpStream>) -> String {
    let mut line = String::new();
    let _ = r.read_line(&mut line);
    line.trim_end().to_string()
}

/// Chạy một closure trên luồng mới có tên.
pub fn spawn_ten<F, T>(ten: &str, f: F) -> thread::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    thread::Builder::new()
        .name(ten.to_string())
        .spawn(f)
        .expect("không tạo được luồng")
}
