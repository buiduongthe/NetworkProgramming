# Bài 7.5 – Dịch vụ gRPC tra cứu thuật ngữ (Go)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 7.6 (Cài đặt và gọi dịch vụ gRPC – mã nghiệp vụ) |
| Mục giáo trình | 7.4.2 Protocol Buffers, 7.4.3 Bốn kiểu gọi và cơ chế truyền trên HTTP/2 |
| Kế thừa | Mã giả 3.7 (nhánh "lookup" của PROCESS_REQUEST thành phương thức Lookup) |
| Thư viện | NGOẠI LỆ thư viện: `google.golang.org/grpc` v1.68.1, `google.golang.org/protobuf` v1.35.2; công cụ `protoc`, `protoc-gen-go`, `protoc-gen-go-grpc` |

> **Trạng thái:** đã chạy trên Windows 11 (Go 1.27.3, protoc 36.2, protoc-gen-go 1.35.2, protoc-gen-go-grpc 1.5.1). Môi trường biên soạn trên Linux không tải được mô-đun Go nên bài được chạy trên máy Windows của tác giả; GitHub Actions biên dịch trên Ubuntu.

## Mục tiêu
Cài đặt phần nghiệp vụ của dịch vụ gRPC theo hợp đồng `tra_cuu.proto` (Mục 7.4.2): kiểu `GlossaryService` nhúng kiểu cơ sở do công cụ sinh, phương thức unary `Lookup` (báo `NOT_FOUND` bằng mã trạng thái gRPC) và phương thức luồng từ server `Watch` (lọc theo tiền tố, dừng khi client hủy); phía client gọi `Lookup` có hạn chót và đọc luồng `Watch`. Client in cách mã hóa Protocol Buffers của `LookupRequest{term: "TCP"}` để đối chiếu với ví dụ 5 byte `0A 03 54 43 50` của giáo trình. Bài tập “Dịch vụ gRPC có hạn chót” được minh họa bằng server cố ý chậm 2 giây và client đặt hạn chót 1 giây.

## Tệp
- `tra_cuu.proto` – hợp đồng dịch vụ (giống hệt bản của C#, Java, Python; không thêm `option go_package`, đường dẫn gói Go được truyền bằng tham số `M` của protoc).
- `glossary_server.go` – `GlossaryService` (nhúng `pb.UnimplementedGlossaryServer`): `Lookup`, `Watch`, `changesFeed`; server `grpc.NewServer`.
- `glossary_client.go` – gọi `Lookup` có hạn chót (`context.WithTimeout`), đọc luồng `Watch`.
- `main.go` – chọn vai trò `server`, `client`.
- `go.mod` – mô-đun Go riêng của bài (mô-đun gốc `go/` chỉ dùng thư viện chuẩn). `go.sum` được tạo bởi `go mod tidy`.
- `glossaryv1/tra_cuu.pb.go`, `glossaryv1/tra_cuu_grpc.pb.go` – mã sinh từ `tra_cuu.proto` (tạo ở bước chuẩn bị, không lưu trong kho, có trong `.gitignore`).
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py` trên Linux hoặc `tools/demo_windows.ps1` trên Windows).

## Cách chạy
Chuẩn bị (cần mạng, Go 1.24, `protoc` 3.x trở lên):
```bash
go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.35.2
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.5.1
export PATH="$PATH:$(go env GOPATH)/bin"
M=github.com/buiduongthe/NetworkProgramming/go/Chuong07/Bai7.5_gRPC
protoc -I. --go_out=. --go_opt=module=$M --go_opt=Mtra_cuu.proto=$M/glossaryv1 \
       --go-grpc_out=. --go-grpc_opt=module=$M --go-grpc_opt=Mtra_cuu.proto=$M/glossaryv1 tra_cuu.proto
go mod tidy
go build -o out/app .
```
Không có `protoc` riêng thì có thể dùng bản đi kèm gói Python: `python3 -m grpc_tools.protoc` với cùng tham số.
```bash
# Cửa sổ 1 – server
out/app server 0.0.0.0 5001
# Cửa sổ 2 – client (hạn chót mặc định 2000 ms)
out/app client 127.0.0.1 5001
# Bài tập hạn chót: server chậm 2 giây, client chờ 1 giây
out/app server 0.0.0.0 5002 2000
out/app client 127.0.0.1 5002 1000
```

## Kết quả thực thi
Đã chạy trên Windows 11 x64 (tháng 11/2026) bằng `tools/demo_windows.ps1`, Go 1.27.3. Bốn cửa sổ: server, server cố ý chậm 2 giây, client hạn chót 2 s và client hạn chót 1 s.

![Kết quả thực thi](ket_qua.png)

Kết quả khớp với bản Python: `LookupRequest{term: "TCP"}` mã hóa thành 5 byte `0A 03 54 43 50`; `Lookup('XYZ')` nhận `NOT_FOUND`; client hạn chót 1 s gọi server chậm 2 s nhận `DEADLINE_EXCEEDED`, còn server ghi nhận lời gọi bị hủy; luồng `Watch(prefix='HTTP')` nhận ba mục.

## Ghi chú
- Kiểu `UnimplementedGlossaryServer`, giao diện `Glossary_WatchServer`, hàm `RegisterGlossaryServer`, `NewGlossaryClient` do `protoc-gen-go-grpc` sinh (tương ứng `GlossaryBase`, `GlossaryClient` của mã giả). Go không có kế thừa lớp: `EXTENDS GlossaryBase` là nhúng (embedding) kiểu cơ sở vào struct.
- `RAISE RpcError(NOT_FOUND, …)` là `return nil, status.Error(codes.NotFound, …)`; `stream.write(t)` là `stream.Send(t)`; `ctx.cancelled` là `stream.Context().Err() != nil`; `deadline ← 2000` là `context.WithTimeout(…, 2000 ms)`. Hạn chót được gửi sang server qua tiêu đề `grpc-timeout`, server thấy `ctx.Done()` và dừng xử lý.
- Go in tên mã trạng thái dạng `NotFound`, `DeadlineExceeded`; hàm `codeName` đổi sang dạng `NOT_FOUND`, `DEADLINE_EXCEEDED` của đặc tả gRPC để thông điệp giống các ngôn ngữ khác.
- `Import` và `Sync` có trong hợp đồng nhưng không được cài đặt (gọi sẽ nhận `UNIMPLEMENTED` nhờ kiểu nhúng), đúng phạm vi của Mã giả 7.6.
- Mã giả dùng `https://glossary.local:5001` (HTTP/2 trên TLS); bài chạy HTTP/2 không mã hóa (h2c, `insecure.NewCredentials()`) trên máy cục bộ cho phòng thí nghiệm. Triển khai thật phải dùng TLS (`credentials.NewTLS`, Chương 8).
- `CHANGES_FEED()` được mô phỏng bằng danh sách cố định HTTP/2, QUIC, HTTP/3, gRPC, HTTPS, mỗi 0,5 giây một mục (goroutine gửi vào kênh); `Watch(prefix='HTTP')` nhận ba mục.
- Tham số thứ ba của server là độ trễ cố ý của `Lookup` (ms), dùng cho bài tập hạn chót; mặc định 0.
- Cổng của kịch bản minh họa: 31750 (server thường), 31751 (server chậm).
