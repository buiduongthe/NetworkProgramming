// Bài 4.1 – UDP Echo Server và UDP Echo Client có thời gian chờ (Mã giả 4.1, 4.2; Mục 4.2.1 Giáo trình Lập trình mạng).
//
// Chạy:  go run . server [host] [port]      (mặc định 0.0.0.0 9000)
//
//	go run . client [host] [port]      (mặc định 127.0.0.1 9000)
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
		os.Exit(RunUdpEchoServer(rest))
	}
	os.Exit(RunUdpEchoClient(rest))
}
