//! Phân tích bản ghi WAV: âm nào có mặt, ở mức bao nhiêu, trong từng đoạn.
//!
//! Mỗi client phát một âm sin có tần số riêng, nên đo mức ở từng tần số bằng thuật toán Goertzel
//! là biết trong bản ghi "ai đang nói" và to bao nhiêu. Bản ghi được chia đoạn theo các mốc sự kiện
//! mà client đã lưu (tệp <tên>_moc.json); bỏ qua 0,15 s đầu mỗi đoạn vì âm thanh tới tai người nghe
//! trễ hơn thông báo TCP (độ sâu bộ đệm jitter + một nhịp trộn). Nếu không có tệp mốc, chia đều mỗi
//! --cua-so giây.

use crate::chung::{chu_db, doc_wav, muc_am_dbfs, TAN_SO_MAU};
use crate::Args;
use serde_json::Value;
use std::path::Path;

const BO_DAU: f64 = 0.15; // giây bỏ qua ở đầu mỗi đoạn (độ trễ)
const NGAN_NHAT: f64 = 0.3; // đoạn ngắn hơn (sau khi bỏ đầu) thì gộp vào mốc kế tiếp
const VANG: f64 = f64::NEG_INFINITY;

/// In số như {:g} của Python: 440.0 -> "440".
fn g(x: f64) -> String {
    if x.fract() == 0.0 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

pub fn run(a: Args) {
    if a.pos.len() < 2 {
        eprintln!("Cách dùng: phan_tich <tệp.wav> <tần_số:nhãn> ... [--cua-so 1.0] [--nguong -50]");
        std::process::exit(2);
    }
    let wav = &a.pos[0];
    let am: Vec<(f64, String)> = a.pos[1..]
        .iter()
        .filter_map(|x| x.split_once(':').and_then(|(f, n)| Some((f.parse().ok()?, n.to_string()))))
        .collect();
    let (cua_so, nguong) = (a.f64("cua-so", 1.0), a.f64("nguong", -50.0));
    let mau = doc_wav(wav).unwrap_or_else(|e| {
        eprintln!("[phan_tich] {wav}: {e}");
        std::process::exit(1)
    });
    let dai = mau.len() as f64 / TAN_SO_MAU as f64;
    println!("[phan_tich] {wav}: {dai:.2} s, 16000 Hz, 16 bit, mono");
    println!("[phan_tich] Goertzel, cửa sổ Hann; RMS dBFS; --- = vắng (<{})", g(nguong));

    // ---- Mốc chia đoạn
    let duong_moc = Path::new(wav).with_extension("").to_string_lossy().into_owned() + "_moc.json";
    let mut ten_minh = None;
    let moc: Vec<(f64, String)> = match std::fs::read_to_string(&duong_moc).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        Some(d) => {
            ten_minh = d["ten"].as_str().map(String::from);
            let mut v = vec![(0.0, "bắt đầu ghi".to_string())];
            for m in d["moc"].as_array().cloned().unwrap_or_default() {
                v.push((m["t"].as_f64().unwrap_or(0.0), m["su_kien"].as_str().unwrap_or("").to_string()));
            }
            v
        }
        None => (0..(dai / cua_so) as usize).map(|i| (i as f64 * cua_so, format!("cửa sổ {}", i + 1))).collect(),
    };
    let mut nhom: Vec<(f64, Vec<String>)> = Vec::new(); // gộp các mốc quá gần nhau thành một
    for (t, s) in moc {
        match nhom.last_mut() {
            Some(last) if t - last.0 < BO_DAU + NGAN_NHAT => last.1.push(s),
            _ => nhom.push((t, vec![s])),
        }
    }

    // ---- Đo mức từng âm trong từng đoạn
    let tieu_de: String = am.iter().map(|(f, n)| format!("{n:>2} {:<4}", g(*f))).collect();
    println!("Đoạn  Từ-đến (s) {tieu_de} Thay đổi");
    let mut truoc: Option<Vec<f64>> = None;
    let mut ket_qua: Vec<Vec<f64>> = Vec::new();
    for (i, (t, _)) in nhom.iter().enumerate() {
        let dau = t + BO_DAU;
        let cuoi = nhom.get(i + 1).map_or(dai, |x| x.0);
        if cuoi - dau < 0.1 {
            continue;
        }
        let doan = &mau[(dau * TAN_SO_MAU as f64) as usize..((cuoi * TAN_SO_MAU as f64) as usize).min(mau.len())];
        let muc: Vec<f64> = am.iter().map(|(f, _)| muc_am_dbfs(doan, *f)).map(|m| if m >= nguong { m } else { VANG }).collect();
        let mut doi = Vec::new();
        if let Some(tr) = &truoc {
            for (((_, n), m0), m1) in am.iter().zip(tr).zip(&muc) {
                if *m0 == VANG && *m1 > VANG {
                    doi.push(format!("{n} bật"));
                } else if *m0 > VANG && *m1 == VANG {
                    doi.push(format!("{n} tắt"));
                } else if *m0 > VANG && (m1 - m0).abs() >= 1.0 {
                    doi.push(format!("{n} {:+.1}dB", m1 - m0));
                }
            }
        }
        let cot: String = muc.iter().map(|m| chu_db(*m, 7)).collect();
        println!("{:3}   {dau:4.2}-{cuoi:4.2} {cot}  {}", i + 1, doi.join(", "));
        truoc = Some(muc.clone());
        ket_qua.push(muc);
    }
    println!("Sự kiện mở đầu mỗi đoạn:");
    for (i, (t, ds)) in nhom.iter().enumerate() {
        println!("{:3} {t:5.2}s {}", i + 1, ds.join("; "));
    }

    // ---- Kiểm tra mix-minus: người nghe không được nghe tiếng của chính mình
    for (j, (f, n)) in am.iter().enumerate() {
        if Some(n) == ten_minh.as_ref() {
            let lon_nhat = ket_qua.iter().map(|m| m[j]).fold(VANG, f64::max);
            let kl = if lon_nhat == VANG { "vắng mọi đoạn -> ĐÚNG".to_string() } else { format!("{lon_nhat:.1} dBFS -> SAI") };
            println!("Mix-minus: {n} không nghe tiếng mình ({} Hz): {kl}", g(*f));
        }
    }
}
