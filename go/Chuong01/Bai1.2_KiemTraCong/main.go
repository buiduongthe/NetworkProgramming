// Bài 1.2 – Kiểm tra trạng thái một cổng TCP.
//
// Cài đặt Mã giả 1.2 – PORT_CHECK (Mục 1.2.5, Giáo trình Lập trình mạng),
// kế thừa Mã giả 1.1 – RESOLVE_HOST (Mục 1.2.3, tệp resolver.go chép từ Bài 1.1).
// Kết quả là một trong bốn trạng thái: OPEN, CLOSED, FILTERED, UNRESOLVED.
// Chỉ kiểm tra trên máy và mạng thuộc quyền quản lý của người thực hiện.
//
// Chạy:  go run . <host> <port> [timeout_ms]      (mặc định timeout_ms = 2000)
// Ví dụ: go run . localhost 9000
package main

import (
	"fmt"
	"os"
	"strconv"
)

func main() {
	if len(os.Args) < 3 {
		fmt.Println("Cách dùng: go run . <host> <port> [timeout_ms]")
		os.Exit(1)
	}
	host := os.Args[1]
	port, err := strconv.Atoi(os.Args[2])
	if err != nil || port < 1 || port > 65535 {
		fmt.Printf("Cổng không hợp lệ: %s\n", os.Args[2])
		os.Exit(1)
	}
	timeoutMs := 2000
	if len(os.Args) > 3 {
		timeoutMs, err = strconv.Atoi(os.Args[3])
		if err != nil || timeoutMs <= 0 {
			fmt.Printf("Thời gian chờ không hợp lệ: %s\n", os.Args[3])
			os.Exit(1)
		}
	}
	fmt.Printf("[port-check] Kiểm tra %s:%d (thời gian chờ %d ms)\n", host, port, timeoutMs)
	state, err := PortCheck(host, port, timeoutMs)
	if err != nil { // lỗi ngoài các loại mã giả bắt: thoát ra ngoài như mã giả
		fmt.Printf("[port-check] Lỗi không mong đợi: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("[port-check] Kết quả: %s:%d -> %s\n", host, port, state)
}
