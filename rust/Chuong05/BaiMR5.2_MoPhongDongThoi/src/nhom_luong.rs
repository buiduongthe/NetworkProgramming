//! Nhóm luồng (thread pool) tự viết bằng kênh mpsc – thư viện chuẩn Rust không có sẵn nhóm luồng.
//!
//! n luồng worker cùng chờ trên MỘT đầu nhận của kênh (bọc trong Arc<Mutex<…>> vì Receiver không được
//! dùng chung). Mỗi việc là một hàm đóng Box<dyn FnOnce() + Send>. Khi nhóm bị hủy (drop), đầu gửi đóng
//! lại, recv() của mọi worker trả lỗi nên các luồng tự thoát, rồi drop chờ (join) từng luồng.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct NhomLuong {
    tx: Option<Sender<Job>>,
    workers: Vec<JoinHandle<()>>,
}

impl NhomLuong {
    pub fn new(n: usize) -> NhomLuong {
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let workers = (0..n)
            .map(|_| {
                let rx = Arc::clone(&rx);
                thread::spawn(move || loop {
                    let job = rx.lock().unwrap().recv(); // khóa chỉ giữ trong lúc lấy việc
                    match job {
                        Ok(f) => f(),
                        Err(_) => return, // kênh đã đóng: hết việc
                    }
                })
            })
            .collect();
        NhomLuong { tx: Some(tx), workers }
    }

    /// Gửi một việc vào hàng đợi; worker rảnh đầu tiên sẽ nhận.
    pub fn execute(&self, f: impl FnOnce() + Send + 'static) {
        self.tx.as_ref().unwrap().send(Box::new(f)).expect("nhóm luồng đã dừng");
    }
}

impl Drop for NhomLuong {
    fn drop(&mut self) {
        drop(self.tx.take()); // đóng kênh
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}
