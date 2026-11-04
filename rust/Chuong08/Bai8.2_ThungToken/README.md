# Bài 8.2 – Thùng token giới hạn tần suất (Rust)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 8.3 (Thùng token giới hạn tần suất) |
| Mục giáo trình | 8.2.4 Tự giới hạn tần suất bằng thùng token |
| Kế thừa | Mã giả 5.8 (RETRY_ASYNC) – client tự giới hạn thay vì chờ lỗi 429 |
| Thư viện | crate `tokio` (`sync::Mutex`, `time::sleep`, `spawn`) |

## Mục tiêu
Cài đặt thuật toán thùng token: thùng dung lượng C được nạp lại R đơn vị mỗi giây, số đơn vị được tính bù một lần mỗi khi có yêu cầu (không cần bộ đếm giờ); yêu cầu thiếu đơn vị phải chờ ngoài vùng khóa rồi tính lại. Nhiều tác vụ đồng thời gửi chuỗi yêu cầu qua cùng một thùng; chương trình in thời điểm mỗi yêu cầu được phép và so với lý thuyết: C yêu cầu đầu đi ngay, yêu cầu thứ m > C được phép sau (m − C)/R giây.

## Tệp
- `src/main.rs` – `Bucket`, `acquire_tokens` (ACQUIRE_TOKENS) và chương trình chính.
- `Cargo.toml`, `Cargo.lock` – gói Cargo `bai8_2_thung_token`.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
```bash
cargo run --release -- 10 2 14 2
```
Tham số: `C R số_yêu_cầu số_tác_vụ` (mặc định `10 2 14 2`, đúng ví dụ C = 10, R = 2 của Mục 8.2.4).

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Với C = 10, R = 2: mười yêu cầu đầu được phép ngay tại t ≈ 0, các yêu cầu 11–14 được phép tại khoảng 0,5; 1,0; 1,5; 2,0 giây, khớp giá trị lý thuyết. Yêu cầu n = 11 > C bị từ chối ngay bằng ArgumentError thay vì chờ mãi.

## Ghi chú
- Thời điểm được phép được ghi lại rồi in theo thứ tự thời gian sau khi mọi tác vụ kết thúc, để thứ tự in không phụ thuộc vào cách lập lịch luồng; cột "tác vụ" cho thấy hai tác vụ cùng dùng một thùng.
- ArgumentError của mã giả tương ứng giá trị `Err(String)` trả về từ `acquire_tokens` (Rust không có ngoại lệ).
- `ASYNC_LOCK()` là `tokio::sync::Mutex`: khóa bất đồng bộ, chờ khóa bằng `.await` mà không chặn luồng. Trạng thái `tokens`, `last` nằm bên trong khóa nên không thể truy cập khi chưa giữ khóa. `DELAY_ASYNC` là `tokio::time::sleep`, gọi sau khi đã nhả khóa (ra khỏi khối `{ … }`). Môi trường chạy tokio đa luồng nên các tác vụ có thể chạy song song thật; khóa vì vậy càng cần thiết.
- Danh sách thời điểm được phép dùng `std::sync::Mutex` (chỉ giữ trong lúc thêm một phần tử, không có `.await` bên trong); `asyncio.sleep(0)` của bản Python là `tokio::task::yield_now()`.
- Kiểm thử: Linux, Rust 1.97.0, tokio 1.54.1.
