//! Kịch bản minh họa tự động cho lookup_gui.rs (không cần người bấm phím).
//!
//! Kịch bản thao tác trên cửa sổ như người dùng: bấm Kết nối tới cổng không có server
//! (lỗi REFUSED), bấm Kết nối tới đúng cổng, nhập "TCP" và bấm Tra cứu. Trong lúc chờ server
//! trả lời chậm, kịch bản đếm số lần đồng hồ giao diện được vẽ lại và "chụp" cửa sổ (in các dòng
//! ký tự của cửa sổ); sau khi có kết quả, chụp cửa sổ lần nữa.
//! Kịch bản là một tác vụ chạy trên chính luồng giao diện, xen kẽ với vòng lặp thông điệp.

use crate::lookup_gui::{close, new_window, on_connect_click, on_lookup_click, render, run_gui, Win};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

fn log(text: &str) {
    println!("[gui] {text}");
}

fn snapshot(win: &Win, title: &str) {
    log(&format!("Ảnh cửa sổ {title}:"));
    for line in render(&win.borrow()) {
        println!("{line}");
    }
}

/// Chờ đến khi điều kiện đúng (kiểm tra mỗi 50 ms, tối đa 10 giây).
async fn wait_until(cond: impl Fn() -> bool) {
    for _ in 0..200 {
        if cond() {
            return;
        }
        sleep(Duration::from_millis(50)).await;
    }
    log("Kịch bản chờ quá lâu");
}

async fn scenario(win: Win, port: u16, bad_port: u16) {
    sleep(Duration::from_millis(500)).await;
    win.borrow_mut().txt_port = bad_port.to_string();
    on_connect_click(&win); // bấm Kết nối (cổng sai)
    wait_until(|| win.borrow().btn_connect_enabled).await;
    log(&format!("Kết nối cổng {bad_port}: {}", win.borrow().lbl_status));
    win.borrow_mut().txt_port = port.to_string();
    on_connect_click(&win); // bấm Kết nối (cổng đúng)
    wait_until(|| win.borrow().btn_lookup_enabled).await;
    log(&format!("Kết nối cổng {port}: {}", win.borrow().lbl_status));
    win.borrow_mut().txt_term = "TCP".to_string();
    let t0 = win.borrow().ticks;
    tokio::task::spawn_local(on_lookup_click(win.clone())); // bấm Tra cứu
    sleep(Duration::from_millis(600)).await;
    {
        let w = win.borrow();
        let state = if w.btn_lookup_enabled { "normal" } else { "disabled" };
        log(&format!("Đang chờ phản hồi: trạng thái '{}', nút Tra cứu {state}", w.lbl_status));
    }
    snapshot(&win, "khi đang chờ");
    wait_until(|| win.borrow().lbl_status != "Đang tra cứu...").await;
    {
        let w = win.borrow();
        log(&format!("Kết quả: {} – trạng thái '{}'", w.txt_result, w.lbl_status));
        log(&format!("Trong lúc chờ, đồng hồ giao diện được vẽ lại {} lần (không đứng hình)", w.ticks - t0));
    }
    snapshot(&win, "khi có kết quả");
    close(&win);
}

pub async fn run(host: &str, port: u16, bad_port: u16) {
    let (win, ui_rx) = new_window(host, port, false);
    let (_keys_tx, keys) = mpsc::unbounded_channel(); // kịch bản không dùng bàn phím
    tokio::task::spawn_local(scenario(win.clone(), port, bad_port));
    run_gui(win, ui_rx, keys).await;
}
