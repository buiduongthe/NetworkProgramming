//! Server truyền nhiều tệp: mỗi kết nối phục vụ một lượt tải xuống/tải lên.
//!
//! Giao thức: nhận một khung JSON điều khiển, rồi truyền luồng byte (xem khung.rs).
//!   - download: {"op":"download","name":TÊN,"offset":OFF}
//!        -> trả {"ok":true,"size":TỔNG,"sha256":HEX} rồi gửi byte [OFF..hết].
//!   - upload:   {"op":"upload","name":TÊN,"offset":OFF,"size":TỔNG,"sha256":HEX}
//!        -> trả {"ok":true} rồi nhận byte, ghi nối vào TÊN.part từ OFF; xong thì kiểm SHA-256,
//!           đổi tên thành TÊN, trả {"ok":true,"sha256_ok":...}.
//! Server KHÔNG tự giới hạn tốc độ – client điều tiết, TCP tự tạo áp lực ngược.

use crate::khung::{doc_du, gui_khung, nhan_khung, sha256_tep, CHUNK};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process;
use std::thread;

/// Chỉ lấy phần tên tệp (như os.path.basename) để client không ghi ra ngoài thư mục lưu.
fn basename(name: &str) -> String {
    Path::new(name).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn xu_ly(conn: &TcpStream, kho: &Path) -> io::Result<()> {
    let req = nhan_khung(conn)?;
    let name = basename(req["name"].as_str().unwrap_or(""));
    let off = req["offset"].as_u64().unwrap_or(0);
    match req["op"].as_str() {
        Some("download") => {
            let path = kho.join(&name);
            if !path.is_file() {
                return gui_khung(conn, &json!({"ok": false, "loi": format!("Không có tệp {name}")}));
            }
            let size = fs::metadata(&path)?.len();
            gui_khung(conn, &json!({"ok": true, "size": size, "sha256": sha256_tep(&path)?}))?;
            let mut f = fs::File::open(&path)?;
            f.seek(SeekFrom::Start(off))?;
            let mut buf = vec![0u8; CHUNK];
            let mut w = conn;
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    return Ok(());
                }
                w.write_all(&buf[..n])?;
            }
        }
        Some("upload") => {
            let total = req["size"].as_u64().unwrap_or(0);
            let part: PathBuf = kho.join(format!("{name}.part"));
            gui_khung(conn, &json!({"ok": true}))?;
            let mut f = if off > 0 && part.exists() {
                OpenOptions::new().write(true).open(&part)?
            } else {
                fs::File::create(&part)?
            };
            f.seek(SeekFrom::Start(off))?;
            let mut nhan = off;
            while nhan < total {
                let b = doc_du(conn, CHUNK.min((total - nhan) as usize))?;
                f.write_all(&b)?;
                nhan += b.len() as u64;
            }
            drop(f);
            let sha_ok = req["sha256"].as_str() == Some(sha256_tep(&part)?.as_str());
            if sha_ok {
                fs::rename(&part, kho.join(&name))?;
            }
            gui_khung(conn, &json!({"ok": true, "sha256_ok": sha_ok}))
        }
        _ => gui_khung(conn, &json!({"ok": false, "loi": "op lạ"})),
    }
}

/// Phục vụ một kết nối; client ngắt giữa chừng (pause/stop) là bình thường nên bỏ qua lỗi.
pub fn phuc_vu(conn: TcpStream, kho: PathBuf) {
    let _ = xu_ly(&conn, &kho);
}

pub fn run(host: &str, port: u16, kho: &str) {
    if let Err(e) = fs::create_dir_all(kho) {
        eprintln!("[server] Không tạo được thư mục {kho}: {e}");
        process::exit(1);
    }
    let srv = TcpListener::bind((host, port)).unwrap_or_else(|e| {
        eprintln!("[server] Không mở được {host}:{port}: {e}");
        process::exit(1)
    });
    ctrlc::set_handler(|| {
        println!("[server] Dừng.");
        process::exit(0);
    })
    .expect("Không cài được hàm bắt Ctrl+C");
    println!("[server] Nghe tại {host}:{port}, thư mục lưu: {kho}");
    for conn in srv.incoming().flatten() {
        let k = PathBuf::from(kho);
        thread::spawn(move || phuc_vu(conn, k));
    }
}
