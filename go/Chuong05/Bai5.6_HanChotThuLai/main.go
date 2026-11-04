// Bài 5.6 – Hạn chót, hủy và thử lại có lùi thời gian (Mã giả 5.7, 5.8; Mục 5.4.2, 5.4.3 Giáo trình Lập trình mạng).
//
// Chạy:  go run . server [host] [port]                          (server "khó tính", mặc định 0.0.0.0 9100)
//
//	go run . client [host] [port] [cổng_không_có_server]   (ba kịch bản, mặc định 127.0.0.1 9100 9101)
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 || (os.Args[1] != "server" && os.Args[1] != "client") {
		fmt.Println("Cách dùng: go run . server [host] [port] | client [host] [port] [cổng_không_có_server]")
		os.Exit(1)
	}
	rest := os.Args[2:]
	if os.Args[1] == "server" {
		os.Exit(RunFlakyServer(rest))
	}
	os.Exit(RunDeadlineRetry(rest))
}
