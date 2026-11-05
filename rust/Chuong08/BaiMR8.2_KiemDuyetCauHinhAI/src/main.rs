//! Bài mở rộng MR8.2 – Kiểm duyệt cấu hình do LLM sinh ra trước khi dùng (guardrail + vòng phản hồi).
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Bài 8.1 (gọi API LLM), Bài 3.3 (dữ liệu JSON)
//! và Bài MR1.1 (kiểm tra địa chỉ). Nguyên tắc: đầu ra của LLM là DỮ LIỆU CHƯA TIN CẬY, giống dữ liệu
//! nhận từ mạng; phải kiểm tra như kiểm tra gói tin từ client lạ:
//!   1. Tách và phân tích JSON (LLM hay bọc JSON trong khối ```json kèm lời dẫn).
//!   2. Lược đồ: đủ trường, đúng kiểu.
//!   3. Ngữ nghĩa: VLAN 1–4094, không trùng; mạng con hợp lệ, nằm trong khối địa chỉ, không chồng
//!      lấn nhau và không đụng hạ tầng đang dùng; cổng mặc định là địa chỉ máy hợp lệ trong mạng.
//!   4. Chính sách an toàn: cấm "permit ip any any", cấm bật telnet.
//! Nếu có lỗi: gửi danh sách lỗi lại cho LLM để sửa (tối đa --so-lan lần); hết lượt vẫn sai thì TỪ CHỐI.
//! Cấu hình đạt mới được chuyển thành lệnh CLI – và vẫn cần người duyệt trước khi áp dụng.
//! Cấu hình qua biến môi trường: LLM_BASE_URL, LLM_API_KEY, LLM_MODEL (như Bài 8.1).
//!
//! Chạy:  cargo run -q --release -- <ha_tang.json> "<yêu cầu>" [--so-lan N]

mod http_post;
mod mang;

use mang::Net;
use regex::Regex;
use serde_json::{json, Value};
use std::net::Ipv4Addr;
use std::process;
use std::time::Duration;

const SYSTEM: &str = "Bạn sinh cấu hình mạng. Chỉ trả về một đối tượng JSON có các khóa: \
vlan (danh sách {id, ten, mang, cong_mac_dinh}), acl (danh sách {ten, quy_tac}), dich_vu ({telnet, ssh}).";

fn chat(base_url: &str, model: &str, messages: &[Value]) -> Result<String, String> {
    let Ok(key) = std::env::var("LLM_API_KEY") else {
        eprintln!("Thiếu biến môi trường LLM_API_KEY");
        process::exit(1)
    };
    let req = json!({"model": model, "messages": messages, "temperature": 0, "response_format": {"type": "json_object"}});
    let hdrs = [("Content-Type", "application/json".to_string()), ("Authorization", format!("Bearer {key}"))];
    let (status, body) = http_post::post_json(&format!("{base_url}/chat/completions"), req.to_string().as_bytes(), &hdrs, Duration::from_secs(10)).map_err(|e| e.to_string())?;
    if status != 200 {
        return Err(format!("HTTP Error {status}"));
    }
    let v: Value = serde_json::from_slice(&body).map_err(|e| format!("phản hồi không phải JSON: {e}"))?;
    Ok(v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
}

/// Lấy JSON từ khối ```json ... ``` nếu có, nếu không thì dùng toàn bộ văn bản.
fn extract_json(text: &str) -> Result<Value, String> {
    let re = Regex::new(r"(?s)```(?:json)?\s*\n(.*?)```").unwrap();
    let src = re.captures(text).map_or(text, |c| c.get(1).unwrap().as_str());
    serde_json::from_str(src).map_err(|e| e.to_string())
}

fn check_schema(cfg: &Value) -> Vec<String> {
    let mut errs = Vec::new();
    if !cfg.is_object() {
        return vec!["gốc phải là đối tượng JSON".into()];
    }
    for (key, ok) in [("vlan", cfg["vlan"].is_array()), ("acl", cfg["acl"].is_array()), ("dich_vu", cfg["dich_vu"].is_object())] {
        if !ok {
            errs.push(format!("thiếu khóa {key} hoặc sai kiểu"));
        }
    }
    for (i, v) in cfg["vlan"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        for (key, ok) in [("id", v["id"].as_i64().is_some()), ("ten", v["ten"].is_string()), ("mang", v["mang"].is_string()), ("cong_mac_dinh", v["cong_mac_dinh"].is_string())] {
            if !ok {
                errs.push(format!("vlan[{i}].{key} thiếu hoặc sai kiểu"));
            }
        }
    }
    // "quy_tac" phải là danh sách chuỗi: nếu là một chuỗi, vòng lặp kiểm chính sách duyệt từng
    // ký tự và quy tắc cấm "permit ip any any" lọt qua.
    for (i, a) in cfg["acl"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        if !a["ten"].is_string() {
            errs.push(format!("acl[{i}].ten thiếu hoặc sai kiểu"));
        }
        if !a["quy_tac"].as_array().is_some_and(|r| r.iter().all(Value::is_string)) {
            errs.push(format!("acl[{i}].quy_tac thiếu hoặc sai kiểu"));
        }
    }
    errs
}

fn check_semantics(cfg: &Value, infra: &Value) -> Vec<String> {
    let mut errs = Vec::new();
    let block = Net::parse(infra["khoi_dia_chi"].as_str().unwrap_or("")).expect("khoi_dia_chi trong tệp hạ tầng sai");
    let da_dung = infra["da_dung"].as_array().cloned().unwrap_or_default();
    let used_name = |id: i64| da_dung.iter().find(|d| d["vlan"].as_i64() == Some(id)).map(|d| d["ten"].as_str().unwrap_or("").to_string());
    let mut seen_ids: Vec<i64> = Vec::new();
    let mut nets: Vec<(String, Net)> = Vec::new();
    for v in cfg["vlan"].as_array().unwrap() {
        let name = v["ten"].as_str().unwrap();
        let id = v["id"].as_i64().unwrap();
        if !(1..=4094).contains(&id) {
            errs.push(format!("{name}: VLAN {id} ngoài miền 1-4094"));
        } else if let Some(u) = used_name(id) {
            errs.push(format!("{name}: VLAN {id} đã dùng cho {u}"));
        } else if seen_ids.contains(&id) {
            errs.push(format!("{name}: VLAN {id} bị trùng"));
        }
        seen_ids.push(id);
        let mang = v["mang"].as_str().unwrap();
        let Some(net) = Net::parse(mang) else {
            // strict: không chấp nhận bit máy
            errs.push(format!("{name}: mạng {mang} không hợp lệ"));
            continue;
        };
        if !net.subnet_of(&block) {
            errs.push(format!("{name}: mạng {net} ngoài khối {block}"));
        }
        let gw_txt = v["cong_mac_dinh"].as_str().unwrap();
        match gw_txt.parse::<Ipv4Addr>() {
            Ok(gw) => {
                let g = u32::from(gw);
                if !net.contains(g) || g == net.addr || g == net.broadcast() {
                    errs.push(format!("{name}: cổng mặc định {gw} không phải địa chỉ máy của {net}"));
                }
            }
            Err(_) => errs.push(format!("{name}: cổng mặc định {gw_txt} không phải IPv4 hợp lệ")),
        }
        for (other, onet) in &nets {
            if net.overlaps(onet) {
                errs.push(format!("{name}: mạng {net} chồng lấn {other} ({onet})"));
            }
        }
        for d in &da_dung {
            let dm = d["mang"].as_str().unwrap_or("");
            if Net::parse(dm).is_some_and(|dn| net.overlaps(&dn)) {
                errs.push(format!("{name}: mạng {net} đụng hạ tầng {} ({dm})", d["ten"].as_str().unwrap_or("")));
            }
        }
        nets.push((name.to_string(), net));
    }
    errs
}

fn check_policy(cfg: &Value) -> Vec<String> {
    let forbidden = Regex::new(r"permit\s+(ip\s+)?any\s+any").unwrap();
    let mut errs = Vec::new();
    for acl in cfg["acl"].as_array().unwrap() {
        let ten = acl["ten"].as_str().unwrap_or("None");
        for rule in acl["quy_tac"].as_array().cloned().unwrap_or_default() {
            if let Some(r) = rule.as_str().filter(|r| forbidden.is_match(r)) {
                errs.push(format!("ACL {ten}: cấm quy tắc \"{r}\""));
            }
        }
    }
    if cfg["dich_vu"]["telnet"] != false {
        errs.push("dich_vu.telnet phải tắt (mật khẩu đi dạng rõ), dùng SSH".into());
    }
    errs
}

fn validate(text: &str, infra: &Value) -> (Option<Value>, Vec<String>) {
    let cfg = match extract_json(text) {
        Ok(c) => c,
        Err(e) => return (None, vec![format!("JSON hỏng: {e}")]),
    };
    let errs = check_schema(&cfg);
    if !errs.is_empty() {
        return (None, errs);
    }
    let mut errs = check_semantics(&cfg, infra);
    errs.extend(check_policy(&cfg));
    (Some(cfg), errs)
}

/// Chuyển cấu hình đã duyệt thành lệnh CLI (kiểu IOS) – mẫu đơn giản.
fn render(cfg: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    for v in cfg["vlan"].as_array().unwrap() {
        let net = Net::parse(v["mang"].as_str().unwrap()).unwrap();
        let id = &v["id"];
        lines.push(format!("vlan {id}"));
        lines.push(format!(" name {}", v["ten"].as_str().unwrap()));
        lines.push(format!("interface Vlan{id}"));
        lines.push(format!(" ip address {} {}", v["cong_mac_dinh"].as_str().unwrap(), net.netmask()));
    }
    for acl in cfg["acl"].as_array().unwrap() {
        lines.push(format!("ip access-list extended {}", acl["ten"].as_str().unwrap_or("")));
        for r in acl["quy_tac"].as_array().cloned().unwrap_or_default() {
            lines.push(format!(" {}", r.as_str().unwrap_or("")));
        }
    }
    lines.push(if cfg["dich_vu"]["telnet"] == true { "service telnet" } else { "no service telnet" }.into());
    lines
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pos = Vec::new();
    let mut so_lan = 3;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--so-lan" {
            so_lan = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(3);
            i += 2;
        } else {
            pos.push(args[i].clone());
            i += 1;
        }
    }
    if pos.len() < 2 {
        eprintln!("Cách dùng: mr8_2_kiem_duyet_cau_hinh_ai <ha_tang.json> \"<yêu cầu>\" [--so-lan N]");
        process::exit(2);
    }
    let base_url = std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:9000/v1".into()).trim_end_matches('/').to_string();
    let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "mock-model".into());
    let infra: Value = std::fs::read_to_string(&pos[0]).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| {
        eprintln!("Không đọc được tệp hạ tầng {}", pos[0]);
        process::exit(2)
    });
    let yeu_cau = &pos[1];
    let mut messages = vec![json!({"role": "system", "content": SYSTEM}), json!({"role": "user", "content": yeu_cau})];
    println!("[kiem-duyet] Yêu cầu: {yeu_cau}");
    for attempt in 1..=so_lan {
        let text = chat(&base_url, &model, &messages).unwrap_or_else(|e| {
            println!("[kiem-duyet] Lỗi gọi LLM: {e}");
            process::exit(1)
        });
        messages.push(json!({"role": "assistant", "content": text}));
        let (cfg, errs) = validate(&text, &infra);
        if errs.is_empty() {
            println!("[kiem-duyet] Lần {attempt}: ĐẠT");
            println!("[kiem-duyet] Lệnh CLI sinh từ cấu hình đã duyệt (chờ người duyệt):");
            for line in render(&cfg.unwrap()) {
                println!("  {line}");
            }
            return;
        }
        println!("[kiem-duyet] Lần {attempt}: {} lỗi", errs.len());
        for e in &errs {
            println!("  - {e}");
        }
        let ds: Vec<String> = errs.iter().map(|e| format!("- {e}")).collect();
        messages.push(json!({"role": "user", "content": format!("Cấu hình chưa đạt, hãy sửa và chỉ trả về JSON:\n{}", ds.join("\n"))}));
    }
    println!("[kiem-duyet] TỪ CHỐI: sau {so_lan} lần vẫn chưa đạt, không sinh lệnh");
    process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tach_json_va_chinh_sach() {
        let t = "Dưới đây:\n```json\n{\"a\": 1}\n```\nhết";
        assert_eq!(extract_json(t).unwrap()["a"], 1);
        let cfg = json!({"vlan": [], "acl": [{"ten": "X", "quy_tac": ["permit   any any"]}], "dich_vu": {"telnet": false}});
        assert_eq!(check_policy(&cfg).len(), 1);
        // quy_tac là chuỗi thay vì danh sách: phải bị lược đồ chặn
        let bad = json!({"vlan": [], "acl": [{"ten": "X", "quy_tac": "permit ip any any"}], "dich_vu": {"telnet": false}});
        assert_eq!(check_schema(&bad), vec!["acl[0].quy_tac thiếu hoặc sai kiểu".to_string()]);
    }
}
