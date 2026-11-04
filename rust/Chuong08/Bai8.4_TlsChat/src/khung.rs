//! Bài 8.4 – Gửi, nhận khung JSON bất đồng bộ (chép gọn Mã giả 5.1, kế thừa Mã giả 3.6).
//!
//! Khung = 4 byte độ dài (big-endian) + JSON UTF-8. Logic không đổi khi chạy trên TLS:
//! luồng TLS của tokio-rustls đọc ghi giống socket gốc (AsyncRead/AsyncWrite, Mục 8.4.5).
use serde_json::Value;
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_FRAME: u32 = 1048576; // 1 MiB

/// RECV_JSON_ASYNC: read_exact = RECV_EXACT_ASYNC; kết nối đóng -> lỗi UnexpectedEof.
pub async fn recv_json_async<R: AsyncRead + Unpin>(r: &mut R) -> io::Result<Value> {
    let n = r.read_u32().await?; // FROM_UINT32_BE
    if n > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Khung quá lớn")); // ProtocolError
    }
    let mut data = vec![0u8; n as usize];
    r.read_exact(&mut data).await?;
    serde_json::from_slice(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// SEND_JSON_ASYNC: ghép tiêu đề độ dài với nội dung rồi gửi.
pub async fn send_json_async<W: AsyncWrite + Unpin>(w: &mut W, obj: &Value) -> io::Result<()> {
    let p = serde_json::to_vec(obj).unwrap();
    let mut frame = (p.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&p);
    w.write_all(&frame).await?;
    w.flush().await // TLS: đẩy bản ghi đã mã hóa ra socket
}
