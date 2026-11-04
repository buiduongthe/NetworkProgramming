# Bài 7.5 – Dịch vụ gRPC tra cứu thuật ngữ (C#)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 7.6 (Cài đặt và gọi dịch vụ gRPC – mã nghiệp vụ) |
| Mục giáo trình | 7.4.2 Protocol Buffers, 7.4.3 Bốn kiểu gọi và cơ chế truyền trên HTTP/2 |
| Kế thừa | Mã giả 3.7 (nhánh "lookup" của PROCESS_REQUEST thành phương thức Lookup) |
| Thư viện | NGOẠI LỆ thư viện (NuGet): `Grpc.AspNetCore` 2.67.0, `Grpc.Net.Client` 2.67.0, `Google.Protobuf` 3.29.3, `Grpc.Tools` 2.69.0; ASP.NET Core (Kestrel) |

## Mục tiêu
Cài đặt phần nghiệp vụ của dịch vụ gRPC theo hợp đồng `tra_cuu.proto` (Mục 7.4.2): lớp `GlossaryService` kế thừa lớp cơ sở do công cụ sinh, phương thức unary `Lookup` (báo `NOT_FOUND` bằng mã trạng thái gRPC) và phương thức luồng từ server `Watch` (lọc theo tiền tố, dừng khi client hủy); phía client gọi `Lookup` có hạn chót và đọc luồng `Watch`. Client in cách mã hóa Protocol Buffers của `LookupRequest{term: "TCP"}` để đối chiếu với ví dụ 5 byte `0A 03 54 43 50` của giáo trình. Bài tập “Dịch vụ gRPC có hạn chót” được minh họa bằng server cố ý chậm 2 giây và client đặt hạn chót 1 giây.

## Tệp
- `tra_cuu.proto` – hợp đồng dịch vụ (dùng chung ba ngôn ngữ); `Grpc.Tools` sinh mã khi build (`<Protobuf Include=… GrpcServices="Both" />`).
- `GlossaryService.cs` – `GlossaryService : Glossary.GlossaryBase` (`Lookup`, `Watch`) và máy chủ Kestrel chỉ bật HTTP/2.
- `GlossaryCaller.cs` – client `Glossary.GlossaryClient` trên `GrpcChannel`: `LookupAsync` có `deadline`, đọc `Watch` bằng `ReadAllAsync`.
- `Program.cs`, `Bai7.5_gRPC.csproj` – điểm vào, phân nhánh `server`/`client`; dự án dùng `Microsoft.NET.Sdk.Web`.

## Cách chạy
Cần mạng tới nuget.org ở lần build đầu (lệnh `dotnet build` tự khôi phục gói):
```bash
dotnet build -c Release
```
```bash
# Cửa sổ 1 – server
dotnet run -- server 0.0.0.0 5001
# Cửa sổ 2 – client (hạn chót mặc định 2000 ms)
dotnet run -- client 127.0.0.1 5001
# Bài tập hạn chót: server chậm 2 giây, client chờ 1 giây
dotnet run -- server 0.0.0.0 5002 2000
dotnet run -- client 127.0.0.1 5002 1000
```
Phiên bản gói có thể nâng lên bản mới hơn cùng dòng 2.x (Grpc.*) và 3.x (Google.Protobuf).


## Ghi chú
- Trạng thái kiểm thử: **chưa chạy trong môi trường biên soạn** – môi trường không truy cập được nuget.org để tải gói gRPC; chưa có ảnh kết quả. Mã viết theo cách dùng chuẩn của Grpc.AspNetCore và Grpc.Net.Client; kết quả mong đợi giống bản Python (cùng thông điệp in ra).
- Trường `term` của thông điệp `Term` trùng tên lớp nên trình sinh mã C# đặt tên thuộc tính là `Term_`.
- Kestrel được cấu hình `HttpProtocols.Http2` trên cổng không TLS (h2c); `GrpcChannel.ForAddress("http://…")` trên .NET 5 trở lên gọi được h2c mà không cần thiết lập thêm.
- `RAISE RpcError(NOT_FOUND, …)` là `throw new RpcException(new Status(StatusCode.NotFound, …))`; `ctx.cancelled` là `ctx.CancellationToken`; hạn chót truyền qua `grpc-timeout` làm `Task.Delay(…, ctx.CancellationToken)` bị hủy.
- Tệp `tra_cuu.proto` giống nhau ở ba ngôn ngữ; các dòng `option csharp_namespace`, `java_package`, `java_multiple_files` chỉ tác động tới trình sinh mã tương ứng. `Import` và `Sync` có trong hợp đồng nhưng không được cài đặt (gọi sẽ nhận `UNIMPLEMENTED`), đúng phạm vi của Mã giả 7.6.
- Mã giả dùng `https://glossary.local:5001` (HTTP/2 trên TLS); bài chạy HTTP/2 không mã hóa (h2c) trên máy cục bộ cho phòng thí nghiệm. Triển khai thật phải dùng TLS (Chương 8).
- `CHANGES_FEED()` được mô phỏng bằng danh sách cố định HTTP/2, QUIC, HTTP/3, gRPC, HTTPS, mỗi 0,5 giây một mục; `Watch(prefix='HTTP')` nhận ba mục.
- Tham số thứ ba của server là độ trễ cố ý của `Lookup` (ms), dùng cho bài tập hạn chót; mặc định 0.
- Kiểm thử: Linux, .NET SDK 10.0.114.
