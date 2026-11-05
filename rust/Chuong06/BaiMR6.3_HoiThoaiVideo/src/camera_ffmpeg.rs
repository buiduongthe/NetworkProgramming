//! Camera thật qua tiến trình `ffmpeg` (không cần thư viện ngoài trong chương trình).
//!
//! ffmpeg đọc camera bằng trình điều khiển của từng hệ điều hành rồi xuất khung RGB thô ra stdout:
//!   - Windows: `-f dshow -i video="Tên camera"` (DirectShow); liệt kê bằng
//!     `ffmpeg -hide_banner -list_devices true -f dshow -i dummy` (đọc stderr, dòng `"Tên" (video)`);
//!   - Linux:   `-f v4l2 -i /dev/video0` (Video4Linux2); liệt kê = các tệp /dev/video*;
//!   - macOS:   `-f avfoundation -i "0"`; liệt kê bằng `-f avfoundation -list_devices true -i ""`.
//! Bộ lọc `-vf scale=160:120,fps=N -pix_fmt rgb24 -f rawvideo pipe:1` cho đúng khung 160×120×3 byte
//! mà đường gửi của bài dùng (giảm độ phân giải, nén zlib, chia mảnh). Một luồng nền đọc stdout của
//! ffmpeg, mỗi lần đủ 57 600 byte là một khung mới (chỉ giữ khung mới nhất).
//! Thêm mục "Mẫu thử ffmpeg (testsrc)" (`-f lavfi -i testsrc`) để kiểm tra đường ffmpeg khi máy
//! không có camera.

use crate::chung::{CAO, RONG};
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub const MAU_THU: &str = "Mẫu thử ffmpeg (testsrc)";

fn lenh_ffmpeg() -> Command {
    #[allow(unused_mut)] // chỉ Windows cần sửa c
    let mut c = Command::new("ffmpeg");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: không bật cửa sổ console cho ffmpeg
    }
    c
}

/// Có ffmpeg trong PATH không.
pub fn co_ffmpeg() -> bool {
    lenh_ffmpeg().arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// Danh sách camera thật (tên dùng cho tham số -i). Chạy ở luồng nền: trên Windows mất ~1 s.
pub fn liet_ke() -> Vec<String> {
    if !co_ffmpeg() {
        return Vec::new();
    }
    let mut ds = Vec::new();
    if cfg!(target_os = "windows") {
        if let Ok(out) = lenh_ffmpeg().args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"]).output() {
            for dong in String::from_utf8_lossy(&out.stderr).lines() {
                // [dshow @ 0000...] "HP HD Camera" (video)
                if dong.contains("(video)") {
                    if let Some(ten) = dong.split('"').nth(1) {
                        ds.push(ten.to_string());
                    }
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(out) = lenh_ffmpeg().args(["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""]).output() {
            let mut trong_video = false;
            for dong in BufReader::new(&out.stderr[..]).lines().map_while(Result::ok) {
                if dong.contains("video devices") {
                    trong_video = true;
                } else if dong.contains("audio devices") {
                    trong_video = false;
                } else if trong_video {
                    // [AVFoundation indev @ 0x...] [0] FaceTime HD Camera
                    if let Some(i) = dong.rfind("] ") {
                        ds.push(dong[i + 2..].to_string());
                    }
                }
            }
        }
    } else if let Ok(rd) = std::fs::read_dir("/dev") {
        let mut v: Vec<String> = rd.flatten().map(|e| e.path().to_string_lossy().into_owned()).filter(|p| p.starts_with("/dev/video")).collect();
        v.sort();
        ds = v;
    }
    ds.push(MAU_THU.to_string());
    ds
}

/// Một tiến trình ffmpeg đang thu camera.
pub struct CameraFfmpeg {
    con: Child,
    moi_nhat: Arc<Mutex<Option<Vec<u8>>>>,
}

impl CameraFfmpeg {
    pub fn mo(ten: &str, fps: u32) -> std::io::Result<CameraFfmpeg> {
        let mut c = lenh_ffmpeg();
        c.args(["-hide_banner", "-loglevel", "error"]);
        if ten == MAU_THU {
            c.args(["-re", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=30"]);
        } else if cfg!(target_os = "windows") {
            c.args(["-f", "dshow", "-i"]).arg(format!("video={ten}"));
        } else if cfg!(target_os = "macos") {
            c.args(["-f", "avfoundation", "-framerate", "30", "-i", ten]);
        } else {
            c.args(["-f", "v4l2", "-i", ten]);
        }
        c.args(["-vf", &format!("scale={RONG}:{CAO},fps={fps}"), "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"]);
        let mut con = c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
        let mut ra = con.stdout.take().unwrap();
        let moi_nhat = Arc::new(Mutex::new(None));
        let m = Arc::clone(&moi_nhat);
        std::thread::spawn(move || {
            let mut khung = vec![0u8; RONG * CAO * 3];
            // read_exact: đọc đủ đúng một khung; ffmpeg thoát -> lỗi -> luồng kết thúc
            while ra.read_exact(&mut khung).is_ok() {
                *m.lock().unwrap() = Some(khung.clone());
            }
        });
        Ok(CameraFfmpeg { con, moi_nhat })
    }

    /// Lấy khung mới nhất (nếu có khung mới kể từ lần lấy trước).
    pub fn lay(&self) -> Option<Vec<u8>> {
        self.moi_nhat.lock().unwrap().take()
    }

    /// ffmpeg đã thoát (camera bận, sai tên…)?
    pub fn da_dung(&mut self) -> bool {
        !matches!(self.con.try_wait(), Ok(None))
    }
}

impl Drop for CameraFfmpeg {
    fn drop(&mut self) {
        let _ = self.con.kill(); // dừng thu = dừng tiến trình ffmpeg (đèn camera tắt)
        let _ = self.con.wait();
    }
}
