//! Mã giả 2.2 – SAFE_CONNECT (Mục 2.2.3, Giáo trình Lập trình mạng).
//! Lỗi được gom về một loại NetError với ba nhóm: NOT_FOUND, REFUSED (không nên thử lại)
//! và UNREACHABLE (hết thời gian chờ / không có đường đi, có thể thử lại).

use std::io::{self, ErrorKind};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Lỗi mạng đã phân loại: kind ∈ {NOT_FOUND, REFUSED, UNREACHABLE}.
#[derive(Debug)]
pub struct NetError {
    pub kind: &'static str,
    pub retry: bool,
    pub cause: io::Error,
}

/// Kết quả lỗi của SAFE_CONNECT: hoặc NetError đã phân loại,
/// hoặc lỗi khác mà mã giả không bắt (chuyển nguyên cho nơi gọi).
#[derive(Debug)]
pub enum ConnectError {
    Net(NetError),
    Other(io::Error),
}

/// Mã giả 2.2 – SAFE_CONNECT(host, port, timeout_ms).
pub fn safe_connect(host: &str, port: u16, timeout_ms: u64) -> Result<TcpStream, ConnectError> {
    // dòng 2–6: RESOLVE – tên miền không tồn tại → NOT_FOUND
    let addrs: Vec<SocketAddr> = match (host, port).to_socket_addrs() {
        Ok(iter) => iter.collect(),
        Err(e) => return Err(ConnectError::Net(NetError { kind: "NOT_FOUND", retry: false, cause: e })),
    };
    let mut refused = 0; // dòng 7
    let mut last_error = io::Error::new(ErrorKind::NotFound, "không có địa chỉ nào");
    for a in &addrs {
        // dòng 8–13: SOCKET(FAMILY(a), STREAM), SETOPT(CONNECT_TIMEOUT), CONNECT
        match TcpStream::connect_timeout(a, Duration::from_millis(timeout_ms)) {
            Ok(s) => return Ok(s), // thành công: trả socket, nơi gọi chịu trách nhiệm đóng
            Err(e) => match e.kind() {
                ErrorKind::ConnectionRefused => {
                    // dòng 14–15: nhận gói RST
                    refused += 1;
                    last_error = e;
                }
                ErrorKind::TimedOut | ErrorKind::NetworkUnreachable | ErrorKind::HostUnreachable => {
                    last_error = e; // dòng 16–17: Timeout, NetworkUnreachable, HostUnreachable
                }
                _ => return Err(ConnectError::Other(e)), // lỗi khác: chuyển lên nơi gọi
            },
        }
        // dòng 18–20: socket thất bại được đóng tự động (Drop) trước khi thử địa chỉ tiếp theo
    }
    if refused > 0 && refused == addrs.len() {
        // dòng 22–24
        return Err(ConnectError::Net(NetError { kind: "REFUSED", retry: false, cause: last_error }));
    }
    Err(ConnectError::Net(NetError { kind: "UNREACHABLE", retry: true, cause: last_error })) // dòng 25
}
