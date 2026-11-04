// Bài 7.5 – mô-đun riêng vì cần thư viện ngoài (gRPC, Protocol Buffers);
// mô-đun gốc go/ chỉ dùng thư viện chuẩn. Chạy "go mod tidy" (cần mạng) để tạo go.sum.
module github.com/buiduongthe/NetworkProgramming/go/Chuong07/Bai7.5_gRPC

go 1.24

require (
	google.golang.org/grpc v1.68.1
	google.golang.org/protobuf v1.35.2
)
