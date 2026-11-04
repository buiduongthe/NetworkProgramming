// Bài mở rộng MR3.2 – Tự động hóa cấu hình thiết bị qua CLI (kiểu expect/Netmiko, chỉ dùng socket).
//
// Bài mở rộng, không có trong giáo trình; vận dụng Chương 3 (luồng byte TCP, định khung bằng
// dấu phân cách) và Chương 6 (server nhiều phiên). Chương trình có hai vai trò:
//
//	out/app thietbi <host> <port>                      – thiết bị mô phỏng R1 (device.go)
//	out/app tudong <host> <port> <muc_tieu.json> [--thu] – client tự động hóa (client.go)
//
// Quy trình tự động hóa: đăng nhập, enable, terminal length 0; với mỗi cổng trong tệp
// trạng thái mong muốn: đọc cấu hình hiện tại, so sánh, CHỈ sinh lệnh cho phần khác biệt
// (lũy đẳng), áp dụng (hoặc chỉ in với --thu), đọc lại để kiểm chứng, lưu cấu hình.
// Tài khoản lấy từ biến môi trường DEVICE_USER, DEVICE_PASSWORD.
package main

import (
	"fmt"
	"os"
)

func main() {
	a := os.Args
	switch {
	case len(a) >= 4 && a[1] == "thietbi":
		runDevice(a[2], a[3])
	case len(a) >= 5 && a[1] == "tudong":
		runAutomation(a[2], a[3], a[4], len(a) > 5 && a[5] == "--thu")
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app thietbi <host> <port>")
		fmt.Println("  out/app tudong <host> <port> <muc_tieu.json> [--thu]")
		os.Exit(1)
	}
}
