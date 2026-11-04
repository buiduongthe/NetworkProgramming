//! Ứng dụng tra cứu có giao diện – cửa sổ vẽ bằng ký tự trên cửa sổ dòng lệnh (terminal UI).
//!
//! Cài đặt Mã giả 5.3 (cách 1: luồng nền và UI_INVOKE) cho nút Kết nối và Mã giả 5.4
//! (cách 2: trình xử lý sự kiện bất đồng bộ) cho nút Tra cứu – Mục 5.3.2, Giáo trình Lập trình mạng.
//! Kế thừa: Mã giả 2.2 (SAFE_CONNECT, bản chặn), 3.8 (một lượt tra cứu), 5.1 (khung JSON bất đồng bộ).
//!
//! Mô hình luồng giao diện đơn (Mục 5.3.1):
//! - Luồng chính là luồng giao diện. Nó chạy một vòng lặp thông điệp (`run_gui`) trên vòng lặp sự kiện
//!   tokio một luồng: mỗi vòng nhận một thông điệp (nhịp đồng hồ, phím bấm, việc do UI_INVOKE gửi tới),
//!   xử lý rồi vẽ lại cửa sổ.
//! - Trạng thái cửa sổ nằm trong `Rc<RefCell<Window>>`. Kiểu `Rc` không được phép chuyển sang luồng khác,
//!   nên trình biên dịch Rust bảo đảm chỉ luồng giao diện đọc, sửa được các "điều khiển".
//! - UI_INVOKE: luồng nền gửi một closure vào kênh `mpsc`; vòng lặp thông điệp chạy closure đó trên
//!   luồng giao diện (giống `Control.BeginInvoke`, `SwingUtilities.invokeLater`).
//! - Cách 2: trình xử lý nút Tra cứu là một tác vụ `spawn_local` chạy trên chính luồng giao diện, nên sau
//!   mỗi `.await` mã chạy tiếp trên luồng giao diện và cập nhật điều khiển trực tiếp.

use crate::khung_json_async::{recv_json_async, safe_connect, send_json_async, CancelToken, FrameError};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::io::{self, BufRead, IsTerminal, Write};
use std::rc::Rc;
use std::thread;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

/// Trạng thái cửa sổ và các điều khiển (chỉ luồng giao diện được truy cập).
pub struct Window {
    pub txt_host: String,
    pub txt_port: String,
    pub txt_term: String,
    pub btn_connect_enabled: bool,
    pub btn_lookup_enabled: bool,
    pub txt_result: String,
    pub lbl_status: String,
    pub ticks: u64,                 // số lần đồng hồ giao diện được vẽ lại
    pub conn: Option<TcpStream>,    // conn: trường của cửa sổ
    pub ct: CancelToken,            // ct: bị kích hoạt khi đóng cửa sổ
    pub alive: bool,
    pub ui: UiHandle,               // để trình xử lý giao việc cho luồng nền
    pub live_redraw: bool,          // vẽ lại liên tục trên terminal (chế độ tương tác)
}

/// Cửa sổ dùng chung trong luồng giao diện.
pub type Win = Rc<RefCell<Window>>;

/// Một việc cần chạy trên luồng giao diện; tạo được ở bất kỳ luồng nào (Send).
type UiJob = Box<dyn FnOnce(&Win) + Send>;

/// UI_INVOKE(proc, args): gọi được từ luồng nền.
#[derive(Clone)]
pub struct UiHandle {
    tx: mpsc::UnboundedSender<UiJob>,
}

impl UiHandle {
    pub fn invoke(&self, job: impl FnOnce(&Win) + Send + 'static) {
        let _ = self.tx.send(Box::new(job));
    }
}

/// Tạo cửa sổ và kênh UI_INVOKE.
pub fn new_window(host: &str, port: u16, live_redraw: bool) -> (Win, mpsc::UnboundedReceiver<UiJob>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let w = Window {
        txt_host: host.to_string(),
        txt_port: port.to_string(),
        txt_term: String::new(),
        btn_connect_enabled: true,
        btn_lookup_enabled: false,
        txt_result: String::new(),
        lbl_status: "Chưa kết nối".to_string(),
        ticks: 0,
        conn: None,
        ct: CancelToken::new(),
        alive: true,
        ui: UiHandle { tx },
        live_redraw,
    };
    (Rc::new(RefCell::new(w)), rx)
}

// ---- vẽ cửa sổ ---------------------------------------------------------------------------

const WIDTH: usize = 64; // số ký tự bên trong khung

/// Cắt hoặc thêm khoảng trắng để chuỗi dài đúng n ký tự.
fn fit(text: &str, n: usize) -> String {
    let mut s: String = text.chars().take(n).collect();
    let len = s.chars().count();
    s.push_str(&" ".repeat(n - len));
    s
}

fn button(label: &str, enabled: bool) -> String {
    if enabled {
        format!("[ {label} ]")
    } else {
        format!("[░{label}░]") // nút bị tắt
    }
}

/// Vẽ cửa sổ thành các dòng ký tự.
pub fn render(w: &Window) -> Vec<String> {
    let title = " Tra cứu thuật ngữ – Bài 5.3 ";
    let top = format!("┌─{title}{}┐", "─".repeat(WIDTH - 1 - title.chars().count()));
    let row = |s: String| format!("│{}│", fit(&s, WIDTH));
    let clock = format!("Đồng hồ giao diện: {:.1} s", w.ticks as f64 / 10.0);
    let status_w = WIDTH - 2 - clock.chars().count();
    let mut lines = vec![
        top,
        row(format!(
            " Máy chủ: {}  Cổng: {}  {}",
            fit(&w.txt_host, 15),
            fit(&w.txt_port, 6),
            button("Kết nối", w.btn_connect_enabled)
        )),
        row(format!(" Thuật ngữ: {}  {}", fit(&w.txt_term, 31), button("Tra cứu", w.btn_lookup_enabled))),
        row(format!(" ┌{}┐", "─".repeat(WIDTH - 4))),
    ];
    // ô kết quả: tối đa 2 dòng, ngắt theo số ký tự
    let chars: Vec<char> = w.txt_result.chars().collect();
    for i in 0..2 {
        let part: String = chars.iter().skip(i * (WIDTH - 4)).take(WIDTH - 4).collect();
        lines.push(row(format!(" │{}│", fit(&part, WIDTH - 4))));
    }
    lines.push(row(format!(" └{}┘", "─".repeat(WIDTH - 4))));
    lines.push(row(format!(" {}{clock} ", fit(&w.lbl_status, status_w))));
    lines.push(format!("└{}┘", "─".repeat(WIDTH)));
    lines
}

/// Vẽ lại cửa sổ ở đầu màn hình terminal (giữ nguyên dòng nhập lệnh bên dưới).
fn redraw(w: &Window) {
    let mut out = io::stdout().lock();
    let _ = write!(out, "\x1b7\x1b[1;1H"); // lưu vị trí con trỏ, về góc trên
    for line in render(w) {
        let _ = write!(out, "{line}\x1b[K\r\n");
    }
    let _ = write!(out, "\x1b8"); // trả con trỏ về dòng nhập lệnh
    let _ = out.flush();
}

// ---- tiện ích giao diện -------------------------------------------------------------------

pub fn show_status(win: &Win, text: &str) {
    win.borrow_mut().lbl_status = text.to_string();
}

// ---- Mã giả 5.3: cách (1) luồng nền và UI_INVOKE ------------------------------------------

/// ON_CONNECT_CLICK – chạy trên luồng giao diện.
pub fn on_connect_click(win: &Win) {
    let mut w = win.borrow_mut();
    if !w.btn_connect_enabled {
        return;
    }
    let port: u16 = match w.txt_port.trim().parse() {
        // TO_INT(txtPort.text)
        Ok(p) if p > 0 => p,
        _ => {
            w.lbl_status = format!("Lỗi: Cổng không hợp lệ: {}", w.txt_port);
            return;
        }
    };
    w.btn_connect_enabled = false; // btnConnect.enabled ← FALSE
    let (host, ui) = (w.txt_host.clone(), w.ui.clone());
    thread::spawn(move || connect_worker(host, port, ui)); // SPAWN_THREAD(CONNECT_WORKER, ...)
}

/// CONNECT_WORKER – chạy trên luồng nền; chỉ chạm vào giao diện qua UI_INVOKE.
fn connect_worker(host: String, port: u16, ui: UiHandle) {
    match safe_connect(&host, port, 3000) {
        // chặn luồng nền, không chặn giao diện
        Ok(conn) => {
            ui.invoke(move |win| attach(win, conn));
            ui.invoke(|win| show_status(win, "Đã kết nối")); // UI_INVOKE(SHOW_STATUS, "Đã kết nối")
        }
        Err(e) => {
            let code = e.code;
            ui.invoke(move |win| show_status(win, &format!("Lỗi: {code}"))); // UI_INVOKE(SHOW_STATUS, ...)
            ui.invoke(|win| win.borrow_mut().btn_connect_enabled = true); // UI_INVOKE(SET_ENABLED, btnConnect, TRUE)
        }
    }
}

/// Giữ socket đã kết nối làm trường `conn` của cửa sổ, chuyển sang dạng bất đồng bộ của tokio.
fn attach(win: &Win, conn: std::net::TcpStream) {
    let mut w = win.borrow_mut();
    let ok = conn.set_nonblocking(true).is_ok();
    match TcpStream::from_std(conn) {
        Ok(s) if ok => {
            w.conn = Some(s);
            w.btn_lookup_enabled = true;
        }
        _ => w.lbl_status = "Lỗi: không dùng được socket".to_string(),
    }
}

// ---- Mã giả 5.4: cách (2) trình xử lý sự kiện bất đồng bộ ---------------------------------

/// ON_LOOKUP_CLICK – tác vụ chạy trên luồng giao diện (spawn_local).
pub async fn on_lookup_click(win: Win) {
    // Lấy kết nối ra khỏi cửa sổ trong lúc dùng (nút đã tắt nên không ai dùng cùng lúc);
    // không giữ `borrow_mut` qua `.await` để giao diện vẫn vẽ lại được trong lúc chờ.
    let (conn, term, ct) = {
        let mut w = win.borrow_mut();
        if !w.btn_lookup_enabled {
            return;
        }
        w.btn_lookup_enabled = false; // chống bấm lặp
        w.lbl_status = "Đang tra cứu...".to_string();
        (w.conn.take(), w.txt_term.clone(), w.ct.clone())
    };
    let Some(mut conn) = conn else { return };
    let req = json!({"op": "lookup", "term": term});
    let result: Result<Value, FrameError> = async {
        send_json_async(&mut conn, &req, &ct).await?;
        recv_json_async(&mut conn, &ct).await // UI không bị chặn
    }
    .await;
    // đã ở luồng giao diện: cập nhật điều khiển trực tiếp, không cần UI_INVOKE
    let mut w = win.borrow_mut();
    match result {
        Ok(resp) => {
            w.txt_result = if resp["status"] == 200 {
                resp["text"].as_str().unwrap_or("").to_string()
            } else {
                format!("({}) {}", resp["status"], resp["error"].as_str().unwrap_or(""))
            };
            w.lbl_status = "Xong".to_string();
        }
        Err(FrameError::Cancelled) => return, // cửa sổ đã đóng: không chạm vào điều khiển nữa
        Err(e) => w.lbl_status = format!("Lỗi: {e}"),
    }
    // FINALLY
    if w.alive {
        w.conn = Some(conn);
        w.btn_lookup_enabled = true;
    }
}

/// Đóng cửa sổ: hủy tác vụ của cửa sổ, đóng kết nối (Mục 5.3.5).
pub fn close(win: &Win) {
    let mut w = win.borrow_mut();
    w.alive = false;
    w.ct.cancel(); // CANCEL(ct)
    w.conn = None; // CLOSE(conn)
}

// ---- vòng lặp thông điệp ------------------------------------------------------------------

/// Bàn phím (chế độ tương tác): luồng phụ đọc từng dòng lệnh và gửi vào kênh.
pub fn start_keyboard() -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                return;
            }
        }
        let _ = tx.send("q".to_string()); // hết dữ liệu vào: đóng cửa sổ
    });
    rx
}

/// Xử lý một dòng lệnh: k [host] [port] = bấm Kết nối; t <thuật ngữ> = nhập rồi bấm Tra cứu; q = đóng.
fn handle_key(win: &Win, line: &str) {
    let mut parts = line.trim().splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").trim().to_string();
    match cmd {
        "k" => {
            let mut it = rest.split_whitespace();
            {
                let mut w = win.borrow_mut();
                if let Some(h) = it.next() {
                    w.txt_host = h.to_string();
                }
                if let Some(p) = it.next() {
                    w.txt_port = p.to_string();
                }
            }
            on_connect_click(win);
        }
        "t" => {
            win.borrow_mut().txt_term = rest;
            tokio::task::spawn_local(on_lookup_click(win.clone()));
        }
        "q" => close(win),
        _ => {}
    }
    if win.borrow().live_redraw {
        print!("\x1b[11;1H\x1b[K> "); // xóa dòng nhập lệnh
        let _ = io::stdout().flush();
    }
}

/// Vòng lặp thông điệp của luồng giao diện; kết thúc khi cửa sổ đóng.
pub async fn run_gui(win: Win, mut ui_rx: mpsc::UnboundedReceiver<UiJob>, mut keys: mpsc::UnboundedReceiver<String>) {
    let mut tick = interval(Duration::from_millis(100)); // đồng hồ: chứng tỏ giao diện không đứng hình
    if win.borrow().live_redraw {
        print!("\x1b[2J\x1b[10;1HLệnh: k [máy] [cổng] = Kết nối | t <thuật ngữ> = Tra cứu | q = Đóng\r\n\x1b[11;1H> ");
        redraw(&win.borrow());
    }
    while win.borrow().alive {
        tokio::select! {
            Some(job) = ui_rx.recv() => job(&win),          // việc do UI_INVOKE gửi tới
            Some(line) = keys.recv() => handle_key(&win, &line),
            _ = tick.tick() => win.borrow_mut().ticks += 1,
        }
        let w = win.borrow();
        if w.live_redraw && w.alive {
            redraw(&w);
        }
    }
    if win.borrow().live_redraw {
        print!("\x1b[12;1H\r\n");
    }
}

/// Chạy ứng dụng ở chế độ tương tác trên terminal.
pub async fn run(host: &str, port: u16) {
    let live = io::stdout().is_terminal();
    let (win, ui_rx) = new_window(host, port, live);
    let keys = start_keyboard();
    run_gui(win, ui_rx, keys).await;
}
