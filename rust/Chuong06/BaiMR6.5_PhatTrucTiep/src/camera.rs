//! Bài mở rộng MR6.5 – Nguồn camera của máy phát: camera thật qua ffmpeg, mẫu thử testsrc hoặc
//! camera giả lập "bảng clapper". Chép từ MR6.3, sửa: lay_khung(số khung, nhãn ms) và camera giả lập
//! là NguonClapper (đồng hồ + chớp trắng mỗi giây tròn).
//!
//! Không cần thư viện ngoài: chương trình chạy tiến trình `ffmpeg` (công cụ dòng lệnh) và đọc khung
//! hình thô từ ống (pipe) stdout của nó:
//!     ffmpeg -f <dshow|v4l2|avfoundation> -i <camera> -vf scale=160:120,fps=<fps>
//!            -pix_fmt rgb24 -f rawvideo pipe:1
//! Mỗi khung đúng 160 × 120 × 3 = 57 600 byte RGB, đưa thẳng vào đường gửi (mã hóa I/P bằng zlib,
//! chia mảnh). Một luồng nền đọc ống liên tục và chỉ giữ khung MỚI NHẤT – video trực tiếp không cần
//! khung cũ; vòng gửi lấy khung mới nhất theo fps của máy phát.
//! Liệt kê camera: Windows `ffmpeg -list_devices true -f dshow -i dummy` (đọc stderr), Linux /dev/video*,
//! macOS `ffmpeg -f avfoundation -list_devices true -i ""`.
//! Không có ffmpeg -> chỉ còn "Camera giả lập" (NguonClapper của chung.rs); ffmpeg dừng/lỗi (không có
//! camera, camera bận…) -> lõi máy phát tự chuyển sang camera giả lập.

use crate::chung::{NguonClapper, CAO, FPS_MAC_DINH, RONG};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub const KICH_THUOC: usize = RONG * CAO * 3;
pub const GIA_LAP: &str = "gia_lap"; // hai "camera" không phải thiết bị thật
pub const MAU_THU: &str = "testsrc";

/// Giao diện chung của mọi nguồn hình (giả lập, ffmpeg, tệp video).
pub trait Camera: Send {
    /// Khung RGB 160×120 (None: chưa có khung).
    fn lay_khung(&mut self, so: u32, ts_ms: u64) -> Option<Vec<u8>>;
    /// Lỗi của nguồn ("" = đang chạy tốt); khác rỗng -> máy phát lùi về giả lập.
    fn loi(&self) -> String {
        String::new()
    }
    fn dat_fps(&mut self, _fps: u32) {}
    fn mo_ta(&self) -> String;
    fn that(&self) -> bool {
        false
    }
    fn dong(&mut self) {}
}

pub type CamChung = Arc<Mutex<Box<dyn Camera>>>;

pub fn cam_chung(c: impl Camera + 'static) -> CamChung {
    Arc::new(Mutex::new(Box::new(c)))
}

/// Lệnh ffmpeg không bật cửa sổ console trên Windows.
pub fn lenh_ffmpeg() -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new("ffmpeg");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Có ffmpeg trong PATH không.
pub fn co_ffmpeg() -> bool {
    lenh_ffmpeg().arg("-version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// Danh sách (mã thiết bị, tên hiển thị) camera thật tìm được qua ffmpeg.
pub fn liet_ke_camera() -> Vec<(String, String)> {
    if !co_ffmpeg() {
        return Vec::new();
    }
    let mut ds: Vec<(String, String)> = Vec::new();
    if cfg!(target_os = "windows") {
        let Ok(out) = lenh_ffmpeg().args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"]).stdin(Stdio::null()).output() else {
            return ds;
        };
        let mut trong_muc_video = false;
        for dong in String::from_utf8_lossy(&out.stderr).lines() {
            if dong.contains("DirectShow video devices") {
                trong_muc_video = true; // định dạng ffmpeg cũ (theo mục)
            } else if dong.contains("DirectShow audio devices") {
                trong_muc_video = false;
            }
            let ten = dong.split('"').nth(1).map(str::to_string);
            if dong.contains("(video)") {
                // định dạng mới: [dshow @ ...] "Tên" (video)
                if let Some(t) = ten {
                    ds.push((t.clone(), t));
                }
            } else if trong_muc_video && !dong.contains("Alternative name") && !dong.contains("(audio)") {
                if let Some(t) = ten {
                    ds.push((t.clone(), t));
                }
            }
        }
        ds.dedup();
    } else if cfg!(target_os = "macos") {
        let Ok(out) = lenh_ffmpeg().args(["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""]).stdin(Stdio::null()).output() else {
            return ds;
        };
        let mut trong_muc_video = false;
        for dong in String::from_utf8_lossy(&out.stderr).lines() {
            if dong.contains("video devices") {
                trong_muc_video = true;
            } else if dong.contains("audio devices") {
                trong_muc_video = false;
            } else if trong_muc_video {
                // [AVFoundation indev @ 0x...] [0] FaceTime HD Camera
                if let Some(i) = dong.rfind("] [") {
                    let sau = &dong[i + 3..];
                    if let Some((so, ten)) = sau.split_once("] ") {
                        ds.push((so.to_string(), ten.trim().to_string()));
                    }
                }
            }
        }
    } else if let Ok(rd) = std::fs::read_dir("/dev") {
        let mut v: Vec<String> = rd.flatten().map(|e| e.path().to_string_lossy().into_owned()).filter(|p| p.starts_with("/dev/video")).collect();
        v.sort();
        ds = v.into_iter().map(|p| (p.clone(), p)).collect();
    }
    ds
}

/// Tham số ffmpeg thu camera `ma` (hoặc ảnh mẫu thử testsrc) ra khung RGB thô 160×120.
fn tham_so_ffmpeg(ma: &str, fps: u32) -> Vec<String> {
    let mut v: Vec<String> = ["-hide_banner", "-loglevel", "error"].map(String::from).to_vec();
    if ma == MAU_THU {
        v.extend(["-re", "-f", "lavfi", "-i"].map(String::from));
        v.push(format!("testsrc=size={RONG}x{CAO}:rate={fps}"));
    } else if cfg!(target_os = "windows") {
        v.extend(["-f", "dshow", "-i"].map(String::from));
        v.push(format!("video={ma}"));
    } else if cfg!(target_os = "macos") {
        v.extend(["-f", "avfoundation", "-framerate", "30", "-i", ma].map(String::from));
    } else {
        v.extend(["-f", "v4l2", "-i", ma].map(String::from));
    }
    v.push("-vf".into());
    v.push(format!("scale={RONG}:{CAO},fps={fps}"));
    v.extend(["-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"].map(String::from));
    v
}

/// Bọc NguonClapper theo cùng giao diện với CameraFfmpeg.
pub struct CameraGiaLap {
    nguon: NguonClapper,
}

impl CameraGiaLap {
    pub fn new(ten: &str, ma: u32, fps: u32) -> CameraGiaLap {
        CameraGiaLap { nguon: NguonClapper::new(ten, ma, fps) }
    }
}

impl Camera for CameraGiaLap {
    fn lay_khung(&mut self, so: u32, ts_ms: u64) -> Option<Vec<u8>> {
        Some(self.nguon.lay_khung(so, ts_ms))
    }
    fn dat_fps(&mut self, fps: u32) {
        self.nguon.fps = fps;
    }
    fn mo_ta(&self) -> String {
        "GIẢ LẬP – bảng clapper".into()
    }
}

/// Đọc stderr còn lại của tiến trình: dòng cuối, hoặc `mac_dinh` nếu trống.
pub fn dong_loi_cuoi(con: &mut Option<std::process::ChildStderr>, mac_dinh: &str) -> String {
    let mut s = String::new();
    if let Some(mut e) = con.take() {
        let _ = e.read_to_string(&mut s);
    }
    s.trim().lines().last().map(str::to_string).unwrap_or_else(|| mac_dinh.to_string())
}

/// Camera qua tiến trình ffmpeg; luồng nền đọc từng khung 57 600 byte, giữ khung mới nhất.
pub struct CameraFfmpeg {
    that: bool,
    mo_ta: String,
    con: Child,
    moi_nhat: Arc<Mutex<(Option<Vec<u8>>, String)>>, // (khung mới nhất, lỗi)
}

impl CameraFfmpeg {
    pub fn mo(ma: &str, ten: &str, fps: u32) -> std::io::Result<CameraFfmpeg> {
        let that = ma != MAU_THU;
        let mo_ta = if that { format!("THẬT – {ten} (ffmpeg)") } else { "MẪU THỬ ffmpeg testsrc (không phải camera)".to_string() };
        let mut con = lenh_ffmpeg().args(tham_so_ffmpeg(ma, fps)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let mut ra = con.stdout.take().unwrap();
        let mut err = con.stderr.take();
        let moi_nhat = Arc::new(Mutex::new((None, String::new())));
        let m = Arc::clone(&moi_nhat);
        std::thread::spawn(move || {
            let mut khung = vec![0u8; KICH_THUOC];
            // read_exact: đọc đủ đúng một khung; ffmpeg kết thúc / lỗi mở camera -> thoát vòng
            while ra.read_exact(&mut khung).is_ok() {
                m.lock().unwrap().0 = Some(khung.clone());
            }
            let loi = dong_loi_cuoi(&mut err, "ffmpeg đã dừng");
            m.lock().unwrap().1 = loi;
        });
        Ok(CameraFfmpeg { that, mo_ta, con, moi_nhat })
    }
}

impl Camera for CameraFfmpeg {
    fn lay_khung(&mut self, _so: u32, _ts_ms: u64) -> Option<Vec<u8>> {
        self.moi_nhat.lock().unwrap().0.clone() // None: chưa có khung nào
    }
    fn loi(&self) -> String {
        self.moi_nhat.lock().unwrap().1.clone()
    }
    fn mo_ta(&self) -> String {
        self.mo_ta.clone()
    }
    fn that(&self) -> bool {
        self.that
    }
    fn dong(&mut self) {
        let _ = self.con.kill(); // dừng thu = dừng tiến trình ffmpeg (đèn camera tắt)
        let _ = self.con.wait();
    }
}

impl Drop for CameraFfmpeg {
    fn drop(&mut self) {
        self.dong();
    }
}

/// Mở camera đã chọn; không có ffmpeg (hoặc ffmpeg không chạy được) -> camera giả lập.
pub fn mo_camera(ma_thiet_bi: &str, ten_hien_thi: &str, ten_may_phat: &str, ma_may_phat: u32, fps: u32) -> CamChung {
    if ma_thiet_bi != GIA_LAP && co_ffmpeg() {
        if let Ok(c) = CameraFfmpeg::mo(ma_thiet_bi, ten_hien_thi, fps) {
            return cam_chung(c);
        }
    }
    cam_chung(CameraGiaLap::new(ten_may_phat, ma_may_phat, if fps == 0 { FPS_MAC_DINH } else { fps }))
}
