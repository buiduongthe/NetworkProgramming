# Bài mở rộng MR5.3 – Giao diện luôn phản hồi khi nhiều tác vụ mạng chạy song song (C#, WinForms)

| Thông tin | Nội dung |
|---|---|
| Loại | **Bài mở rộng** – không có trong giáo trình, không có mã giả tương ứng |
| Liên hệ với giáo trình | Chương 5: Bài 5.3 (giao diện bất đồng bộ), Bài 5.5 (tải tệp có tiến trình và hủy), Bài 5.2 (giới hạn đồng thời); Bài 1.2 (kiểm tra cổng), Bài 4.3 (tra cứu tên qua UDP); MR5.2 (các mô hình đồng thời) |
| Thư viện | BCL và Windows Forms (`net10.0-windows`, `UseWindowsForms`): `System.Windows.Forms`, `HttpClient`, `System.Net.Sockets`, `Progress<T>`, `CancellationTokenSource` |
| Bản ngôn ngữ khác | [Python](../../../python/Chuong05/BaiMR5.3_GiaoDienDaTacVu/) (Tkinter), [Go](../../../go/Chuong05/BaiMR5.3_GiaoDienDaTacVu/) (giao diện web cục bộ), [Java](../../../java/Chuong05/BaiMR5.3_GiaoDienDaTacVu/), [Rust](../../../rust/Chuong05/BaiMR5.3_GiaoDienDaTacVu/) |

## Mục tiêu
Một ứng dụng desktop chạy **nhiều tác vụ mạng cùng lúc** – tải tệp HTTP từ máy chủ thử với độ trễ khác nhau, kiểm tra cổng TCP, tra cứu tên qua máy chủ “DNS” giả lập trên UDP – mà **giao diện không bao giờ đứng**: đồng hồ (có mili giây) và hoạt hình chạy đều, mỗi tác vụ có thanh tiến trình và nút Hủy riêng, có nút Hủy tất cả và thanh trượt giới hạn số tác vụ chạy đồng thời. Nút **“Làm SAI”** chạy đúng việc tải tệp đó bằng lời gọi chặn ngay trên luồng giao diện để thấy cả cửa sổ đứng bao lâu. Ứng dụng tự đo **khoảng cách thật giữa hai nhịp đồng hồ** (đặt 20 ms): luồng giao diện còn rảnh thì khoảng cách gần mức đặt, bị chặn thì tăng vọt. Chức năng và kịch bản minh họa giống bản Python.

## Kiến thức chính
- **Hai quy tắc của lập trình giao diện:** (1) chỉ luồng giao diện được chạm vào điều khiển; (2) luồng giao diện không bao giờ chờ mạng hay tính toán lâu. WinForms (như Tk, WPF, Android, trình duyệt) theo mô hình một luồng giao diện với vòng lặp thông điệp (`Application.Run`).
- **`async`/`await` trên luồng giao diện (cách 2 của Mục 5.3.2):** khác bản Python (luồng nền + `queue.Queue`), phần điều phối mỗi tác vụ (`RunTask`: xin lượt, cập nhật hàng, báo kết quả) là một hàm `async` chạy trên chính luồng giao diện; `SynchronizationContext` của WinForms đưa phần tiếp theo sau mỗi `await` về luồng giao diện, nên mã gán trực tiếp vào điều khiển mà không cần `BeginInvoke` hay hàng đợi.
- **Phần mạng chạy bằng `await Task.Run(...)`:** đoạn *đồng bộ* đầu tiên của một hàm `async` (trước `await` chưa xong đầu tiên) chạy trên luồng gọi nó. Lần gọi `HttpClient` đầu tiên phải khởi tạo và biên dịch JIT cả chồng xử lý HTTP nên mất hàng trăm mili giây; khi phần này nằm trên luồng giao diện, lần chạy đầu trên Windows 11 đo được nhịp đồng hồ trễ tối đa **733 ms** ngay lúc bấm Tải tệp – giao diện khựng dù mã toàn `await`. Đưa phần mạng sang thread pool bằng `Task.Run` loại bỏ khoảng khựng này; tiến trình vẫn về giao diện qua `Progress<T>`.
- **Báo tiến trình bằng `Progress<T>`:** đối tượng tạo trên luồng giao diện tự đưa mỗi lần `Report` về luồng giao diện – phần tác vụ (`TacVu.cs`) không biết gì về WinForms, biên dịch và kiểm thử được độc lập.
- **Giới hạn đồng thời đổi được khi đang chạy:** `Limiter` giữ danh sách `TaskCompletionSource` của các tác vụ đang chờ; có chỗ trống, đổi giới hạn hay bấm Hủy thì các tác vụ chờ được đánh thức. Vì mọi lời gọi đều trên luồng giao diện nên không cần khóa.
- **Hủy hợp tác:** mỗi tác vụ có một `CancellationTokenSource`; token được truyền xuống mọi lời gọi mạng nên hủy có hiệu lực ngay cả khi đang chờ dữ liệu (không phải đợi hết một khối như bản Python). Tác vụ bị hủy khi còn đang chờ lượt thì không bao giờ mở kết nối.
- **Đo độ “mượt”:** `System.Windows.Forms.Timer` 20 ms ghi khoảng cách thật giữa hai lần chạy, theo giai đoạn “rỗi”, “chạy song song”, “luồng GUI bị chặn”.

## Cấu trúc tệp
- `BaiMR5.3_GiaoDienDaTacVu.csproj` – dự án `net10.0-windows`, `UseWindowsForms`.
- `Program.cs` – điểm vào `[STAThread]`, phân nhánh `server`/`gui`/`demo`.
- `UngDung.cs` – cửa sổ WinForms: dựng giao diện, nhịp đồng hồ và thống kê, tạo/hủy tác vụ (`Spawn`, `RunTask`), nút “Làm SAI”; lớp `TaskRow` (một hàng tác vụ), `HoatHinh` (quả bóng, cung tròn vẽ bằng GDI+).
- `TacVu.cs` – phần không dùng WinForms: `Limiter`, tải tệp HTTP có tiến trình (`DownloadAsync`) và bản chặn (`DownloadBlocking`, chỉ cho nút “Làm SAI”), kiểm tra cổng, tra cứu tên qua UDP.
- `MayChuThu.cs` – máy chủ thử cục bộ (vai trò `server`): HTTP `GET /tep?kb=..&tre=..` viết trên `TcpListener` (gửi từng khối 16 KB, nghỉ `tre` ms sau mỗi khối), “DNS” giả lập trên UDP (100–400 ms, có một tên không tồn tại), hai cổng TCP mở.
- `DemoGui.cs` – kịch bản minh họa tự động (vai trò `demo`): cùng các bước với `--demo` của bản Python, chụp cửa sổ bằng `Control.DrawToBitmap`, ghi `ket_qua_gui.png`.
- `demo.json` – kịch bản chạy minh họa dạng Windows (dùng với `tools/demo_windows.ps1`).

## Cách chạy
Trên Windows (cần .NET 10 SDK), trong thư mục bài:
```powershell
dotnet build -c Release
# Cửa sổ 1 – máy chủ thử (dùng cổng 9530–9532; cổng 9533 để trống)
dotnet run -- server 127.0.0.1 9530
# Cửa sổ 2 – ứng dụng giao diện
dotnet run -- gui 127.0.0.1 9530
```
Thử: bấm **Tải tệp** (5 tệp), **Kiểm tra cổng**, **Tra cứu tên**; kéo thanh trượt giới hạn đồng thời; bấm **Hủy** ở một tác vụ; rồi bấm **Làm SAI** và quan sát đồng hồ, quả bóng đứng yên khoảng 1,2 s.

Chạy kịch bản tự động và chụp ảnh (từ thư mục gốc kho mã):
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\demo_windows.ps1 csharp\Chuong05\BaiMR5.3_GiaoDienDaTacVu
```
Script biên dịch bài, chạy máy chủ thử ở cổng 42530, chạy `demo 127.0.0.1 42530`, ghi nhật ký `.log_*.txt` và ảnh `ket_qua_gui.png`; ảnh `ket_qua.png` dựng từ nhật ký bằng `tools/render_output.py`. Vai trò `server` chỉ dùng BCL nên cũng chạy được trên Linux/macOS (`dotnet run -- server`), nhưng cửa sổ WinForms thì chỉ chạy trên Windows.

## Kết quả thực thi
**Chưa chạy – cần Windows.** Môi trường biên soạn (Linux) không có gói tham chiếu Windows Desktop nên không biên dịch được dự án `net10.0-windows`; vì vậy thư mục chưa có `ket_qua.txt`, `ket_qua.png`, `ket_qua_gui.png` (không dựng kết quả giả). Phần không phụ thuộc WinForms (`MayChuThu.cs`, `TacVu.cs`) đã được biên dịch và chạy thử trên Linux bằng một dự án console tạm: tải 64 KB có tiến trình, kiểm tra 4 cổng (3 mở, 1 đóng), tra cứu 2 tên (1 có địa chỉ, 1 NXDOMAIN), tải chặn, hủy giữa chừng (máy chủ ghi nhận client đóng kết nối), `Limiter` trả `false` khi bị hủy lúc chờ lượt. Mã WinForms đã được kiểm tra biên dịch với các kiểu thay thế tối giản (stub) của `System.Windows.Forms`/`System.Drawing`.

Kết quả mong đợi khi chạy `tools/demo_windows.ps1` (theo bản Python): 7 tác vụ, tác vụ #2 bị hủy khi đang tải; ảnh 1 có 5 tác vụ chưa xong, 2 “chờ lượt”; kiểm tra cổng 3 mở, 1 đóng; tra cứu 4/5 tên có địa chỉ; “Làm SAI” chặn luồng giao diện ≈ 1,2 s trong khi các tệp tải nền vẫn chạy (bản C# chúng chạy bằng I/O bất đồng bộ, phần tiếp theo chỉ được xử lý khi luồng giao diện rảnh trở lại); Hủy tất cả dừng ngay mọi tác vụ.

## Ghi chú
- Không chạm điều khiển WinForms từ luồng khác: phần chạy trên thread pool (`TacVu.cs`) chỉ báo qua `IProgress<T>`, mọi thao tác trên điều khiển nằm trong `RunTask`/`Progress<T>` trên luồng giao diện. I/O bất đồng bộ không giữ luồng nào khi đang chờ dữ liệu, nên `Task.Run` không tốn một luồng cho mỗi tác vụ.
- `HttpClient` được tạo một lần và dùng chung (khuyến nghị của .NET); thời gian chờ đặt `InfiniteTimeSpan` vì hạn chót và hủy do `CancellationToken` của từng tác vụ quyết định.
- Nút “Làm SAI” dùng `HttpClient.Send` – phiên bản **đồng bộ** – và `Stream.Read`; không dùng `.Result`/`.Wait()` trên tác vụ async vì trên luồng giao diện có thể gây bế tắc (phần tiếp theo cần chính luồng đang bị chặn).
- Bộ hẹn giờ WinForms dựa trên `WM_TIMER` nên độ phân giải phụ thuộc Windows (thường ≈ 15,6 ms): đặt 20 ms thì khoảng cách đo được thường ≈ 31 ms ngay cả khi rỗi; điều cần quan sát là chênh lệch giữa “chạy song song” và “luồng GUI bị chặn”.
- Máy chủ thử viết HTTP trực tiếp trên `TcpListener` thay vì `HttpListener` (trên Windows `HttpListener` dùng http.sys, có thể đòi quyền đăng ký URL).
- Thời gian chụp ảnh trong kịch bản minh họa (`DrawToBitmap` chạy đồng bộ trên luồng giao diện) không được tính vào thống kê nhịp.
- Kiểm thử: phần không phụ thuộc WinForms – Linux, .NET SDK 10.0.114; phần giao diện – chưa chạy (cần Windows).

## Bài tập gợi ý
1. Đổi `TickMs` thành 5 ms và 100 ms; khoảng cách trung bình đo được có đúng như đặt không? Vì sao bộ hẹn giờ WinForms không đảm bảo chính xác?
2. Cho thanh trượt giới hạn đồng thời = 1 rồi bấm Tải 5 tệp; trong lúc chạy kéo lên 8. Quan sát các tác vụ “chờ lượt” được khởi động cùng lúc.
3. Thêm cột “tốc độ” (KB/s) và “thời gian còn lại” cho mỗi tác vụ tải, tính trong `DownloadAsync` và gửi qua `IProgress`.
4. Viết lại theo cách 1 của Mục 5.3.2: mỗi tác vụ chạy trên một luồng nền với API chặn và báo về giao diện bằng `BeginInvoke`; so sánh độ dài mã và độ mượt.
5. (Nâng cao) Bỏ `Task.Run` trong `RunTask` (gọi thẳng `await job(...)`), chạy lại kịch bản và so sánh nhịp tối đa ở giai đoạn “chạy song song”, nhất là ngay lần bấm đầu. Sau đó chạy việc nặng CPU (MR5.2) ngay sau `await` trong `RunTask`; giải thích kết quả bằng `SynchronizationContext` và đoạn đồng bộ của hàm `async`.

## Đọc thêm
- Tài liệu .NET: *Threading in Windows Forms* (`Control.Invoke`, `BeginInvoke`), *Task-based asynchronous pattern (TAP)* – mục *Progress reporting* và *Cancellation*, `IHttpClientFactory` và hướng dẫn dùng `HttpClient` (learn.microsoft.com).
- RFC 9110 – *HTTP Semantics*, 2022 (§8.6 Content-Length).
- Claus Töpke, *Network Programming and Automation Essentials*, Packt, 2023, Chương 8 *Scaling Your Code*.

Mã nguồn của bài được viết mới cho kho này; các tài liệu trên chỉ là nguồn tham khảo kiến thức.
