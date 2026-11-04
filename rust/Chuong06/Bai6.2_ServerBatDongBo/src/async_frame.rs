//! Bài 6.2 – Các hàm gửi, nhận khung JSON bất đồng bộ, chép gọn từ Mã giả 5.1 (Mục 5.2.4).
//!
//! Trong tokio, `read(...).await` là RECV_ASYNC, `write_all(...).await` là SEND_ALL_ASYNC
//! (chờ đến khi hệ điều hành nhận hết dữ liệu vào bộ đệm gửi).
use crate::netutil::{NetError, MAX_FRAME};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// RECV_EXACT_ASYNC: nhận đủ n byte; đóng giữa chừng → ConnectionClosed(số byte đã nhận).
pub async fn recv_exact_async(conn: &mut TcpStream, n: usize) -> Result<Vec<u8>, NetError> {
    let mut buf = vec![0u8; n];
    let mut got = 0;
    while got < n {
        let k = conn.read(&mut buf[got..]).await?; // AWAIT RECV_ASYNC
        if k == 0 {
            return Err(NetError::ConnectionClosed(got));
        }
        got += k;
    }
    Ok(buf)
}

/// RECV_JSON_ASYNC (Mã giả 5.1, dòng 12–20). JSON sai trả về NetError::Json.
pub async fn recv_json_async(conn: &mut TcpStream) -> Result<Value, NetError> {
    let h = recv_exact_async(conn, 4).await?;
    let n = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    if n > MAX_FRAME {
        return Err(NetError::Protocol("Khung quá lớn".into()));
    }
    let data = match recv_exact_async(conn, n as usize).await {
        Ok(d) => d,
        Err(NetError::ConnectionClosed(k)) => return Err(NetError::ConnectionClosed(4 + k)), // đã nhận xong tiêu đề
        Err(e) => return Err(e),
    };
    let obj: Value = serde_json::from_slice(&data).map_err(|e| NetError::Json(e.to_string()))?;
    if !obj.is_object() {
        return Err(NetError::Json("Thông điệp không phải đối tượng JSON".into()));
    }
    Ok(obj)
}

/// SEND_JSON_ASYNC (Mã giả 5.1, dòng 22–24): ghép 4 byte độ dài rồi gửi cả khung.
pub async fn send_json_async(conn: &mut TcpStream, obj: &Value) -> Result<(), NetError> {
    let p = serde_json::to_vec(obj).expect("JSON hợp lệ");
    let mut frame = (p.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&p);
    conn.write_all(&frame).await?; // SEND_ALL_ASYNC
    Ok(())
}
