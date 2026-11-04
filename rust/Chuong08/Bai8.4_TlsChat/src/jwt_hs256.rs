//! Bài 8.4 – Cấp và kiểm tra JWT ký HMAC-SHA-256 (HS256) cho xác thực người dùng chat.
//!
//! VERIFY_JWT của Mã giả 8.6 theo Mục 7.2.4: kiểm tra đúng thuật toán (từ chối alg "none"), chữ ký,
//! exp chưa quá hạn, iss là ISSUER, aud là đối tượng mong đợi. Khóa ký lấy qua SECRET("CHAT_JWT_KEY").
//! Chạy (cấp token thử nghiệm, vai trò server ủy quyền giản lược):
//!     CHAT_JWT_KEY=<khóa> cargo run -- token <sub> <tệp_token> [aud=chat] [thời_hạn_giây=3600]
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

pub const ISSUER: &str = "auth.lab.local";

/// SECRET(name): bí mật chỉ lấy từ biến môi trường.
pub fn secret(name: &str) -> Result<String, String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => Err(format!("Thiếu biến môi trường {name}")),
    }
}

fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
}

/// HMAC-SHA-256 của dữ liệu với khóa.
fn mac(key: &str, data: &[u8]) -> Hmac<Sha256> {
    let mut m = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("HMAC nhận khóa mọi độ dài");
    m.update(data);
    m
}

/// Ký JWT: header.claims.chữ_ký, mỗi phần mã hóa Base64url không có dấu '='.
pub fn sign_jwt(claims: &Value, key: &str) -> String {
    let head = B64URL.encode(json!({"alg": "HS256", "typ": "JWT"}).to_string());
    let body = B64URL.encode(claims.to_string());
    let sig = mac(key, format!("{head}.{body}").as_bytes()).finalize().into_bytes();
    format!("{head}.{body}.{}", B64URL.encode(sig))
}

/// VERIFY_JWT(token, issuer, audience) -> claims; lỗi -> lý do.
pub fn verify_jwt(token: &Value, key: &str, issuer: &str, audience: &str) -> Result<Value, String> {
    let parts: Vec<&str> = token.as_str().map(|t| t.split('.').collect()).unwrap_or_default();
    if parts.len() != 3 {
        return Err("định dạng sai".into());
    }
    let decode = |s: &str| -> Result<Vec<u8>, String> { B64URL.decode(s.trim_end_matches('=')).map_err(|_| "định dạng sai".to_string()) };
    let header: Value = serde_json::from_slice(&decode(parts[0])?).map_err(|_| "định dạng sai")?;
    let claims: Value = serde_json::from_slice(&decode(parts[1])?).map_err(|_| "định dạng sai")?;
    let sig = decode(parts[2])?;
    if header["alg"] != "HS256" {
        return Err("thuật toán không được chấp nhận".into()); // đúng thuật toán mong đợi
    }
    // verify_slice so sánh trong thời gian hằng
    if mac(key, format!("{}.{}", parts[0], parts[1]).as_bytes()).verify_slice(&sig).is_err() {
        return Err("chữ ký sai".into());
    }
    match claims["exp"].as_f64() {
        Some(exp) if exp > now_secs() => {}
        _ => return Err("đã hết hạn".into()),
    }
    if claims["iss"] != issuer {
        return Err("iss không đúng".into());
    }
    let aud_ok = match &claims["aud"] {
        Value::Array(list) => list.iter().any(|a| a == audience),
        a => a == audience,
    };
    if !aud_ok {
        return Err("aud không đúng".into());
    }
    if claims["sub"].as_str().map_or(true, str::is_empty) {
        return Err("thiếu sub".into());
    }
    Ok(claims)
}

/// Vai trò "token": cấp token thử nghiệm và ghi ra tệp.
pub fn run(args: &[String]) -> Result<(), String> {
    if args.len() < 2 {
        return Err("Cách dùng: cargo run -- token <sub> <tệp_token> [aud] [thời_hạn_giây]".into());
    }
    let (sub, path) = (&args[0], &args[1]);
    let aud = args.get(2).map(String::as_str).unwrap_or("chat");
    let ttl: u64 = match args.get(3) {
        Some(s) => s.parse().map_err(|_| format!("Tham số phải là số nguyên: {s}"))?,
        None => 3600,
    };
    let now = now_secs() as u64;
    let token = sign_jwt(&json!({"iss": ISSUER, "sub": sub, "aud": aud, "iat": now, "exp": now + ttl}), &secret("CHAT_JWT_KEY")?);
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, token).map_err(|e| format!("Không ghi được {path}: {e}"))?;
    println!("[token] Đã cấp token sub={sub}, aud={aud}, iss={ISSUER}, hết hạn sau {ttl} s -> {path}");
    Ok(())
}
