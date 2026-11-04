# Bài 5.4 – Lớp dịch vụ ChatClient bất đồng bộ (C#)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 5.5 (Lớp dịch vụ ChatClient bất đồng bộ) |
| Mục giáo trình | 5.3.2 Ba cách tiếp cận (cách 3), 5.3.3 Ví dụ: lớp dịch vụ chat client, 5.4.5 Xử lý ngoại lệ trong tác vụ bất đồng bộ |
| Kế thừa | Mã giả 5.1 (khung JSON bất đồng bộ); SAFE_CONNECT_ASYNC = Mã giả 2.2 + AWAIT; giao thức chat của Mục 6.3.1 (phần `hello`, `msg`, `sys`, `welcome`) |
| Thư viện | BCL: `System.Net.Sockets`, `System.Text.Json.Nodes`, `SemaphoreSlim`, `CancellationTokenSource`, `System.Threading.Channels` |

## Mục tiêu
Cài đặt lớp `ChatClient` theo cách tiếp cận (3) của Mục 5.3.2: lớp dịch vụ mạng giữ kết nối dài hạn, có vòng nhận chạy nền phát sự kiện `on_message`, thủ tục gửi tuần tự hóa bằng khóa `send_lock`, và thủ tục ngắt kết nối hủy vòng nhận trước khi đóng socket. Tầng giao diện (ở đây là giao diện dòng lệnh) chỉ đăng ký hai sự kiện và gọi các thủ tục của lớp, nên lớp kiểm thử được mà không cần cửa sổ.

## Tệp
- `ChatClient.cs` – lớp `ChatClient`: `ConnectAsync`, `ReceiveLoopAsync`, `SendMessageAsync`, `Disconnect`, sự kiện `OnMessage`, `OnStatus`.
- `ChatCli.cs` – giao diện dòng lệnh dùng `ChatClient`.
- `ChatServer.cs` – server chat tối giản đi kèm.
- `KhungJson.cs` – khung JSON bất đồng bộ (Mã giả 5.1), `SafeConnectAsync`.
- `Program.cs`, `Bai5.4_ChatClient.csproj` – điểm vào, phân nhánh `server`/`cli`.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
```bash
dotnet build -c Release
```
```bash
# Cửa sổ 1 – server chat
dotnet run -- server 0.0.0.0 9200
# Cửa sổ 2, 3 – hai người dùng
dotnet run -- cli Bình 127.0.0.1 9200
dotnet run -- cli An 127.0.0.1 9200
```
Mỗi dòng nhập được gửi thành một tin; `/quit` để ngắt kết nối; `/cho <giây>` tạm dừng đọc bàn phím (dùng trong kịch bản minh họa).

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Bình kết nối trước, An kết nối sau; tin của mỗi người được server chuyển tới cả hai (gồm cả người gửi). Khi An gõ `/quit`, `DISCONNECT` hủy vòng nhận trước rồi mới đóng socket, nên phía An kết thúc lặng lẽ (nhánh `Cancelled`), còn Bình nhận thông báo hệ thống "An đã rời phòng". Cuối kịch bản, server bị dừng trước: vòng nhận của Bình gặp kết nối đóng và báo `on_status("lost: ...")` (nhánh lỗi ngoài ý muốn), sau đó Bình được dừng bằng Ctrl+C. Ở cửa sổ của An, dòng "Trạng thái: connected" xuất hiện sau hai thông điệp đầu: `_ = ReceiveLoopAsync()` chạy đồng bộ cho tới điểm `await` đầu tiên chưa hoàn tất, và vì `welcome`, `sys` đã có sẵn trong bộ đệm nhận nên sự kiện `OnMessage` được phát trước khi `ConnectAsync` kịp báo `OnStatus`.

## Ghi chú
- `send_lock` là `SemaphoreSlim(1, 1)`; `cts` là `CancellationTokenSource`; `RUN_BACKGROUND` là `_ = ReceiveLoopAsync()` (không `await`).
- Sau `Disconnect`, lời gọi đọc có thể kết thúc bằng `ObjectDisposedException` hoặc `IOException` thay vì `OperationCanceledException` vì socket bị đóng ngay sau khi hủy; vòng nhận dùng bộ lọc `when (cts.IsCancellationRequested)` để vẫn đi vào nhánh `Cancelled` như mã giả.
- Vòng nhận bắt `NetError`, `ConnectionClosed`, `ProtocolError` và `JsonException` (tương ứng `JsonError`) như dòng 23 của Mã giả 5.5, cùng `IOException` của luồng mạng. `SendMessageAsync(room, text)` gửi `{"type":"msg","room":…,"text":…}` theo dòng 28–31; giao diện dòng lệnh luôn chọn phòng `chung`, và server tối giản phát lại tin với đúng trường `room` nhận được.
- Thứ tự sự kiện `OnStatus("connected")` và `OnMessage` không được bảo đảm (xem Kết quả thực thi); giao diện không nên giả định thứ tự này.
- Bàn phím được đọc trên một luồng nền và chuyển qua `Channel<string?>`; Ctrl+C được chặn để thoát gọn qua `Disconnect`.
- Server chat đi kèm là bản tối giản viết bằng cùng ngôn ngữ: một phòng chung, chuyển tiếp `msg`, trả lời `ping`, lỗi 409 khi trùng tên. Server dùng một khóa gửi cho mỗi client thay cho hộp thư đi và tác vụ ghi riêng của Chương 6. Bản đầy đủ (nhiều phòng, tin riêng) thuộc Chương 6.
- Kiểm thử: Linux, .NET SDK 10.0.114.
