# Bài 5.6 – Hạn chót, hủy và thử lại có lùi thời gian (C#)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 5.7 (Yêu cầu có hạn chót và hủy), Mã giả 5.8 (Thử lại có lùi theo cấp số nhân và nhiễu ngẫu nhiên) |
| Mục giáo trình | 5.4.1 Hủy tác vụ hợp tác, 5.4.2 Thời gian chờ và hạn chót, 5.4.3 Thử lại có lùi thời gian và nhiễu ngẫu nhiên |
| Kế thừa | Mã giả 5.1 (khung JSON bất đồng bộ), 2.2 (SAFE_CONNECT, `NetError` có trường `retry`), 4.4 (UDP_REQUEST – vòng gửi lại có lùi thời gian) |
| Thư viện | BCL: `CancellationTokenSource` (`CancelAfter`, `CreateLinkedTokenSource`), `Task.Delay`, `Random.Shared`, `System.Text.Json.Nodes` |

## Mục tiêu
Cài đặt `REQUEST_WITH_DEADLINE`, kết hợp tín hiệu hủy của người dùng với tín hiệu tự hủy khi hết hạn và phân biệt hai nguyên nhân, cùng `RETRY_ASYNC` thử lại có lùi theo cấp số nhân với nhiễu ngẫu nhiên, tôn trọng thời gian chờ do server đề nghị. Server đi kèm cố ý trả lời chậm ở yêu cầu thứ nhất và trả lỗi 503 ở yêu cầu thứ hai.

## Tệp
- `DeadlineRetry.cs` – `RequestWithDeadlineAsync` (Mã giả 5.7), `RetryAsync` (Mã giả 5.8), hàm tạo `op()` và ba kịch bản.
- `FlakyServer.cs` – server tra cứu cố ý trả lời chậm, trả 503.
- `KhungJson.cs` – khung JSON bất đồng bộ (Mã giả 5.1), `SafeConnectAsync`, `NetError` có `RetryAfter`.
- `Program.cs`, `Bai5.6_HanChotThuLai.csproj` – điểm vào, phân nhánh `server`/`client`.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
```bash
dotnet build -c Release
```
```bash
# Cửa sổ 1 – server "khó tính"
dotnet run -- server 0.0.0.0 9100
# Cửa sổ 2 – ba kịch bản; cổng thứ hai là cổng không có server
dotnet run -- client 127.0.0.1 9100 9101
```
Server đếm yêu cầu từ lúc khởi động; cần khởi động lại server trước mỗi lần chạy client để lặp lại đúng kịch bản.

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Kịch bản 1: lần thử 1 hết hạn chót 1000 ms (lỗi `TIMEOUT`, có thể thử lại), chờ một khoảng ngẫu nhiên không quá 200 ms; lần thử 2 nhận 503 kèm đề nghị chờ 300 ms nên thời gian chờ được nâng lên ít nhất 300 ms (giới hạn trên của khoảng ngẫu nhiên lúc này là 400 ms); lần thử 3 thành công. Kịch bản 2: người dùng bấm Hủy sau 500 ms trong khi hạn chót là 5000 ms; kết quả là `Cancelled`, không phải `Timeout`, và không có lần thử lại. Kịch bản 3: kết nối bị từ chối (`REFUSED`, `retry = FALSE`) nên lỗi được báo ngay sau lần thử đầu. Thời gian chờ ngẫu nhiên khác nhau giữa các lần chạy.

## Ghi chú
- `CANCEL_AFTER` là `new CancellationTokenSource(timeoutMs)`, `LINK` là `CancellationTokenSource.CreateLinkedTokenSource(userCt, deadline.Token)`; sau `OperationCanceledException`, `userCt.IsCancellationRequested` cho biết nguyên nhân.
- `op(ct)` gồm kết nối mới và một lần `REQUEST_WITH_DEADLINE`. Theo dòng 5–6 của Mã giả 5.8, `RetryAsync` bắt cả `TimeoutException` (tương ứng `Timeout`, ném từ Mã giả 5.7 khi quá hạn chót) và `NetError`; `Timeout` luôn được thử lại, `NetError` chỉ khi `Retry` = true. Phản hồi 503 được `op` đổi thành `NetError("UNAVAILABLE", retry ← TRUE, retry_after ← retry_after_ms)`. Sau khi hết hạn, kết nối đã bị đóng nên mỗi lần thử phải mở kết nối mới.
- `RetryAsync(op, maxTries, baseMs, capMs, ct)` truyền `ct` cho `op(ct)` và `Task.Delay(delay, ct)` như dòng 4 và 12: nút Hủy dừng được cả lần thử đang chạy lẫn khoảng chờ lùi; `OperationCanceledException` không bị bắt nên không có lần thử lại.
- Kiểm thử: Linux, .NET SDK 10.0.114.
