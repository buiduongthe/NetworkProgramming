# Bài 8.2 – Thùng token giới hạn tần suất (Java)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 8.3 (Thùng token giới hạn tần suất) |
| Mục giáo trình | 8.2.4 Tự giới hạn tần suất bằng thùng token |
| Kế thừa | Mã giả 5.8 (RETRY_ASYNC) – client tự giới hạn thay vì chờ lỗi 429 |
| Thư viện | JDK: `ReentrantLock`, luồng ảo (`Thread.ofVirtual`) |

## Mục tiêu
Cài đặt thuật toán thùng token: thùng dung lượng C được nạp lại R đơn vị mỗi giây, số đơn vị được tính bù một lần mỗi khi có yêu cầu (không cần bộ đếm giờ); yêu cầu thiếu đơn vị phải chờ ngoài vùng khóa rồi tính lại. Nhiều tác vụ đồng thời gửi chuỗi yêu cầu qua cùng một thùng; chương trình in thời điểm mỗi yêu cầu được phép và so với lý thuyết: C yêu cầu đầu đi ngay, yêu cầu thứ m > C được phép sau (m − C)/R giây.

## Tệp
- `Bucket.java` – lớp Bucket với acquireTokens (Mã giả 8.3).
- `ThungToken.java` – chương trình chính.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
```bash
javac -encoding UTF-8 -d out Bucket.java ThungToken.java
java -cp out ThungToken 10 2 14 2
```
Tham số: `C R số_yêu_cầu số_tác_vụ` (mặc định `10 2 14 2`, đúng ví dụ C = 10, R = 2 của Mục 8.2.4).

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Với C = 10, R = 2: mười yêu cầu đầu được phép ngay tại t ≈ 0, các yêu cầu 11–14 được phép tại khoảng 0,5; 1,0; 1,5; 2,0 giây, khớp giá trị lý thuyết. Yêu cầu n = 11 > C bị từ chối ngay bằng ArgumentError thay vì chờ mãi.

## Ghi chú
- Thời điểm được phép được ghi lại rồi in theo thứ tự thời gian sau khi mọi tác vụ kết thúc, để thứ tự in không phụ thuộc vào cách lập lịch luồng; cột "tác vụ" cho thấy hai tác vụ cùng dùng một thùng.
- ArgumentError của mã giả tương ứng `ValueError` (Python), `ArgumentException` (C#), `IllegalArgumentException` (Java).
- Java dùng luồng ảo với mã chặn (`ReentrantLock`, `Thread.sleep`) thay cho ASYNC_LOCK và DELAY_ASYNC, theo hướng tiếp cận của Mục 5.2.3.
- Kiểm thử: Linux, OpenJDK 21.0.12.1.
