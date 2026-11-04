// Bài 3.1 – TCP Echo Server tuần tự và TCP Echo Client (Mã giả 3.1, 3.2; Mục 3.2 Giáo trình Lập trình mạng).
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
		os.Exit(RunEchoServer(rest))
	}
	os.Exit(RunEchoClient(rest))
}
