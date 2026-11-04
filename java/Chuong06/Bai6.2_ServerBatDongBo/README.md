# Bài 6.2 – Server tra cứu bất đồng bộ (Java)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 6.3 (Server bất đồng bộ: mỗi kết nối một tác vụ) |
| Mục giáo trình | 6.2.3 Server bất đồng bộ |
| Kế thừa | Mã giả 6.1 (SOCKET, SETOPT, BIND, LISTEN), 5.1 (RECV_JSON_ASYNC, SEND_JSON_ASYNC), 3.7 (PROCESS_REQUEST), 2.1; client: 2.2, 3.6, 3.8 (rút gọn) |
| Thư viện | JDK 21: luồng ảo (`Thread.ofVirtual()`), `java.net.ServerSocket`, `java.lang.management.ThreadMXBean`; JSON tự viết (`Json.java`) |

## Mục tiêu
Phục vụ mỗi kết nối bằng một tác vụ bất đồng bộ thay cho một luồng: khi tác vụ chờ ở `AWAIT RECV_JSON_ASYNC`, nó không chiếm luồng nào. Mã nguồn gần như giống phiên bản đa luồng, chỉ khác ở các lời gọi bất đồng bộ.

## Tệp
- `AsyncServer.java` – Mã giả 6.3: `main` (ASYNC_SERVER), `serveAsync` (SERVE_ASYNC) trên luồng ảo.
- `LookupService.java` – Mã giả 3.7: `processRequest`.
- `NetUtil.java` – Mã giả 2.1, 2.2, 3.4, 3.6.
- `Json.java` – JSON_ENCODE, JSON_DECODE tối giản.
- `MultiClient.java` – chương trình mở n client tra cứu đồng thời.
- `glossary.json` – từ điển thuật ngữ.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
Mở các cửa sổ dòng lệnh trong thư mục bài:
```bash
javac -encoding UTF-8 -d out *.java
# Cửa sổ 1 – server (mặc định 0.0.0.0 9000)
java -cp out AsyncServer 0.0.0.0 9000
# Cửa sổ 2 – 5 client đồng thời, mỗi client giữ kết nối 1000 ms
java -cp out MultiClient 127.0.0.1 9000 5 1000
```
Dừng server bằng Ctrl+C.

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Năm client khởi động cách nhau 100 ms đều được phục vụ ngay (t ≈ 0, 100, …, 400 ms) dù các client trước vẫn giữ kết nối. Số tác vụ tăng theo số client trong khi số luồng của tiến trình không đổi, minh họa việc tác vụ đang chờ không chiếm luồng.

## Ghi chú
- `SERVE_ASYNC` không có thao tác `bye` (khác `HANDLE_LOOKUP` của Mã giả 3.7), nên client chỉ đóng kết nối; server nhận `ConnectionClosed` với 0 byte và ghi "client đóng kết nối".
- Mã giả không liệt kê `JsonError` trong khối `CATCH`; bản cài đặt coi JSON sai như `ProtocolError` và kết thúc phiên (Mã giả 3.7 thì trả 400 và tiếp tục).
- Con số "luồng" in ra có ý nghĩa khác nhau giữa ba nền tảng (xem mục riêng của ngôn ngữ); điểm cần quan sát là con số này không tăng theo số client.
- Theo Mục 5.2.3 và bảng ánh xạ bất đồng bộ, Java cài đặt bằng luồng ảo (JDK 21): mỗi tác vụ là một luồng ảo viết mã chặn; khi chờ vào/ra, luồng ảo được tháo khỏi luồng hệ điều hành. `WITH_TIMEOUT` là `setSoTimeout(300000)`. "luồng" là số luồng nền tảng của JVM (`ThreadMXBean`), không tính luồng ảo.
- Kiểm thử: Linux, OpenJDK 21.0.12.1.
