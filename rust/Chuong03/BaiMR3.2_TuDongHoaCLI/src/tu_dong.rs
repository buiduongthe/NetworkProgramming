//! Client tự động hóa cấu hình thiết bị qua CLI (kiểu expect/Netmiko, chỉ dùng socket).
//!
//! Dữ liệu CLI là luồng byte KHÔNG có khung thông điệp, nên client phải "đọc cho tới khi gặp dấu
//! nhắc" (READ_UNTIL) – một dạng định khung bằng dấu phân cách, giống READ_LINE ở Bài 3.2 nhưng
//! dấu phân cách là biểu thức chính quy.
//! Quy trình:
//!   1. Đăng nhập, vào chế độ đặc quyền (enable), tắt phân trang (terminal length 0).
//!   2. Với mỗi cổng trong tệp trạng thái mong muốn: đọc cấu hình hiện tại, phân tích,
//!      so sánh và CHỈ sinh lệnh cho phần khác biệt (lũy đẳng – idempotent).
//!   3. Áp dụng lệnh (hoặc chỉ in ra với --thu), đọc lại để kiểm chứng, lưu cấu hình.

use crate::ipmask::{from_cidr, to_cidr};
use crate::repr;
use regex::Regex;
use serde_json::Value;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process;
use std::time::{Duration, Instant};

/// R1>  R1#  R1(config)#  R1(config-if)#  (so ở cuối bộ đệm)
const PROMPT: &str = r"[\w.-]+(\([\w-]+\))?[>#]$";

/// Lỗi của phiên CLI: thông điệp đã định dạng sẵn (như CliError của bản Python).
struct CliError(String);

impl From<io::Error> for CliError {
    fn from(e: io::Error) -> Self {
        CliError(e.to_string())
    }
}

struct CliSession {
    sock: TcpStream,
    timeout: Duration,
    buf: String,
    prompt: Regex,
}

impl CliSession {
    fn connect(host: &str, port: u16, timeout: Duration) -> Result<CliSession, CliError> {
        let addr = (host, port).to_socket_addrs()?.next().ok_or(CliError(format!("Không phân giải được {host}")))?;
        let sock = TcpStream::connect_timeout(&addr, timeout)?;
        Ok(CliSession { sock, timeout, buf: String::new(), prompt: Regex::new(PROMPT).unwrap() })
    }

    /// READ_UNTIL: đọc dồn vào bộ đệm cho tới khi phần cuối khớp mẫu; trả về phần đã đọc.
    fn read_until(&mut self, pattern: &Regex) -> Result<String, CliError> {
        let deadline = Instant::now() + self.timeout;
        let mut chunk = [0u8; 4096];
        let mut raw: Vec<u8> = Vec::new(); // byte chưa đủ thành ký tự UTF-8 trọn vẹn
        loop {
            let text = self.buf.trim_end_matches(' ').to_string();
            if pattern.is_match(&text) {
                self.buf.clear();
                return Ok(text);
            }
            if self.buf.contains("% Authentication failed") {
                return Err(CliError("Đăng nhập thất bại".into()));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let tail: String = {
                    let v: Vec<char> = self.buf.chars().collect();
                    v[v.len().saturating_sub(60)..].iter().collect()
                };
                return Err(CliError(format!("Hết thời gian chờ dấu nhắc, đã nhận: {}", repr(&tail))));
            }
            self.sock.set_read_timeout(Some(remaining))?;
            let n = match self.sock.read(&mut chunk) {
                Ok(n) => n,
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
                Err(e) => return Err(e.into()),
            };
            if n == 0 {
                return Err(CliError("Thiết bị đóng kết nối".into()));
            }
            raw.extend_from_slice(&chunk[..n]);
            // chỉ giải mã phần UTF-8 hợp lệ; phần đuôi bị cắt giữa ký tự đợi lần đọc sau
            let valid = match std::str::from_utf8(&raw) {
                Ok(_) => raw.len(),
                Err(e) => e.valid_up_to(),
            };
            self.buf.push_str(std::str::from_utf8(&raw[..valid]).unwrap());
            raw.drain(..valid);
        }
    }

    fn send_line(&mut self, line: &str) -> io::Result<()> {
        self.sock.write_all(format!("{line}\n").as_bytes())
    }

    fn login(&mut self, user: &str, password: &str) -> Result<(), CliError> {
        self.read_until(&Regex::new(r"Username:$").unwrap())?;
        self.send_line(user)?;
        self.read_until(&Regex::new(r"Password:$").unwrap())?;
        self.send_line(password)?;
        let p = self.prompt.clone();
        self.read_until(&p)?;
        Ok(())
    }

    /// Gửi một lệnh, chờ dấu nhắc kế tiếp, trả về phần kết quả (bỏ dòng vọng lại và dấu nhắc).
    fn command(&mut self, cmd: &str) -> Result<Vec<String>, CliError> {
        self.send_line(cmd)?;
        let p = self.prompt.clone();
        let text = self.read_until(&p)?.replace("\r\n", "\n");
        let mut out: Vec<String> = text.split('\n').map(String::from).collect();
        if out.first().is_some_and(|l| l.trim() == cmd) {
            out.remove(0); // bỏ dòng vọng lại
        }
        out.pop(); // bỏ dòng dấu nhắc
        if out.iter().any(|l| l.starts_with('%')) {
            return Err(CliError(format!("Thiết bị từ chối lệnh {}: {}", repr(cmd), out[0])));
        }
        Ok(out)
    }

    fn close(&mut self) {
        let _ = self.send_line("exit");
        let _ = self.sock.shutdown(std::net::Shutdown::Both);
    }
}

/// Cấu hình một cổng, cùng dạng với tệp mong muốn.
#[derive(PartialEq)]
struct IfConf {
    mo_ta: String,
    ip: String,
    bat: bool,
}

/// Phân tích khối 'interface ...' thành IfConf.
fn parse_interface(lines: &[String]) -> IfConf {
    let mut cur = IfConf { mo_ta: String::new(), ip: String::new(), bat: true };
    for line in lines {
        let line = line.trim();
        if let Some(d) = line.strip_prefix("description ") {
            cur.mo_ta = d.to_string();
        } else if line.starts_with("ip address ") {
            let w: Vec<&str> = line.split_whitespace().collect();
            cur.ip = to_cidr(w[2], w[3]).unwrap_or_default();
        } else if line == "shutdown" {
            cur.bat = false;
        }
    }
    cur
}

/// Chỉ sinh lệnh cho phần khác biệt giữa cấu hình hiện tại và mong muốn.
fn diff_commands(cur: &IfConf, want: &IfConf) -> Vec<String> {
    let mut cmds = Vec::new();
    if cur.mo_ta != want.mo_ta {
        cmds.push(if want.mo_ta.is_empty() { "no description".into() } else { format!("description {}", want.mo_ta) });
    }
    if cur.ip != want.ip {
        if let Some((ip, mask)) = from_cidr(&want.ip) {
            cmds.push(format!("ip address {ip} {mask}"));
        }
    }
    if cur.bat != want.bat {
        cmds.push(if want.bat { "no shutdown".into() } else { "shutdown".into() });
    }
    cmds
}

fn py_bool(b: bool) -> &'static str {
    if b { "True" } else { "False" }
}

fn work(cli: &mut CliSession, want: &Value, user: &str, password: &str, dry_run: bool) -> Result<(), CliError> {
    cli.login(user, password)?;
    cli.command("enable")?;
    cli.command("terminal length 0")?; // tắt --More-- để đọc trọn kết quả
    println!("[tu-dong] {}", cli.command("show version")?[0]);
    let mut total = 0;
    for item in want["cong"].as_array().cloned().unwrap_or_default() {
        let name = item["ten"].as_str().unwrap_or("").to_string();
        let target = IfConf {
            mo_ta: item["mo_ta"].as_str().unwrap_or("").into(),
            ip: item["ip"].as_str().unwrap_or("").into(),
            bat: item["bat"].as_bool().unwrap_or(true),
        };
        let cur = parse_interface(&cli.command(&format!("show running-config interface {name}"))?);
        let cmds = diff_commands(&cur, &target);
        if cmds.is_empty() {
            println!("[tu-dong] {name}: đã đúng, không cần thay đổi");
            continue;
        }
        println!("[tu-dong] {name}: cần {} lệnh", cmds.len());
        for c in &cmds {
            println!("           + {c}");
        }
        if dry_run {
            continue;
        }
        cli.command("configure terminal")?;
        cli.command(&format!("interface {name}"))?;
        for c in &cmds {
            cli.command(c)?;
        }
        cli.command("end")?;
        let after = parse_interface(&cli.command(&format!("show running-config interface {name}"))?);
        if after != target {
            // đọc lại để kiểm chứng
            return Err(CliError(format!(
                "{name}: cấu hình sau khi áp dụng chưa khớp: {{'mo_ta': {}, 'ip': {}, 'bat': {}}}",
                repr(&after.mo_ta),
                repr(&after.ip),
                py_bool(after.bat)
            )));
        }
        total += cmds.len();
    }
    if total > 0 {
        cli.command("write memory")?;
        println!("[tu-dong] Đã áp dụng và kiểm chứng {total} lệnh, đã lưu cấu hình");
    } else if !dry_run {
        println!("[tu-dong] Thiết bị đã ở đúng trạng thái mong muốn (lũy đẳng)");
    }
    println!("[tu-dong] show ip interface brief:");
    for line in cli.command("show ip interface brief")? {
        println!("  {line}");
    }
    Ok(())
}

pub fn run(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Cách dùng: tudong <host> <port> <muc_tieu.json> [--thu]");
        process::exit(2);
    }
    let (host, path) = (&args[0], &args[2]);
    let port: u16 = args[1].parse().unwrap_or_else(|_| {
        eprintln!("Cổng không hợp lệ: {}", args[1]);
        process::exit(2)
    });
    let dry_run = args[3..].iter().any(|a| a == "--thu");
    let want: Value = match std::fs::read_to_string(path).map(|t| serde_json::from_str(&t)) {
        Ok(Ok(v)) => v,
        _ => {
            eprintln!("Không đọc được tệp mong muốn {path}");
            process::exit(2)
        }
    };
    let user = std::env::var("DEVICE_USER").unwrap_or_else(|_| "admin".into());
    let password = std::env::var("DEVICE_PASSWORD").unwrap_or_default();
    println!("[tu-dong] Kết nối {host}:{port}, tài khoản {user}");
    if dry_run {
        println!("[tu-dong] Chế độ chạy thử: chỉ in lệnh, không áp dụng");
    }
    let mut cli = match CliSession::connect(host, port, Duration::from_secs(5)) {
        Ok(c) => c,
        Err(CliError(e)) => {
            println!("[tu-dong] LỖI: {e}");
            process::exit(1)
        }
    };
    let r = work(&mut cli, &want, &user, &password, dry_run);
    cli.close();
    if let Err(CliError(e)) = r {
        println!("[tu-dong] LỖI: {e}");
        process::exit(1);
    }
}
