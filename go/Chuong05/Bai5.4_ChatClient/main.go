// Bài 5.4 – Lớp dịch vụ ChatClient bất đồng bộ (Mã giả 5.5; Mục 5.3.3 Giáo trình Lập trình mạng).
//
// Chạy:  go run . server [host] [port]          (server chat tối giản, mặc định 0.0.0.0 9200)
//
//	go run . cli <tên> [host] [port]       (giao diện dòng lệnh, mặc định 127.0.0.1 9200)
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 || (os.Args[1] != "server" && os.Args[1] != "cli") {
		fmt.Println("Cách dùng: go run . server [host] [port] | cli <tên> [host] [port]")
		os.Exit(1)
	}
	rest := os.Args[2:]
	if os.Args[1] == "server" {
		os.Exit(RunChatServer(rest))
	}
	os.Exit(RunChatCli(rest))
}
