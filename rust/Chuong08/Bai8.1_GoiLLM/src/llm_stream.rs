//! Bài 8.1 – Nhận câu trả lời của LLM theo luồng SSE.
//!
//! Cài đặt Mã giả 8.1 – STREAM_COMPLETION (Mục 8.2.2, Giáo trình Lập trình mạng),
//! kế thừa Mã giả 7.5 – SSE_READ (Mục 7.3.3) và lớp lỗi NetError của Mã giả 2.2.
use serde_json::Value;
use std::fmt;
use std::io::{BufRead, BufReader, Read};

/// Các lỗi của một lời gọi: NetError (Mã giả 2.2: có cờ thử lại và Retry-After tính bằng ms),
/// Timeout (luôn đáng thử lại, Mã giả 5.8) và Cancelled (người dùng hủy: không bắt, không thử lại).
#[derive(Debug)]
pub enum CallError {
    Net { msg: String, retry: bool, retry_after: Option<u64> },
    Timeout(String),
    Cancelled,
}

impl CallError {
    /// NetError(message) với retry = FALSE.
    pub fn net(msg: impl Into<String>) -> Self {
        CallError::Net { msg: msg.into(), retry: false, retry_after: None }
    }
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CallError::Net { msg, .. } | CallError::Timeout(msg) => write!(f, "{msg}"),
            CallError::Cancelled => write!(f, "Đã hủy"),
        }
    }
}

/// READ_LINE (Mã giả 3.5) trên thân phản hồi đã giải mã khối (chunked).
/// Trả về None khi thân kết thúc hoặc kết nối bị ngắt (SSE_READ chỉ dừng ở hai trường hợp này).
fn read_line<R: Read>(r: &mut BufReader<R>, max_len: usize) -> Result<Option<String>, CallError> {
    let mut raw = Vec::new();
    match (&mut *r).take(max_len as u64 + 1).read_until(b'\n', &mut raw) {
        Ok(0) | Err(_) => return Ok(None), // hết thân, ngắt kết nối, khối bị cắt dở, hết thời gian chờ
        Ok(_) => {}
    }
    if raw.len() > max_len && !raw.ends_with(b"\n") {
        return Err(CallError::net("Dòng SSE quá dài")); // ProtocolError("Dòng quá dài")
    }
    while raw.last() == Some(&b'\n') || raw.last() == Some(&b'\r') {
        raw.pop();
    }
    Ok(Some(String::from_utf8_lossy(&raw).into_owned()))
}

/// Mã giả 7.5 – SSE_READ(r, on_event).
fn sse_read<R: Read>(
    r: &mut BufReader<R>,
    mut on_event: impl FnMut(&str, &str) -> Result<(), CallError>,
) -> Result<(), CallError> {
    let (mut ev_type, mut ev_data) = ("message".to_string(), Vec::<String>::new());
    while let Some(line) = read_line(r, 65536)? {
        if line.is_empty() {
            // dòng trống: hết một sự kiện
            if !ev_data.is_empty() {
                on_event(&ev_type, &ev_data.join("\n"))?;
            }
            (ev_type, ev_data) = ("message".to_string(), Vec::new());
        } else if !line.starts_with(':') {
            // ":": dòng chú thích
            let (field, value) = line.split_once(':').unwrap_or((&line, ""));
            let value = value.strip_prefix(' ').unwrap_or(value); // STRIP_ONE_SPACE
            match field {
                "event" => ev_type = value.to_string(),
                "data" => ev_data.push(value.to_string()),
                _ => {} // "id", "retry": không dùng trong bài này
            }
        }
    }
    Ok(()) // NULL: server đóng luồng
}

/// Trạng thái của một luồng trả lời (dòng 1 của Mã giả 8.1).
pub struct StreamState {
    pub text: String,
    pub usage: Option<Value>,
    pub done: bool,
}

/// Mã giả 8.1 – STREAM_COMPLETION(r, on_token); r: thân phản hồi sau tiêu đề.
pub fn stream_completion<R: Read>(body: R, mut on_token: impl FnMut(&str)) -> Result<StreamState, CallError> {
    let mut st = StreamState { text: String::new(), usage: None, done: false }; // dòng 1
    let mut r = BufReader::new(body);
    sse_read(&mut r, |_type, data| {
        // dòng 2: ON_EVENT
        if data == "[DONE]" {
            st.done = true; // dòng 3: không phải JSON
            return Ok(());
        }
        let chunk: Value = serde_json::from_str(data).map_err(|e| CallError::net(format!("JSON sai: {e}")))?; // dòng 4
        if !chunk["usage"].is_null() {
            st.usage = Some(chunk["usage"].clone()); // dòng 5–6
        }
        let Some(choice) = chunk["choices"].as_array().and_then(|c| c.first()) else {
            return Ok(()); // dòng 7: sự kiện chỉ mang usage
        };
        if let Some(d) = choice["delta"]["content"].as_str() {
            // dòng 8–12
            st.text.push_str(d);
            on_token(d);
        }
        Ok(())
    })?; // dòng 14
    if !st.done {
        return Err(CallError::net("Luồng bị ngắt giữa chừng")); // dòng 15
    }
    Ok(st) // dòng 16
}
