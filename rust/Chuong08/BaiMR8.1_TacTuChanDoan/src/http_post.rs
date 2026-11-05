//! POST JSON tối giản trên HTTP/1.1 (http:// hoặc https:// bọc TLS bằng rustls) – đủ để gọi API LLM
//! dạng Chat Completions; đọc thân phản hồi theo Content-Length, mã hóa chunked hoặc tới khi đóng.

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

trait Stream: Read + Write {}
impl<T: Read + Write> Stream for T {}

fn loi(msg: String) -> io::Error {
    io::Error::other(msg)
}

/// Bọc TLS cho socket: kiểm tra chứng chỉ theo kho của hệ điều hành và tên máy.
fn tls_wrap(sock: TcpStream, host: &str) -> io::Result<Box<dyn Stream>> {
    let mut roots = RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    let config = ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    let name = ServerName::try_from(host.to_string()).map_err(|e| loi(format!("tên máy sai: {e}")))?;
    let conn = ClientConnection::new(Arc::new(config), name).map_err(|e| loi(format!("TLS: {e}")))?;
    Ok(Box::new(StreamOwned::new(conn, sock)))
}

/// Gửi POST url với thân JSON; trả về (mã trạng thái, thân phản hồi).
pub fn post_json(url: &str, body: &[u8], headers: &[(&str, String)], timeout: Duration) -> io::Result<(u16, Vec<u8>)> {
    let (https, rest) = match (url.strip_prefix("https://"), url.strip_prefix("http://")) {
        (Some(r), _) => (true, r),
        (_, Some(r)) => (false, r),
        _ => return Err(loi(format!("URL không hỗ trợ: {url}"))),
    };
    let (hostport, path) = rest.split_once('/').map_or((rest, "/".to_string()), |(h, p)| (h, format!("/{p}")));
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().map_err(|_| loi(format!("cổng sai trong {url}")))?),
        None => (hostport, if https { 443 } else { 80 }),
    };
    let addr = (host, port).to_socket_addrs()?.next().ok_or_else(|| loi(format!("không phân giải được {host}")))?;
    let sock = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    sock.set_read_timeout(Some(timeout))?;
    let mut s: Box<dyn Stream> = if https { tls_wrap(sock, host)? } else { Box::new(sock) };
    let mut req = format!("POST {path} HTTP/1.1\r\nHost: {hostport}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes())?;
    s.write_all(body)?;
    s.flush()?;
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line)?;
    let status: u16 = line.split_whitespace().nth(1).and_then(|c| c.parse().ok()).ok_or_else(|| loi(format!("phản hồi lạ: {}", line.trim())))?;
    let (mut len, mut chunked) = (None, false);
    loop {
        let mut h = String::new();
        if r.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => len = v.trim().parse::<usize>().ok(),
                "transfer-encoding" => chunked = v.to_ascii_lowercase().contains("chunked"),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    if chunked {
        loop {
            let mut sz = String::new();
            r.read_line(&mut sz)?;
            let n = usize::from_str_radix(sz.trim().split(';').next().unwrap_or("0"), 16).map_err(|_| loi("chunk sai".into()))?;
            if n == 0 {
                break;
            }
            let mut c = vec![0u8; n + 2]; // dữ liệu + "\r\n"
            r.read_exact(&mut c)?;
            out.extend_from_slice(&c[..n]);
        }
    } else if let Some(n) = len {
        out.resize(n, 0);
        r.read_exact(&mut out)?;
    } else {
        r.read_to_end(&mut out)?;
    }
    Ok((status, out))
}
