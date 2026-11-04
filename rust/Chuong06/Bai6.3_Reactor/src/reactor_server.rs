//! Bài 6.3 – Server echo một luồng dùng dồn kênh vào/ra (mô hình reactor).
//!
//! Cài đặt Mã giả 6.4 (Mục 6.2.4, Giáo trình Lập trình mạng). Kế thừa Mã giả 3.1: cùng chức năng echo,
//! vòng ACCEPT/RECV chặn được thay bằng vòng SELECT trên tập socket không chặn (crate polling).
//!
//! Chạy:  cargo run -- server [host] [port]      (mặc định 0.0.0.0 9000)
use crate::netutil::{exit_with, parse_endpoint};
use polling::{Event, Events, Poller};
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};

const MAX_BUF: usize = 65536; // MAX_BUF ← 65536: ngưỡng bộ đệm gửi của mỗi kết nối
const SRV_KEY: usize = 0; // khóa của socket lắng nghe trong poller

/// Một kết nối: socket, bộ đệm gửi (out_buf[c] của mã giả) và cờ closing (c ∈ closing:
/// client đã đóng chiều gửi); name, echoed chỉ để ghi nhật ký.
struct Conn {
    sock: TcpStream,
    out_buf: Vec<u8>,
    closing: bool,
    name: String,
    echoed: usize,
}

pub fn run(args: &[String]) {
    let (host, port) = parse_endpoint(args, "0.0.0.0", 9000);
    // SOCKET, SETOPT(REUSEADDR), BIND, LISTEN như Mã giả 3.1
    let srv = TcpListener::bind((host.as_str(), port))
        .unwrap_or_else(|e| exit_with(&format!("Không mở được cổng {port}: {e}")));
    srv.set_nonblocking(true).unwrap(); // SETOPT(srv, NONBLOCKING, TRUE)
    let mut out_buf: HashMap<usize, Conn> = HashMap::new(); // khóa → kết nối và dữ liệu chờ gửi
    let mut next_key = 1;
    let poller = Poller::new().unwrap(); // epoll trên Linux
    // Đăng ký socket với poller; mỗi socket phải được xóa khỏi poller trước khi đóng.
    unsafe { poller.add(&srv, Event::readable(SRV_KEY)).unwrap() };
    let mut events = Events::new();
    println!("[server] Reactor một luồng đang lắng nghe tại {host}:{port}");

    loop {
        // Chế độ mặc định của polling là "một lần" (oneshot): sau mỗi sự kiện phải khai báo lại
        // mối quan tâm. Vì vậy mỗi vòng lặp dựng lại hai tập của mã giả:
        //   room    = kết nối chưa đóng chiều gửi, bộ đệm gửi chưa đầy → theo dõi đọc
        //   pending = kết nối còn dữ liệu chờ gửi                      → theo dõi ghi
        poller.modify(&srv, Event::readable(SRV_KEY)).unwrap();
        for (key, c) in &out_buf {
            let room = !c.closing && c.out_buf.len() < MAX_BUF; // KEYS(out_buf) − closing
            let pending = !c.out_buf.is_empty();
            poller.modify(&c.sock, Event::new(*key, room, pending)).unwrap();
        }
        events.clear();
        poller.wait(&mut events, None).unwrap(); // SELECT({srv} ∪ room, pending)
        let readable: Vec<usize> = events.iter().filter(|e| e.readable).map(|e| e.key).collect();
        let writable: Vec<usize> = events.iter().filter(|e| e.writable).map(|e| e.key).collect();

        for key in readable {
            if key == SRV_KEY {
                // có kết nối mới
                let Ok((conn, a)) = srv.accept() else { continue }; // ACCEPT
                conn.set_nonblocking(true).unwrap(); // SETOPT(conn, NONBLOCKING, TRUE)
                unsafe { poller.add(&conn, Event::none(next_key)).unwrap() };
                let c = Conn { sock: conn, out_buf: Vec::new(), closing: false, name: a.to_string(), echoed: 0 }; // EMPTY_BYTES
                out_buf.insert(next_key, c);
                println!("[server] Client {} kết nối (đang mở: {})", a, out_buf.len());
                next_key += 1;
            } else {
                let c = out_buf.get_mut(&key).unwrap();
                let mut data = [0u8; 4096];
                let n = match c.sock.read(&mut data) {
                    Ok(n) => n,                                      // RECV(s, 4096): socket đã sẵn sàng
                    Err(e) if e.kind() == ErrorKind::WouldBlock => continue, // báo nhầm: bỏ qua
                    Err(_) => {
                        close_conn(&poller, &mut out_buf, key); // ví dụ ConnectionReset: đóng ngay
                        continue;
                    }
                };
                if n == 0 {
                    // client gửi FIN: gửi nốt dữ liệu còn trong bộ đệm rồi mới đóng
                    if !c.closing {
                        println!("[server] Client {} đóng chiều gửi, còn {} byte chờ gửi", c.name, c.out_buf.len());
                    }
                    c.closing = true; // ADD(closing, s)
                } else {
                    c.out_buf.extend_from_slice(&data[..n]); // chờ gửi
                }
            }
        }
        for key in writable {
            let Some(c) = out_buf.get_mut(&key) else { continue }; // bỏ s vừa đóng do lỗi
            match c.sock.write(&c.out_buf) {
                Ok(n) => {
                    // có thể gửi một phần: giữ lại phần chưa gửi
                    c.out_buf.drain(..n); // out_buf[s] ← out_buf[s][n .. END]
                    c.echoed += n;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(_) => close_conn(&poller, &mut out_buf, key),
            }
        }
        // FOR EACH s IN COPY(closing): đã gửi hết thì đóng
        let done: Vec<usize> = out_buf.iter().filter(|(_, c)| c.closing && c.out_buf.is_empty()).map(|(k, _)| *k).collect();
        for key in done {
            close_conn(&poller, &mut out_buf, key); // REMOVE(out_buf, s); REMOVE(closing, s); CLOSE(s)
        }
    }
}

/// REMOVE(out_buf, s); REMOVE(closing, s); CLOSE(s) – xóa khỏi poller trước khi đóng socket.
fn close_conn(poller: &Poller, out_buf: &mut HashMap<usize, Conn>, key: usize) {
    let c = out_buf.remove(&key).unwrap();
    poller.delete(&c.sock).unwrap();
    drop(c.sock); // CLOSE(s)
    println!("[server] Đóng {}: đã echo {} byte, còn mở {}", c.name, c.echoed, out_buf.len());
}
