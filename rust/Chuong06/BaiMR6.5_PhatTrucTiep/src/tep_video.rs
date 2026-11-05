//! Bài mở rộng MR6.5 – Nguồn "tệp video" cho máy phát: phát lại hình + tiếng của một tệp (mp4, mkv,
//! avi, webm…) như một buổi phát trực tiếp (giống OBS "Media Source" hay `ffmpeg -re … rtmp://…`).
//!
//! Hai tiến trình ffmpeg cùng đọc tệp với `-re` (đọc theo tốc độ thật, không nhanh hơn thời gian phát)
//! và `-stream_loop -1` (hết tệp thì phát lại từ đầu):
//!     hình:  ffmpeg -re -stream_loop -1 -i <tệp> -an -vf scale=160:120:force_original_aspect_ratio=decrease,
//!                   pad=160:120:(ow-iw)/2:(oh-ih)/2,fps=<fps> -pix_fmt rgb24 -f rawvideo pipe:1
//!     tiếng: ffmpeg -re -stream_loop -1 -i <tệp> -vn -ac 1 -ar 16000 -f s16le pipe:1
//! Bộ lọc `fps=<fps>` cho đúng <fps> khung mỗi giây THỜI GIAN CỦA TỆP, nên khung thứ i có thời điểm trong
//! tệp i × 1000 / fps ms; tiếng có 16 000 mẫu mỗi giây của tệp.
//!
//! Đồng bộ tiếng-hình ở phía phát. Hai tiến trình khởi động lệch nhau vài chục – vài trăm ms; nếu cứ gửi
//! "khung mới nhất" thì hình và tiếng của TỆP lệch nhau ngay từ máy phát, người xem đồng bộ theo nhãn
//! thời gian cũng không sửa được. Cách làm ở đây (giống bộ ghép kênh – muxer – của bộ mã hóa thật):
//!   - Tiếng là đồng hồ chủ: vòng 20 ms của máy phát lấy lần lượt 320 mẫu của tệp. Chưa đủ mẫu (ffmpeg
//!     chưa chạy kịp) -> gửi im lặng và `lech_ms` += 20; dồn quá 300 ms -> bỏ bớt 20 ms và `lech_ms` −= 20.
//!     Vậy ở thời điểm phương tiện T máy phát đang gửi tiếng của giây T − lech_ms trong tệp.
//!   - Hình: luồng đọc giữ ~3 s khung gần nhất kèm thời điểm trong tệp; khi máy phát chụp khung ở thời
//!     điểm phương tiện T, chọn khung có thời điểm trong tệp ≤ T − lech_ms gần nhất. Nhờ vậy nhãn thời gian
//!     gửi đi khớp với nội dung của tệp, và cơ chế đồng bộ ở người xem (MR6.5) hoạt động như với camera.
//! Tệp không có tiếng (hoặc chọn micro khác) -> hình lấy khung mới nhất.

use crate::camera::{cam_chung, co_ffmpeg, dong_loi_cuoi, lenh_ffmpeg, CamChung, Camera, KICH_THUOC};
use crate::chung::{CAO, KHUNG_MS, MAU_MOI_KHUNG, RONG, TAN_SO_MAU};
use crate::thiet_bi::{mic_chung, Mic, MicChung};
use std::collections::VecDeque;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};

pub const TEP: &str = "tep"; // mã nguồn "tệp video" trong danh sách camera/micro
const BYTE_MOI_KHUNG_AM: usize = MAU_MOI_KHUNG * 2; // 320 mẫu × 2 byte (s16le)
const DON_TOI_DA_MS: usize = 300; // tiếng của tệp dồn quá mức này thì bỏ bớt
const GIU_HINH_MS: i64 = 3000; // giữ khung hình của 3 s gần nhất

fn lenh_hinh(duong_dan: &str, fps: u32) -> Vec<String> {
    let loc = format!("scale={RONG}:{CAO}:force_original_aspect_ratio=decrease,pad={RONG}:{CAO}:(ow-iw)/2:(oh-ih)/2,fps={fps}");
    let mut v: Vec<String> = ["-hide_banner", "-loglevel", "error", "-re", "-stream_loop", "-1", "-i", duong_dan].map(String::from).to_vec();
    v.extend(["-an", "-vf", &loc, "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"].map(String::from));
    v
}

fn lenh_tieng(duong_dan: &str) -> Vec<String> {
    let tsm = TAN_SO_MAU.to_string();
    ["-hide_banner", "-loglevel", "error", "-re", "-stream_loop", "-1", "-i", duong_dan, "-vn", "-ac", "1", "-ar", &tsm, "-f", "s16le", "pipe:1"].map(String::from).to_vec()
}

struct TepTT {
    hinh: VecDeque<(i64, Vec<u8>)>, // (ms trong tệp, khung rgb)
    tieng: VecDeque<u8>,
    so_khung_doc: i64,
    lech_ms: i64, // thời điểm phương tiện − thời điểm trong tệp (tiếng)
    dung_tieng: bool, // micro của máy phát đang là tiếng của tệp này
    loi_hinh: String,
    loi_tieng: String,
    het_tieng: bool,
}

/// Một tệp đang phát: hai tiến trình ffmpeg + bộ đệm tiếng + vòng khung hình có thời điểm trong tệp.
pub struct TepVideo {
    pub ten: String,
    tt: Arc<Mutex<TepTT>>,
    con: Mutex<Vec<Child>>,
}

impl TepVideo {
    pub fn mo(duong_dan: &str, fps: u32, co_tieng: bool) -> io::Result<Arc<TepVideo>> {
        if !co_ffmpeg() {
            return Err(io::Error::other("không có ffmpeg trong PATH"));
        }
        if !Path::new(duong_dan).is_file() {
            return Err(io::Error::other(format!("không thấy tệp {duong_dan}")));
        }
        let ten = Path::new(duong_dan).file_name().map(|t| t.to_string_lossy().into_owned()).unwrap_or_default();
        let tt = Arc::new(Mutex::new(TepTT {
            hinh: VecDeque::new(),
            tieng: VecDeque::new(),
            so_khung_doc: 0,
            lech_ms: 0,
            dung_tieng: false,
            loi_hinh: String::new(),
            loi_tieng: String::new(),
            het_tieng: !co_tieng,
        }));
        let mut ds = Vec::new();
        let mut hinh = lenh_ffmpeg().args(lenh_hinh(duong_dan, fps)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        {
            let (mut ra, mut err, t) = (hinh.stdout.take().unwrap(), hinh.stderr.take(), Arc::clone(&tt));
            let fps = fps.max(1) as i64;
            std::thread::spawn(move || {
                let mut khung = vec![0u8; KICH_THUOC];
                while ra.read_exact(&mut khung).is_ok() {
                    let mut s = t.lock().unwrap();
                    let ms = s.so_khung_doc * 1000 / fps;
                    s.so_khung_doc += 1;
                    s.hinh.push_back((ms, khung.clone()));
                    while s.hinh.front().is_some_and(|h| h.0 < ms - GIU_HINH_MS) {
                        s.hinh.pop_front();
                    }
                }
                let loi = dong_loi_cuoi(&mut err, "ffmpeg (hình) đã dừng");
                t.lock().unwrap().loi_hinh = loi;
            });
        }
        ds.push(hinh);
        if co_tieng {
            let mut tieng = lenh_ffmpeg().args(lenh_tieng(duong_dan)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
            let (mut ra, mut err, t) = (tieng.stdout.take().unwrap(), tieng.stderr.take(), Arc::clone(&tt));
            std::thread::spawn(move || {
                let mut b = vec![0u8; BYTE_MOI_KHUNG_AM];
                loop {
                    match ra.read(&mut b) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => t.lock().unwrap().tieng.extend(&b[..n]),
                    }
                }
                let loi = dong_loi_cuoi(&mut err, "tệp không có tiếng");
                let mut s = t.lock().unwrap();
                s.het_tieng = true;
                s.loi_tieng = loi;
            });
            ds.push(tieng);
        }
        Ok(Arc::new(TepVideo { ten, tt, con: Mutex::new(ds) }))
    }

    // ------------------------------------------------------------ tiếng (vòng 20 ms của máy phát)
    /// 320 mẫu tiếp theo của tệp; chưa đủ -> im lặng (lech_ms += 20).
    pub fn lay_tieng(&self) -> Vec<i16> {
        let mut s = self.tt.lock().unwrap();
        let don = BYTE_MOI_KHUNG_AM * DON_TOI_DA_MS / KHUNG_MS as usize;
        while s.tieng.len() > don {
            s.tieng.drain(..BYTE_MOI_KHUNG_AM); // dồn quá nhiều: bỏ 20 ms cũ nhất
            s.lech_ms -= KHUNG_MS as i64;
        }
        if s.tieng.len() < BYTE_MOI_KHUNG_AM {
            s.lech_ms += KHUNG_MS as i64;
            return vec![0; MAU_MOI_KHUNG];
        }
        let b: Vec<u8> = s.tieng.drain(..BYTE_MOI_KHUNG_AM).collect();
        b.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect() // s16le: byte thấp trước
    }

    // ------------------------------------------------------------ hình (vòng fps của máy phát)
    pub fn lay_hinh(&self, ts_ms: u64) -> Option<Vec<u8>> {
        let s = self.tt.lock().unwrap();
        let cuoi = s.hinh.back()?;
        if !s.dung_tieng || s.het_tieng {
            return Some(cuoi.1.clone()); // không có tiếng của tệp: khung mới nhất
        }
        let can = ts_ms as i64 - s.lech_ms; // thời điểm trong tệp ứng với tiếng đang gửi
        let mut chon = None;
        for (ms, khung) in &s.hinh {
            if *ms <= can {
                chon = Some(khung);
            } else {
                break;
            }
        }
        Some(chon.unwrap_or(&s.hinh[0].1).clone()) // hình chưa tới kịp: dùng khung sớm nhất đang có
    }

    pub fn loi_hinh(&self) -> String {
        self.tt.lock().unwrap().loi_hinh.clone()
    }

    #[allow(dead_code)]
    pub fn loi_tieng(&self) -> String {
        self.tt.lock().unwrap().loi_tieng.clone()
    }

    pub fn dong(&self) {
        for c in self.con.lock().unwrap().iter_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Phần hình của TepVideo theo giao diện camera của máy phát.
pub struct CameraTep(pub Arc<TepVideo>);

impl Camera for CameraTep {
    fn lay_khung(&mut self, _so: u32, ts_ms: u64) -> Option<Vec<u8>> {
        self.0.lay_hinh(ts_ms)
    }
    fn loi(&self) -> String {
        self.0.loi_hinh()
    }
    fn mo_ta(&self) -> String {
        format!("TỆP VIDEO – {}", self.0.ten)
    }
    fn dong(&mut self) {
        self.0.dong() // ffmpeg chạy theo fps lúc mở (dat_fps không làm gì)
    }
}

/// Phần tiếng của TepVideo theo giao diện micro của máy phát.
pub struct MicTep(pub Arc<TepVideo>);

impl Mic for MicTep {
    fn lay_khung(&mut self) -> Vec<i16> {
        self.0.lay_tieng()
    }
    fn mo_ta(&self) -> String {
        format!("TIẾNG CỦA TỆP – {}", self.0.ten)
    }
    fn dong(&mut self) {
        self.0.dong()
    }
}

/// Mở tệp video; trả về (camera của tệp, micro của tệp hoặc None, tên tệp). Lỗi -> io::Error.
pub fn mo_tep(duong_dan: &str, fps: u32, lay_tieng: bool) -> io::Result<(CamChung, Option<MicChung>, String)> {
    let tep = TepVideo::mo(duong_dan, fps, lay_tieng)?;
    let mic = if lay_tieng {
        tep.tt.lock().unwrap().dung_tieng = true;
        Some(mic_chung(MicTep(Arc::clone(&tep))))
    } else {
        None
    };
    let ten = tep.ten.clone();
    Ok((cam_chung(CameraTep(tep)), mic, ten))
}

/// Tạo tệp mp4 mẫu bằng ffmpeg (để thử khi chưa có tệp): ảnh testsrc2 chớp trắng 0,1 s đầu mỗi
/// giây, tiếng 440 Hz nhỏ + bíp 1000 Hz đúng lúc chớp – nhìn/nghe là thấy hình và tiếng có khớp.
pub fn tao_tep_mau(duong_dan: &str, giay: u32) -> io::Result<String> {
    let hinh = format!(r"testsrc2=size=320x240:rate=25:duration={giay},drawbox=x=0:y=0:w=iw:h=ih:color=white@0.85:t=fill:enable='lt(mod(t\,1)\,0.1)'");
    let tieng = format!(r"aevalsrc='0.08*sin(2*PI*440*t)+0.4*sin(2*PI*1000*t)*lt(mod(t\,1)\,0.1)':s={TAN_SO_MAU}:d={giay}");
    let chay = |ma_hoa: &str| {
        lenh_ffmpeg()
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", &hinh, "-f", "lavfi", "-i", &tieng, "-c:v", ma_hoa, "-pix_fmt", "yuv420p", "-g", "25", "-c:a", "aac", "-shortest", duong_dan])
            .stdin(Stdio::null())
            .output()
    };
    let mut r = chay("libx264")?;
    if !r.status.success() {
        r = chay("mpeg4")?; // ffmpeg thiếu libx264: dùng mpeg4
    }
    if !r.status.success() {
        let e = String::from_utf8_lossy(&r.stderr);
        return Err(io::Error::other(e.trim().lines().last().unwrap_or("ffmpeg lỗi").to_string()));
    }
    Ok(duong_dan.to_string())
}
