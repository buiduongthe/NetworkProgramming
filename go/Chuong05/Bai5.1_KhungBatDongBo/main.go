// Bài 5.1 – Các hàm gửi, nhận khung JSON bất đồng bộ (Mã giả 5.1; Mục 5.2.4 Giáo trình Lập trình mạng).
//
// Chạy:  go run . server [host] [port]      (mặc định 0.0.0.0 9100)
//
//	go run . client [host] [port]      (mặc định 127.0.0.1 9100)
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 || (os.Args[1] != "server" && os.Args[1] != "client") {
		fmt.Println("Cách dùng: go run . server|client [host] [port]")
		os.Exit(1)
	}
	rest := os.Args[2:]
	if os.Args[1] == "server" {
		os.Exit(RunFrameServer(rest))
	}
	os.Exit(RunFrameClient(rest))
}
