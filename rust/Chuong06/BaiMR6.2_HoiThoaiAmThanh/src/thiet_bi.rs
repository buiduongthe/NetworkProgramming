//! Micro và loa thật qua crate `cpal` (WASAPI trên Windows, ALSA trên Linux, CoreAudio trên macOS),
//! cùng nguồn giả lập (sóng sin) khi không có / không chọn thiết bị thật.
//!
//! Thiết bị thật hiếm khi chạy đúng 16 kHz mono 16 bit như giao thức của bài: Windows thường cho
//! 48 kHz, 2 kênh, số thực f32. Vì vậy:
//!   - thu: trộn các kênh về mono, đổi tần số lấy mẫu về 16 kHz, đổi sang i16 rồi đẩy vào HÀNG ĐỢI;
//!   - phát: lấy mẫu 16 kHz từ hàng đợi, nội suy tuyến tính lên tần số của loa, chép ra mọi kênh.
//! Hàm callback của cpal chạy trên luồng âm thanh riêng của hệ điều hành – chỉ được làm việc ngắn
//! (khóa hàng đợi, chép mẫu), không chạm giao diện, không gọi mạng.
//! Luồng 20 ms của chương trình lấy/đẩy đúng 320 mẫu mỗi nhịp, nên hàng đợi là chỗ "gặp nhau" giữa
//! đồng hồ của card âm thanh và đồng hồ của chương trình; hàng đợi có giới hạn để độ trễ không tăng mãi.
//!
//! `cpal::Stream` không chuyển được sang luồng khác trên mọi nền tảng (không Send), nên luồng âm
//! thanh do luồng giao diện tạo và giữ; dừng = drop đối tượng Stream.

use crate::chung::{cat_ngon, MAU_MOI_KHUNG, TAN_SO_MAU};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Hàng đợi mẫu PCM 16 kHz mono dùng chung giữa callback cpal và luồng 20 ms.
pub type HangMau = Arc<Mutex<VecDeque<i16>>>;

pub fn hang_moi() -> HangMau {
    Arc::new(Mutex::new(VecDeque::new()))
}

const TOI_DA_THU: usize = TAN_SO_MAU / 2; // giữ tối đa 0,5 s mẫu thu chưa gửi
const TOI_DA_PHAT: usize = TAN_SO_MAU * 3 / 10; // giữ tối đa 0,3 s mẫu chờ phát

/// Đẩy mẫu vào hàng đợi; quá giới hạn thì bỏ mẫu cũ nhất (giữ độ trễ thấp).
pub fn day_vao(hang: &HangMau, mau: &[i16], toi_da: usize) {
    let mut h = hang.lock().unwrap();
    h.extend(mau.iter().copied());
    while h.len() > toi_da {
        h.pop_front();
    }
}

/// Đẩy một khung vào hàng đợi phát của loa.
pub fn day_ra_loa(hang: &HangMau, mau: &[i16]) {
    day_vao(hang, mau, TOI_DA_PHAT);
}

// ---------------------------------------------------------------- Nguồn micro
/// Nguồn âm cho micro: sóng sin giả lập (pha liên tục) hoặc hàng đợi do callback cpal nạp.
pub enum NguonMic {
    GiaLap { tan_so: f64, n: u64 },
    ThietBi(HangMau),
}

impl NguonMic {
    pub fn gia_lap(tan_so: f64) -> NguonMic {
        NguonMic::GiaLap { tan_so, n: 0 }
    }

    /// Lấy đúng 320 mẫu (20 ms). Thiết bị chưa đủ mẫu thì đệm khoảng lặng.
    pub fn lay_khung(&mut self) -> Vec<i16> {
        match self {
            NguonMic::GiaLap { tan_so, n } => {
                // giống nguồn sin của client dòng lệnh: biên độ 0,25 toàn thang
                let w = 2.0 * std::f64::consts::PI * *tan_so / TAN_SO_MAU as f64;
                let a = 0.25 * 32767.0;
                let k = (0..MAU_MOI_KHUNG).map(|i| (a * (w * (*n + i as u64) as f64).sin()).round_ties_even() as i16).collect();
                *n += MAU_MOI_KHUNG as u64;
                k
            }
            NguonMic::ThietBi(hang) => {
                let mut h = hang.lock().unwrap();
                (0..MAU_MOI_KHUNG).map(|_| h.pop_front().unwrap_or(0)).collect()
            }
        }
    }
}

/// Nhân hệ số (âm lượng micro, âm lượng nghe…) rồi cắt ngọn về 16 bit.
pub fn nhan_he_so(mau: &[i16], g: f64) -> Vec<i16> {
    if g == 1.0 {
        return mau.to_vec();
    }
    mau.iter().map(|&x| cat_ngon(x as f64 * g)).collect()
}

// ---------------------------------------------------------------- Liệt kê thiết bị
/// Linux: máy không có card âm thanh (máy chủ, container) thì thư viện ALSA in hàng loạt lỗi ra
/// stderr khi dò thiết bị; kiểm tra /proc/asound/cards trước để chuyển thẳng sang giả lập.
/// Windows/macOS: luôn hỏi hệ điều hành qua cpal.
fn co_card_am_thanh() -> bool {
    if cfg!(target_os = "linux") {
        return std::fs::read_to_string("/proc/asound/cards").map(|s| !s.contains("no soundcards") && !s.trim().is_empty()).unwrap_or(false);
    }
    true
}

/// Tên các micro thật. Gọi ở luồng nền: trên một số hệ thống việc dò thiết bị mất vài trăm ms.
pub fn ds_micro() -> Vec<String> {
    if !co_card_am_thanh() {
        return Vec::new();
    }
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(ds) => ds.filter_map(|d| d.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

fn tim_micro(ten: &str) -> Option<cpal::Device> {
    cpal::default_host().input_devices().ok()?.find(|d| d.name().map(|n| n == ten).unwrap_or(false))
}

// ---------------------------------------------------------------- Đổi tần số lấy mẫu
/// Đổi tần số lấy mẫu của luồng thu về 16 kHz.
/// Giảm tần số (48 kHz -> 16 kHz): lấy trung bình các mẫu trong mỗi khoảng `buoc` – một bộ lọc
/// thông thấp thô để bớt hiện tượng chồng phổ (aliasing). Tăng tần số (8 kHz -> 16 kHz): lặp mẫu.
struct DoiTanSoThu {
    buoc: f64, // số mẫu vào ứng với một mẫu ra
    vi_tri: f64,
    tong: f32,
    dem: u32,
}

impl DoiTanSoThu {
    fn new(tan_so_vao: u32) -> DoiTanSoThu {
        DoiTanSoThu { buoc: tan_so_vao as f64 / TAN_SO_MAU as f64, vi_tri: 0.0, tong: 0.0, dem: 0 }
    }

    fn day(&mut self, x: f32, ra: &mut Vec<i16>) {
        let mau = |v: f32| cat_ngon(v as f64 * 32767.0);
        if self.buoc >= 1.0 {
            self.tong += x;
            self.dem += 1;
            self.vi_tri += 1.0;
            if self.vi_tri >= self.buoc {
                self.vi_tri -= self.buoc;
                ra.push(mau(self.tong / self.dem as f32));
                self.tong = 0.0;
                self.dem = 0;
            }
        } else {
            while self.vi_tri < 1.0 {
                ra.push(mau(x));
                self.vi_tri += self.buoc;
            }
            self.vi_tri -= 1.0;
        }
    }
}

// ---------------------------------------------------------------- Mở micro
/// Mở micro thật tên `ten`; mẫu 16 kHz mono được đẩy vào `hang`. Trả về luồng (giữ để còn thu)
/// và mô tả định dạng gốc của thiết bị.
pub fn mo_micro(ten: &str, hang: HangMau) -> Result<(cpal::Stream, String), String> {
    let dev = tim_micro(ten).ok_or_else(|| format!("không tìm thấy micro \"{ten}\""))?;
    let cfg = dev.default_input_config().map_err(|e| e.to_string())?;
    let mo_ta = format!("{} Hz, {} kênh, {:?}", cfg.sample_rate().0, cfg.channels(), cfg.sample_format());
    let sc: cpal::StreamConfig = cfg.config();
    let stream = match cfg.sample_format() {
        SampleFormat::F32 => tao_luong_thu::<f32>(&dev, &sc, hang),
        SampleFormat::I16 => tao_luong_thu::<i16>(&dev, &sc, hang),
        SampleFormat::U16 => tao_luong_thu::<u16>(&dev, &sc, hang),
        SampleFormat::I32 => tao_luong_thu::<i32>(&dev, &sc, hang),
        f => return Err(format!("định dạng mẫu {f:?} chưa hỗ trợ")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, mo_ta))
}

fn tao_luong_thu<T>(dev: &cpal::Device, sc: &cpal::StreamConfig, hang: HangMau) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let kenh = sc.channels.max(1) as usize;
    let mut doi = DoiTanSoThu::new(sc.sample_rate.0);
    let mut ra = Vec::with_capacity(4096);
    dev.build_input_stream(
        sc,
        move |du_lieu: &[T], _: &cpal::InputCallbackInfo| {
            // callback chạy trên luồng âm thanh: trộn kênh -> mono -> 16 kHz -> hàng đợi
            ra.clear();
            for khung in du_lieu.chunks(kenh) {
                let x = khung.iter().map(|&s| f32::from_sample(s)).sum::<f32>() / khung.len() as f32;
                doi.day(x, &mut ra);
            }
            day_vao(&hang, &ra, TOI_DA_THU);
        },
        |e| eprintln!("lỗi luồng micro: {e}"),
        None,
    )
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- Mở loa
/// Mở loa mặc định; mẫu 16 kHz lấy từ `hang` được nội suy lên tần số của loa.
pub fn mo_loa(hang: HangMau) -> Result<(cpal::Stream, String), String> {
    if !co_card_am_thanh() {
        return Err("máy không có card âm thanh".into());
    }
    let dev = cpal::default_host().default_output_device().ok_or("không có loa (thiết bị phát)")?;
    let ten = dev.name().unwrap_or_else(|_| "loa mặc định".into());
    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
    let mo_ta = format!("{ten}: {} Hz, {} kênh", cfg.sample_rate().0, cfg.channels());
    let sc: cpal::StreamConfig = cfg.config();
    let stream = match cfg.sample_format() {
        SampleFormat::F32 => tao_luong_phat::<f32>(&dev, &sc, hang),
        SampleFormat::I16 => tao_luong_phat::<i16>(&dev, &sc, hang),
        SampleFormat::U16 => tao_luong_phat::<u16>(&dev, &sc, hang),
        SampleFormat::I32 => tao_luong_phat::<i32>(&dev, &sc, hang),
        f => return Err(format!("định dạng mẫu {f:?} chưa hỗ trợ")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, mo_ta))
}

fn tao_luong_phat<T>(dev: &cpal::Device, sc: &cpal::StreamConfig, hang: HangMau) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let kenh = sc.channels.max(1) as usize;
    let buoc = TAN_SO_MAU as f64 / sc.sample_rate.0 as f64; // mẫu 16 kHz ứng với một mẫu ra
    let (mut truoc, mut sau, mut vi_tri) = (0.0f32, 0.0f32, 0.0f64);
    dev.build_output_stream(
        sc,
        move |ra: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut h = hang.lock().unwrap();
            for khung in ra.chunks_mut(kenh) {
                // nội suy tuyến tính giữa hai mẫu 16 kHz liên tiếp
                let x = truoc + (sau - truoc) * vi_tri as f32;
                for s in khung.iter_mut() {
                    *s = T::from_sample(x);
                }
                vi_tri += buoc;
                while vi_tri >= 1.0 {
                    vi_tri -= 1.0;
                    truoc = sau;
                    sau = h.pop_front().map_or(0.0, |v| v as f32 / 32768.0); // hết mẫu -> khoảng lặng
                }
            }
        },
        |e| eprintln!("lỗi luồng loa: {e}"),
        None,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doi_48k_ve_16k() {
        let mut d = DoiTanSoThu::new(48000);
        let mut ra = Vec::new();
        for _ in 0..4800 {
            d.day(0.5, &mut ra);
        }
        assert_eq!(ra.len(), 1600); // 0,1 s
        assert!(ra.iter().all(|&x| x == cat_ngon(0.5 * 32767.0)));
    }
}
