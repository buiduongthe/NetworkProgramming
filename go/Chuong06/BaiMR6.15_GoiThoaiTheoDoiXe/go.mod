// Bài mở rộng MR6.15 – mô-đun riêng vì cần thư viện ngoài (Pion WebRTC, gorilla/websocket); mô-đun gốc go/ chỉ
// dùng thư viện chuẩn. Chạy "go mod tidy" (cần mạng) để tạo go.sum trước lần biên dịch đầu tiên.
module github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.15_GoiThoaiTheoDoiXe

go 1.24

require (
	github.com/gorilla/websocket v1.5.3
	github.com/pion/ice/v4 v4.2.1
	github.com/pion/interceptor v0.1.44
	github.com/pion/webrtc/v4 v4.2.9
)
