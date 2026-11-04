//! Bài 8.1 – Gọi API LLM có truyền luồng SSE, giới hạn tần suất, thử lại, hạn chót và tín hiệu hủy.
//!
//! Cài đặt Mã giả 8.2 – CALL_LLM(req, on_token, ct) (Mục 8.2.3, Giáo trình Lập trình mạng), kế thừa
//! Mã giả 8.1 – STREAM_COMPLETION (llm_stream.rs), Mã giả 5.8 – RETRY_ASYNC(op, max_tries, base_ms, cap_ms, ct)
//! và Mã giả 8.3 – ACQUIRE_TOKENS (thung_token.rs, bản đồng bộ chép gọn).
use crate::https_post::{https_post, CancelToken};
use crate::llm_stream::{stream_completion, CallError, StreamState};
use crate::thung_token::{acquire_tokens, Bucket};
use serde_json::Value;
use std::hash::{BuildHasher, Hasher};
use std::sync::LazyLock;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Hạn mức tự đặt: 20 000 token, nạp 2 000 token/giây.
static BUCKET: LazyLock<Bucket> = LazyLock::new(|| Bucket::new(20000.0, 2000.0));

/// SECRET(name): bí mật chỉ lấy từ biến môi trường, không ghi trong mã nguồn, không in ra.
pub fn secret(name: &str) -> Result<String, String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => Err(format!("Thiếu biến môi trường {name}")),
    }
}

/// ESTIMATE_TOKENS: ước lượng thô ~4 ký tự/token cho đầu vào, cộng max_tokens cho đầu ra.
fn estimate_tokens(req: &Value) -> f64 {
    let chars: usize = req["messages"]
        .as_array()
        .map(|ms| ms.iter().map(|m| m["content"].as_str().unwrap_or("").chars().count()).sum())
        .unwrap_or(0);
    (chars / 4) as f64 + req["max_tokens"].as_f64().unwrap_or(0.0)
}

/// Số ngày kể từ 1970-01-01 của một ngày dương lịch (thuật toán "days from civil" của H. Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// PARSE_RETRY_AFTER: Retry-After là số giây hoặc một ngày giờ HTTP (RFC 9110, ví dụ
/// "Sun, 06 Nov 1994 08:49:37 GMT"); trả về số mili giây.
fn parse_retry_after(v: Option<&String>) -> Option<u64> {
    let v = v?.trim();
    if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) {
        return v.parse::<u64>().ok().map(|s| s * 1000);
    }
    let p: Vec<&str> = v.split_whitespace().collect(); // [thứ, ngày, tháng, năm, giờ:phút:giây, GMT]
    if p.len() != 6 {
        return None;
    }
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let m = months.iter().position(|x| *x == p[2])? as i64 + 1;
    let t: Vec<i64> = p[4].split(':').filter_map(|x| x.parse().ok()).collect();
    if t.len() != 3 {
        return None;
    }
    let at = days_from_civil(p[3].parse().ok()?, m, p[1].parse().ok()?) * 86400 + t[0] * 3600 + t[1] * 60 + t[2];
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some(((at - now).max(0) * 1000) as u64)
}

/// RANDOM(0, max): số ngẫu nhiên lấy từ khóa băm của RandomState (thư viện chuẩn không có bộ sinh số ngẫu nhiên).
fn random_up_to(max: u64) -> u64 {
    std::collections::hash_map::RandomState::new().build_hasher().finish() % (max + 1)
}

/// DELAY(delay_ms, ct): chờ nhưng dừng ngay khi bị hủy.
fn delay(ms: u64, ct: &CancelToken) -> Result<(), CallError> {
    let mut left = ms;
    while left > 0 {
        ct.check()?;
        let step = left.min(50);
        thread::sleep(Duration::from_millis(step));
        left -= step;
    }
    ct.check()
}

/// Mã giả 5.8 – RETRY_ASYNC(op, max_tries, base_ms, cap_ms, ct): bắt Timeout và NetError, không bắt Cancelled.
fn retry_call<T>(
    mut op: impl FnMut(&CancelToken) -> Result<T, CallError>,
    max_tries: u32,
    base_ms: u64,
    cap_ms: u64,
    ct: &CancelToken,
) -> Result<T, CallError> {
    for k in 0..max_tries {
        let e = match op(ct) {
            Ok(v) => return Ok(v),
            Err(CallError::Cancelled) => return Err(CallError::Cancelled), // không thử lại
            Err(e) => e,
        };
        let (ok, retry_after) = match &e {
            CallError::Timeout(_) => (true, None), // Timeout: luôn đáng thử lại
            CallError::Net { retry, retry_after, .. } => (*retry, *retry_after),
            CallError::Cancelled => unreachable!(),
        };
        if !ok || k == max_tries - 1 {
            return Err(e);
        }
        let mut d = random_up_to(cap_ms.min(base_ms * 2u64.pow(k))); // jitter
        if let Some(ra) = retry_after {
            d = d.max(ra); // server đề nghị
        }
        println!("[llm] Lần thử {}/{max_tries}: {e} – chờ {d} ms rồi thử lại", k + 1);
        delay(d, ct)?; // DELAY_ASYNC(delay, ct): hủy được khi đang chờ
    }
    Err(CallError::net("max_tries phải ≥ 1"))
}

/// Mã giả 8.2 – CALL_LLM(req, on_token, ct).
pub fn call_llm(base_url: &str, req: &Value, mut on_token: impl FnMut(&str), ct: &CancelToken) -> Result<StreamState, CallError> {
    let mut started = false;
    let one_attempt = |ct: &CancelToken| -> Result<StreamState, CallError> {
        acquire_tokens(&BUCKET, estimate_tokens(req)).map_err(CallError::net)?; // 8.2.4
        let key = secret("LLM_API_KEY").map_err(CallError::net)?;
        let hdrs = [("Authorization", format!("Bearer {key}"))];
        let body = serde_json::to_vec(req).unwrap();
        let resp = https_post(&format!("{base_url}/chat/completions"), &body, &hdrs, 30000, ct)?;
        if [408, 429, 500, 502, 503, 504].contains(&resp.status) {
            return Err(CallError::Net {
                msg: format!("HTTP {}", resp.status),
                retry: !started,
                retry_after: parse_retry_after(resp.headers.get("retry-after")),
            });
        }
        if resp.status >= 400 {
            return Err(CallError::net(format!("HTTP {}", resp.status)));
        }
        let st = stream_completion(resp.body, |d| {
            started = true; // tok ← (d) → { started ← TRUE; on_token(d) }
            on_token(d);
        });
        ct.check()?; // luồng dừng vì bị hủy: báo Cancelled thay cho "bị ngắt giữa chừng"
        st
    };
    retry_call(one_attempt, 5, 500, 20000, ct)
}
