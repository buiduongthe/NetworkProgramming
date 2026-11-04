// Bài 2.1 – Đọc địa chỉ và cổng từ tham số dòng lệnh, biến môi trường.
//
// Cài đặt Mã giả 2.1 PARSE_ENDPOINT (Mục 2.1.3, Giáo trình Lập trình mạng).
// Thứ tự ưu tiên: tham số dòng lệnh > biến môi trường APP_HOST, APP_PORT > giá trị mặc định.
// Chương trình chỉ in cấu hình nhận được, không mở socket.
//
// Chạy:  go run . [host] [port]                 (mặc định 0.0.0.0 9000)
// Ví dụ: APP_PORT=9100 go run .                 (Linux/macOS)
//
//	$env:APP_PORT="9100"; go run .         (Windows PowerShell)
package main

import (
	"fmt"
	"os"
	"strings"
)

// envOr trả về giá trị biến môi trường hoặc "(không đặt)".
func envOr(name string) string {
	if v, ok := os.LookupEnv(name); ok {
		return v
	}
	return "(không đặt)"
}

func main() {
	args := os.Args[1:]
	argText := "(không có)"
	if len(args) > 0 {
		argText = strings.Join(args, " ")
	}
	// In dữ liệu vào để thấy rõ giá trị nào thắng
	fmt.Printf("[cấu hình] APP_HOST=%s, APP_PORT=%s, tham số: %s\n", envOr("APP_HOST"), envOr("APP_PORT"), argText)
	host, port, err := ParseEndpoint(args, "0.0.0.0", 9000)
	if err != nil {
		fmt.Printf("[cấu hình] ConfigError: %v\n", err)
		os.Exit(2)
	}
	fmt.Printf("[cấu hình] Kết quả: host=%s, port=%d\n", host, port)
}
