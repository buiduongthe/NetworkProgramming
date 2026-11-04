# Bài 5.3 – Ứng dụng tra cứu có giao diện bất đồng bộ (C#)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 5.3 (Cách (1): luồng nền và UI_INVOKE), Mã giả 5.4 (Cách (2): trình xử lý sự kiện bất đồng bộ) |
| Mục giáo trình | 5.3.1 Mô hình luồng giao diện đơn, 5.3.2 Ba cách tiếp cận, 5.3.5 Tranh chấp và quản lý trạng thái |
| Kế thừa | Mã giả 2.2 (SAFE_CONNECT), 3.7 (server tra cứu), 3.8 (một lượt tra cứu), 5.1 (khung JSON bất đồng bộ) |
| Thư viện | BCL và Windows Forms (`net10.0-windows`, `UseWindowsForms`): `System.Windows.Forms`, `System.Net.Sockets`, `System.Text.Json.Nodes` |

## Mục tiêu
Kết hợp socket với giao diện đồ họa theo hai cách tiếp cận của Mục 5.3.2. Nút Kết nối dùng cách (1): luồng nền gọi `SAFE_CONNECT` chặn rồi chuyển kết quả về luồng giao diện bằng `UI_INVOKE` (Mã giả 5.3). Nút Tra cứu dùng cách (2): trình xử lý sự kiện bất đồng bộ gửi yêu cầu và chờ phản hồi mà không chặn luồng giao diện (Mã giả 5.4). Một đồng hồ trên cửa sổ cập nhật mỗi 100 ms để kiểm chứng giao diện không đứng hình khi server trả lời chậm.

## Tệp
- `LookupForm.cs` – cửa sổ WinForms: `OnConnectClick`, `ConnectWorker` (cách 1, `Control.BeginInvoke`), `OnLookupClick` (cách 2, `async void`), `SafeConnect` bản chặn.
- `KhungJson.cs` – khung JSON bất đồng bộ (Mã giả 5.1), `ParseEndpoint`, `SafeConnectAsync`.
- `LookupServer.cs` – server tra cứu tối giản có độ trễ giả lập.
- `Program.cs`, `Bai5.3_GiaoDienBatDongBo.csproj` – điểm vào `[STAThread]`, phân nhánh `server`/`gui`.

## Cách chạy
Trên Windows (cần .NET 10 SDK), trong thư mục bài:
```powershell
dotnet build -c Release
# Cửa sổ 1 – server, trả lời chậm 1500 ms
dotnet run -- server 0.0.0.0 9100 1500
# Cửa sổ 2 – ứng dụng giao diện
dotnet run -- gui 127.0.0.1 9100
```
Bấm Kết nối, nhập thuật ngữ (ví dụ `TCP`) rồi bấm Tra cứu; trong lúc chờ, kéo cửa sổ hoặc quan sát đồng hồ để kiểm chứng giao diện không đứng hình.

## Kết quả thực thi
Trạng thái kiểm thử: chưa chạy trong môi trường biên soạn (cần Windows). Dự án WinForms (`net10.0-windows`) không biên dịch được trên Linux khi không có gói tham chiếu Windows Desktop; vì vậy bài không có ảnh kết quả. Hai tệp `KhungJson.cs`, `LookupServer.cs` đã được biên dịch thử trong một dự án `net10.0` tạm và server chạy đúng với client Python của Bài 5.1.

## Ghi chú
- Cách (2) là cách được khuyến nghị trên .NET: sau `await`, `SynchronizationContext` của WinForms đưa phần tiếp theo về luồng giao diện, nên mã gán trực tiếp `txtResult.Text` mà không cần `UI_INVOKE` (Mục 5.3.2). Trình xử lý dùng `async void` vì đây là chữ ký của sự kiện; mọi ngoại lệ được bắt bên trong (Mục 5.4.5).
- `UI_INVOKE` dùng dạng không đồng bộ `BeginInvoke`; dạng đồng bộ `Invoke` bắt luồng nền chờ luồng giao diện xử lý xong.
- Trong mã giả 5.4, `conn` được mở khi nạp cửa sổ; bài mở kết nối bằng nút Kết nối (cách 1): `CONNECT_WORKER` kết nối trên luồng nền rồi dùng `UI_INVOKE(SET_FIELD, conn, c)` để lưu socket vào trường của cửa sổ trên luồng giao diện trước khi báo "Đã kết nối" (dòng 7–9 của Mã giả 5.3). Theo dòng 8–12 của Mã giả 5.4, phản hồi khác 200 (ví dụ 404 khi thuật ngữ không có trong từ điển) được hiển thị dạng "(mã) lý do", ví dụ `(404) Không có thuật ngữ`.
- Đóng cửa sổ (`FormClosing`) hủy `CancellationTokenSource` của cửa sổ và đóng kết nối (Mục 5.3.5).
- Kiểm thử: chưa chạy – cần Windows (.NET SDK 10.0.114 trên Linux chỉ dùng để biên dịch thử phần không phụ thuộc WinForms).
