//! Thiết bị mạng mô phỏng có giao diện dòng lệnh (CLI) qua TCP.
//!
//! Mô phỏng một bộ định tuyến kiểu Telnet để luyện tự động hóa mà không cần thiết bị thật:
//!   - Đăng nhập: "Username: " / "Password: " (mật khẩu lấy từ biến môi trường DEVICE_PASSWORD).
//!   - Dấu nhắc theo chế độ: R1>  R1#  R1(config)#  R1(config-if)#
//!   - Lệnh: enable, terminal length 0, show version, show ip interface brief,
//!     show running-config [interface <tên>], configure terminal, interface <tên>,
//!     description <chữ>, ip address <ip> <mặt_nạ>, [no] shutdown, exit, end, write memory.
//!   - Mỗi dòng lệnh được "vọng lại" (echo) như terminal thật; dòng kết thúc bằng CRLF.
//!   - Khi chưa tắt phân trang, "show running-config" dừng sau 8 dòng với " --More-- ".
//! Trạng thái cấu hình giữ trong bộ nhớ, dùng chung cho mọi phiên (có khóa Mutex).
//! CẢNH BÁO: giống Telnet, mật khẩu đi dạng rõ; thiết bị thật phải dùng SSH.

use crate::ipmask::to_cidr;
use crate::repr;
use std::env;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};
use std::thread;

const HOSTNAME: &str = "R1";
const PAGE: usize = 8; // số dòng mỗi trang khi bật phân trang

#[derive(Clone)]
struct Iface {
    name: &'static str,
    desc: String,
    ip: String,
    mask: String,
    shut: bool,
}

/// Trạng thái ban đầu của thiết bị (giữ thứ tự cổng như dict của bản Python).
static STATE: LazyLock<Mutex<Vec<Iface>>> = LazyLock::new(|| {
    let mk = |name, desc: &str, ip: &str, mask: &str, shut| Iface { name, desc: desc.into(), ip: ip.into(), mask: mask.into(), shut };
    Mutex::new(vec![
        mk("Gi0/0", "Lien ket R2", "10.20.1.225", "255.255.255.252", false),
        mk("Gi0/1", "", "", "", true),
        mk("Gi0/2", "", "", "", true),
    ])
});
static SESSIONS: AtomicUsize = AtomicUsize::new(1);

fn log(text: &str) {
    println!("[thiet-bi] {text}");
}

fn has_iface(name: &str) -> bool {
    STATE.lock().unwrap().iter().any(|i| i.name == name)
}

/// Sinh cấu hình đang chạy từ trạng thái (toàn bộ hoặc một cổng).
fn running_config(only: Option<&str>) -> Vec<String> {
    let mut lines: Vec<String> = if only.is_some() { vec![] } else { vec!["!".into(), format!("hostname {HOSTNAME}"), "!".into()] };
    for i in STATE.lock().unwrap().iter() {
        if only.is_some_and(|o| o != i.name) {
            continue;
        }
        lines.push(format!("interface {}", i.name));
        if !i.desc.is_empty() {
            lines.push(format!(" description {}", i.desc));
        }
        lines.push(if i.ip.is_empty() { " no ip address".into() } else { format!(" ip address {} {}", i.ip, i.mask) });
        if i.shut {
            lines.push(" shutdown".into());
        }
        lines.push("!".into());
    }
    if only.is_none() {
        lines.push("end".into());
    }
    lines
}

fn ip_brief() -> Vec<String> {
    let mut lines = vec![format!("{:<10}{:<14}{:<22}Protocol", "Interface", "IP-Address", "Status")];
    for i in STATE.lock().unwrap().iter() {
        let status = if i.shut { "administratively down" } else { "up" };
        let ip = if i.ip.is_empty() { "unassigned" } else { &i.ip };
        lines.push(format!("{:<10}{:<14}{:<22}{}", i.name, ip, status, if i.shut { "down" } else { "up" }));
    }
    lines
}

fn version() -> Vec<String> {
    let n = STATE.lock().unwrap().len();
    vec!["Thiết bị mô phỏng R1, phần mềm SIM-IOS 1.0".into(), format!("{n} cổng GigabitEthernet")]
}

struct Session {
    w: TcpStream,             // nửa ghi
    rf: BufReader<TcpStream>, // nửa đọc: đọc theo dòng và theo từng byte
    sid: usize,
    mode: &'static str,
    iface: Option<String>,
    paging: bool,
    user: String,
    password: String,
}

/// Lỗi "hết dữ liệu" (bên kia đóng kết nối) dùng chung kiểu io::Error cho gọn.
fn eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "EOF")
}

impl Session {
    fn send(&mut self, text: &str) -> io::Result<()> {
        self.w.write_all(text.as_bytes())
    }

    fn readline(&mut self) -> io::Result<String> {
        let mut buf = Vec::new();
        if self.rf.read_until(b'\n', &mut buf)? == 0 {
            return Err(eof());
        }
        Ok(String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string())
    }

    fn prompt(&mut self) -> io::Result<()> {
        let suffix = match self.mode {
            "user" => ">",
            "enable" => "#",
            "config" => "(config)#",
            _ => "(config-if)#",
        };
        self.send(&format!("{HOSTNAME}{suffix}"))
    }

    /// In nhiều dòng; nếu đang phân trang thì dừng mỗi PAGE dòng chờ một phím bất kỳ.
    fn show(&mut self, lines: Vec<String>) -> io::Result<()> {
        for (k, line) in lines.iter().enumerate().map(|(i, l)| (i + 1, l)) {
            self.send(&format!("{line}\r\n"))?;
            if self.paging && k % PAGE == 0 && k < lines.len() {
                self.send(" --More-- ")?;
                let mut one = [0u8; 1];
                if self.rf.read(&mut one)? == 0 {
                    return Err(eof());
                }
                self.send("\r          \r")?; // xóa chữ --More-- như terminal thật
            }
        }
        Ok(())
    }

    fn login(&mut self) -> io::Result<bool> {
        self.send(&format!("\r\n{HOSTNAME} - thiết bị mô phỏng cho bài thực hành\r\n\r\nUsername: "))?;
        let user = self.readline()?;
        self.send("Password: ")?;
        let pw = self.readline()?; // không vọng lại mật khẩu
        self.send("\r\n")?;
        if user != self.user || pw != self.password {
            log(&format!("phiên #{}: đăng nhập SAI (user={})", self.sid, repr(&user)));
            self.send("% Authentication failed\r\n")?;
            return Ok(false);
        }
        log(&format!("phiên #{}: đăng nhập {user}", self.sid));
        Ok(true)
    }

    fn run(&mut self) -> io::Result<()> {
        if !self.login()? {
            return Ok(());
        }
        loop {
            self.prompt()?;
            let cmd = self.readline()?.trim().to_string();
            self.send(&format!("{cmd}\r\n"))?; // vọng lại dòng lệnh
            if cmd.is_empty() {
                continue;
            }
            if !self.execute(&cmd)? {
                return Ok(());
            }
        }
    }

    fn invalid(&mut self) -> io::Result<()> {
        self.send("% Invalid input detected at '^' marker.\r\n")
    }

    fn execute(&mut self, cmd: &str) -> io::Result<bool> {
        let words: Vec<&str> = cmd.split_whitespace().collect();
        let m = self.mode;
        if cmd == "exit" && (m == "user" || m == "enable") {
            return Ok(false);
        }
        if m == "user" {
            match cmd {
                "enable" => self.mode = "enable",
                "show version" => self.show(version())?,
                _ => self.invalid()?,
            }
        } else if m == "enable" {
            match cmd {
                "terminal length 0" => self.paging = false,
                "show version" => self.show(version())?,
                "show ip interface brief" => self.show(ip_brief())?,
                "show running-config" => self.show(running_config(None))?,
                "configure terminal" | "conf t" => {
                    self.send("Enter configuration commands, one per line.  End with CNTL/Z.\r\n")?;
                    self.mode = "config";
                }
                "write memory" | "wr" => {
                    self.send("Building configuration...\r\n[OK]\r\n")?;
                    log(&format!("phiên #{}: lưu cấu hình (write memory)", self.sid));
                }
                _ if words.len() == 4 && words[..3] == ["show", "running-config", "interface"] => {
                    if has_iface(words[3]) {
                        self.show(running_config(Some(words[3])))?;
                    } else {
                        self.invalid()?;
                    }
                }
                _ => self.invalid()?,
            }
        } else if cmd == "end" {
            self.mode = "enable";
            self.iface = None;
        } else if m == "config" {
            if words[0] == "interface" && words.len() == 2 && has_iface(words[1]) {
                self.mode = "config-if";
                self.iface = Some(words[1].to_string());
            } else if cmd == "exit" {
                self.mode = "enable";
            } else {
                self.invalid()?;
            }
        } else if m == "config-if" {
            self.config_if(cmd, &words)?;
        }
        Ok(true)
    }

    fn config_if(&mut self, cmd: &str, words: &[&str]) -> io::Result<()> {
        if cmd == "exit" {
            self.mode = "config";
            self.iface = None;
            return Ok(());
        }
        let name = self.iface.clone().unwrap_or_default();
        {
            let mut st = STATE.lock().unwrap();
            let i = st.iter_mut().find(|i| i.name == name).unwrap();
            if words[0] == "description" && words.len() > 1 {
                i.desc = cmd["description ".len()..].to_string();
            } else if cmd == "no description" {
                i.desc.clear();
            } else if words.len() == 4 && words[..2] == ["ip", "address"] {
                if to_cidr(words[2], words[3]).is_none() {
                    drop(st);
                    return self.send("% Invalid address\r\n");
                }
                i.ip = words[2].to_string();
                i.mask = words[3].to_string();
            } else if cmd == "shutdown" {
                i.shut = true;
            } else if cmd == "no shutdown" {
                i.shut = false;
            } else {
                drop(st);
                return self.invalid();
            }
        }
        log(&format!("phiên #{}: {name}: {cmd}", self.sid));
        Ok(())
    }
}

fn handle(conn: TcpStream, user: String, password: String) {
    let sid = SESSIONS.fetch_add(1, Ordering::SeqCst);
    let addr = conn.peer_addr().map(|a| a.to_string()).unwrap_or_default();
    let rf = BufReader::new(conn.try_clone().expect("try_clone"));
    let mut s = Session { w: conn, rf, sid, mode: "user", iface: None, paging: true, user, password };
    let _ = s.run(); // EOF hoặc lỗi socket: kết thúc phiên
    drop(s); // đóng kết nối
    log(&format!("phiên #{sid} ({addr}) kết thúc"));
}

pub fn run(args: &[String]) {
    let host = args.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let port: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(9000);
    let user = env::var("DEVICE_USER").unwrap_or_else(|_| "admin".into());
    let password = env::var("DEVICE_PASSWORD").unwrap_or_else(|_| "lab123".into());
    let srv = TcpListener::bind((host.as_str(), port)).unwrap_or_else(|e| {
        eprintln!("[thiet-bi] Không mở được {host}:{port}: {e}");
        process::exit(1)
    });
    ctrlc::set_handler(|| {
        log("Dừng thiết bị");
        process::exit(0);
    })
    .expect("Không cài được hàm bắt Ctrl+C");
    log(&format!("{HOSTNAME} lắng nghe CLI tại {host}:{port}"));
    for conn in srv.incoming().flatten() {
        let (u, p) = (user.clone(), password.clone());
        thread::spawn(move || handle(conn, u, p));
    }
}
