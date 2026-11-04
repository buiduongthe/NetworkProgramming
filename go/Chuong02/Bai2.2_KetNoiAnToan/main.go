// Bài 2.2 – Kết nối có thời gian chờ và phân loại lỗi.
//
// Cài đặt Mã giả 2.2 SAFE_CONNECT (Mục 2.2.3, Giáo trình Lập trình mạng).
// Lỗi được gom về một loại NetError với ba nhóm: NOT_FOUND, REFUSED (không nên thử lại)
// và UNREACHABLE (hết thời gian chờ / không có đường đi, có thể thử lại).
// Kế thừa Mã giả 2.1 (PARSE_ENDPOINT) để đọc địa chỉ, cổng.
//
// Chạy:  go run . client [host] [port] [timeout_ms]   (mặc định 127.0.0.1 9000 3000)
//
//	go run . server [host] [port]                (server thử nghiệm, mặc định 0.0.0.0 9000)
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 || (os.Args[1] != "server" && os.Args[1] != "client") {
		fmt.Println("Cách dùng: go run . server|client [host] [port] [timeout_ms]")
		os.Exit(1)
	}
	rest := os.Args[2:]
	if os.Args[1] == "server" {
		os.Exit(RunTestServer(rest))
	}
	os.Exit(RunClient(rest))
}
