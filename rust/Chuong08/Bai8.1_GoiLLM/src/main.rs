//! Bài 8.1 – Gọi API LLM: truyền luồng SSE, giới hạn tần suất, thử lại (Mã giả 8.1, 8.2; Mục 8.2)
//!
//! Cấu hình qua biến môi trường: LLM_BASE_URL (mặc định http://127.0.0.1:9000/v1 – server giả lập),
//! LLM_API_KEY (bắt buộc), LLM_MODEL (mặc định mock-model), LLM_PRICE_IN, LLM_PRICE_OUT
//! (đơn giá USD cho 1 triệu token, tùy chọn). Ctrl+C hủy lời gọi (tín hiệu ct).
//!
//! Chạy:  python3 mock_llm_server.py 127.0.0.1 9000 --fail-first 1                 (cửa sổ 1)
//!        LLM_API_KEY=khoa-thu cargo run --release -- "So sánh TCP và UDP."       (cửa sổ 2)
mod https_post;
mod llm_client;
mod llm_stream;
mod thung_token;

use https_post::CancelToken;
use llm_stream::CallError;
use serde_json::json;
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let base_url = std::env::var("LLM_BASE_URL").unwrap_or("http://127.0.0.1:9000/v1".into());
    let base_url = base_url.trim_end_matches('/').to_string();
    let model = std::env::var("LLM_MODEL").unwrap_or("mock-model".into());
    let args: Vec<String> = std::env::args().skip(1).collect();
    let question = if args.is_empty() { "So sánh TCP và UDP.".to_string() } else { args.join(" ") };
    let req = json!({"model": model, "stream": true, "max_tokens": 512,
                     "messages": [{"role": "system", "content": "Trợ lý mạng, trả lời ngắn."},
                                  {"role": "user", "content": question}]});
    println!("[llm] Mô hình {model} tại {base_url}");
    println!("[llm] Câu hỏi: {question}");
    if let Err(e) = llm_client::secret("LLM_API_KEY") {
        println!("[llm] {e}");
        std::process::exit(2);
    }
    // Ctrl+C: phát tín hiệu hủy ct (dừng cả lần gọi đang chạy lẫn khoảng chờ giữa hai lần thử)
    let ct = Arc::new(CancelToken::default());
    let ct2 = Arc::clone(&ct);
    ctrlc::set_handler(move || ct2.cancel()).expect("Không cài được hàm bắt Ctrl+C");

    let t0 = Instant::now();
    let mut first: Option<Instant> = None;
    let on_token = |d: &str| {
        // UI: UI_INVOKE(APPEND, d)
        if first.is_none() {
            first = Some(Instant::now());
            print!("[llm] Trả lời: ");
        }
        print!("{d}");
        let _ = std::io::stdout().flush();
    };
    let result = llm_client::call_llm(&base_url, &req, on_token, &ct);
    let st = match result {
        Ok(st) => st,
        Err(e) => {
            if first.is_some() {
                println!();
            }
            match e {
                CallError::Cancelled => {
                    println!("[llm] Người dùng đã hủy lời gọi (Ctrl+C)");
                    std::process::exit(130);
                }
                e => {
                    let note = if first.is_some() { " – không thử lại vì đã hiển thị một phần câu trả lời" } else { "" };
                    println!("[llm] Lỗi: {e}{note}");
                    std::process::exit(1);
                }
            }
        }
    };
    let total = t0.elapsed();
    println!();
    let ttft = first.map(|f| f.duration_since(t0).as_millis()).unwrap_or(0);
    println!("[llm] TTFT: {ttft} ms, tổng thời gian: {} ms", total.as_millis());
    let usage = st.usage.unwrap_or(json!({}));
    let (n_in, n_out) = (usage["prompt_tokens"].as_u64().unwrap_or(0), usage["completion_tokens"].as_u64().unwrap_or(0));
    println!("[llm] usage: prompt_tokens = {n_in}, completion_tokens = {n_out}");
    if let (Ok(p_in), Ok(p_out)) = (std::env::var("LLM_PRICE_IN"), std::env::var("LLM_PRICE_OUT")) {
        // Công thức (8.1): N_in·p_in + N_out·p_out
        if let (Ok(a), Ok(b)) = (p_in.parse::<f64>(), p_out.parse::<f64>()) {
            let cost = (n_in as f64 * a + n_out as f64 * b) / 1_000_000.0;
            println!("[llm] Chi phí ước tính: {n_in} × {p_in} + {n_out} × {p_out} (USD/1 triệu token) = {cost:.8} USD");
        }
    }
}
