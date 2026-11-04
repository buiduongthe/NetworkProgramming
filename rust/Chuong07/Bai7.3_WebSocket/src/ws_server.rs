//! Bài 7.3 – Server WebSocket tối giản (tokio).
//!
//! Cài đặt Mã giả 7.3 (WS_ACCEPT – bắt tay) và Mã giả 7.4 (WS_READ_FRAME – đọc khung), Mục 7.3.2.
//! Kế thừa: Mã giả 7.2 (đọc dòng đầu, READ_HEADERS, RESPOND), 3.4 (READ_EXACT), 6.3 (mỗi kết nối một tác vụ).
//! Server gửi trả khung văn bản "echo: …", trả lời ping bằng pong, đáp khung đóng; khung vi phạm bị đóng
//! với mã 1009 (quá lớn) hoặc 1002 (khung từ client không che).
//!
//! Chạy:  cargo run -- server [host] [port]      (mặc định 0.0.0.0 8080)
use crate::{accept_key, exit_with, port_arg};
use base64::Engine;
use std::collections::HashMap;
use std::fmt;
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

const MAX_FRAME: u64 = 1048576; // 1 MiB (Mã giả 3.6)

/// Lỗi của một phiên: vi phạm giao thức hoặc kết nối đóng đột ngột.
enum WsError {
    Protocol(String),
    Io(io::Error),
}

impl From<io::Error> for WsError {
    fn from(e: io::Error) -> Self {
        WsError::Io(e)
    }
}

impl fmt::Display for WsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WsError::Protocol(m) => write!(f, "{m}"),
            WsError::Io(e) => write!(f, "{e}"),
        }
    }
}

type Conn = BufReader<TcpStream>; // bộ đọc có bộ đệm; ghi qua conn.get_mut()

async fn read_line_async(r: &mut Conn) -> Result<String, WsError> {
    let mut line = Vec::new();
    (&mut *r).take(8193).read_until(b'\n', &mut line).await?;
    if line.is_empty() {
        return Err(WsError::Io(io::Error::new(io::ErrorKind::UnexpectedEof, "kết nối đóng")));
    }
    if line.len() > 8192 {
        return Err(WsError::Protocol("Dòng quá dài".into()));
    }
    while line.last() == Some(&b'\n') || line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

async fn read_headers_async(r: &mut Conn) -> Result<HashMap<String, String>, WsError> {
    let mut hdr = HashMap::new();
    loop {
        let line = read_line_async(r).await?;
        if line.is_empty() {
            return Ok(hdr);
        }
        let (k, v) = line.split_once(':').unwrap_or((&line, ""));
        hdr.insert(k.trim().to_lowercase(), v.trim().to_string());
    }
}

/// RESPOND(conn, 400, PROBLEM(400, title)) của Mã giả 7.2.
async fn respond_400(w: &mut TcpStream, title: &str) -> io::Result<()> {
    let b = format!(r#"{{"type":"about:blank","title":"{title}","status":400}}"#);
    let h = format!(
        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/problem+json; charset=utf-8\r\nContent-Length: {}\r\n\r\n",
        b.len()
    );
    w.write_all((h + &b).as_bytes()).await
}

/// Mã giả 7.3: kiểm tra yêu cầu nâng cấp và gửi 101 kèm Sec-WebSocket-Accept.
async fn ws_accept(w: &mut TcpStream, hdr: &HashMap<String, String>) -> io::Result<bool> {
    let key = hdr.get("sec-websocket-key").map(String::as_str).unwrap_or("");
    // khóa của client: Base64 của đúng 16 byte (giải mã lỗi coi như độ dài 0)
    let key_len = base64::engine::general_purpose::STANDARD.decode(key).map(|k| k.len()).unwrap_or(0);
    let upgrade = hdr.get("upgrade").map(|v| v.to_lowercase()).unwrap_or_default();
    if upgrade != "websocket" || hdr.get("sec-websocket-version").map(String::as_str) != Some("13") || key_len != 16 {
        respond_400(w, "Sai WebSocket").await?;
        return Ok(false);
    }
    let acc = accept_key(key); // BASE64(SHA1(key + WS_GUID))
    let mut h = String::from("HTTP/1.1 101 Switching Protocols\r\n");
    h += "Upgrade: websocket\r\nConnection: Upgrade\r\n";
    h += &format!("Sec-WebSocket-Accept: {acc}\r\n\r\n");
    w.write_all(h.as_bytes()).await?;
    Ok(true) // từ đây trao đổi bằng khung WebSocket
}

/// Gửi một khung từ server: FIN = 1, không che (server không che dữ liệu).
async fn ws_send(w: &mut TcpStream, opcode: u8, data: &[u8]) -> io::Result<()> {
    let n = data.len();
    let mut head = vec![0x80 | opcode];
    if n < 126 {
        head.push(n as u8);
    } else if n < 65536 {
        head.push(126);
        head.extend_from_slice(&(n as u16).to_be_bytes());
    } else {
        head.push(127);
        head.extend_from_slice(&(n as u64).to_be_bytes());
    }
    head.extend_from_slice(data);
    w.write_all(&head).await
}

/// WS_CLOSE(s, code): khung opcode 0x8 chứa mã đóng 2 byte.
async fn ws_close(w: &mut TcpStream, code: u16) -> io::Result<()> {
    ws_send(w, 0x8, &code.to_be_bytes()).await
}

/// Mã giả 7.4: đọc một khung WebSocket; trả về (fin, opcode, dữ liệu đã bỏ che).
async fn ws_read_frame(r: &mut Conn, from_client: bool) -> Result<(bool, u8, Vec<u8>), WsError> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b).await?;
    let fin = (b[0] & 0x80) != 0;
    let opcode = b[0] & 0x0F;
    let masked = (b[1] & 0x80) != 0;
    let mut n = (b[1] & 0x7F) as u64;
    if n == 126 {
        n = r.read_u16().await? as u64; // độ dài 16 bit, thứ tự mạng
    } else if n == 127 {
        n = r.read_u64().await?; // độ dài 64 bit
    }
    if n > MAX_FRAME {
        // kể cả khi bit cao = 1
        ws_close(r.get_mut(), 1009).await?; // 1009: thông điệp quá lớn
        return Err(WsError::Protocol("Khung quá lớn".into()));
    }
    if from_client && !masked {
        ws_close(r.get_mut(), 1002).await?; // 1002: vi phạm giao thức
        return Err(WsError::Protocol("Khung client phải được che".into()));
    }
    let mut mask = [0u8; 4];
    if masked {
        r.read_exact(&mut mask).await?;
    }
    let mut data = vec![0u8; n as usize];
    r.read_exact(&mut data).await?;
    if masked {
        for (i, d) in data.iter_mut().enumerate() {
            *d ^= mask[i % 4]; // XOR với byte khóa ở vị trí i MOD 4
        }
    }
    Ok((fin, opcode, data))
}

/// Vòng phục vụ một kết nối: bắt tay rồi đọc khung đến khi đóng.
async fn session(stream: TcpStream) {
    let addr = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
    let mut r = BufReader::new(stream);
    match session_loop(&mut r, &addr).await {
        Ok(()) => {}
        Err(WsError::Protocol(e)) => println!("[server] Lỗi giao thức với {addr}: {e}"),
        Err(WsError::Io(e)) => println!("[server] Kết nối {addr} đóng đột ngột: {e}"),
    }
    drop(r); // CLOSE(conn)
    println!("[server] Đóng kết nối {addr}");
}

async fn session_loop(r: &mut Conn, addr: &str) -> Result<(), WsError> {
    let line = read_line_async(r).await?;
    let hdr = read_headers_async(r).await?;
    if !ws_accept(r.get_mut(), &hdr).await? {
        println!("[server] {addr} {line} -> 400 (không phải yêu cầu WebSocket)");
        return Ok(());
    }
    println!("[server] Bắt tay WebSocket thành công với {addr}");
    let (mut message, mut msg_type) = (Vec::new(), 0u8);
    loop {
        let (fin, opcode, data) = ws_read_frame(r, true).await?;
        if opcode == 0x8 {
            // đóng: gửi lại mã đóng rồi kết thúc
            let code = if data.len() >= 2 { u16::from_be_bytes([data[0], data[1]]) } else { 1005 };
            println!("[server] {addr} gửi khung đóng, mã {code}");
            ws_close(r.get_mut(), if code == 1005 { 1000 } else { code }).await?;
            return Ok(());
        }
        if opcode == 0x9 {
            // ping -> pong cùng dữ liệu
            println!("[server] Nhận ping: '{}' -> gửi pong", String::from_utf8_lossy(&data));
            ws_send(r.get_mut(), 0xA, &data).await?;
            continue;
        }
        if opcode == 0x1 || opcode == 0x2 {
            message = data;
            msg_type = opcode;
        } else if opcode == 0x0 {
            message.extend(data); // khung tiếp nối
        }
        if !fin {
            continue;
        }
        if msg_type == 0x1 {
            let text = String::from_utf8(message.clone()).map_err(|_| WsError::Protocol("Văn bản không phải UTF-8".into()))?;
            let head: String = text.chars().take(40).collect();
            println!("[server] Nhận văn bản ({} byte): '{head}'", message.len());
            ws_send(r.get_mut(), 0x1, format!("echo: {text}").as_bytes()).await?;
        } else {
            println!("[server] Nhận nhị phân ({} byte)", message.len());
            ws_send(r.get_mut(), 0x2, &message).await?;
        }
    }
}

async fn serve(args: &[String]) {
    let host = args.first().map(String::as_str).unwrap_or("0.0.0.0");
    let port = port_arg(args, 1, 8080);
    let srv = TcpListener::bind((host, port))
        .await
        .unwrap_or_else(|e| exit_with(&format!("Không mở được cổng {port}: {e}")));
    println!("[server] Đang lắng nghe tại {host}:{port}");
    loop {
        let Ok((conn, _)) = srv.accept().await else { continue };
        tokio::spawn(session(conn)); // mỗi kết nối một tác vụ
    }
}

pub async fn run(args: &[String]) {
    tokio::select! {
        _ = serve(args) => {}
        _ = tokio::signal::ctrl_c() => println!("[server] Dừng server"),
    }
}
