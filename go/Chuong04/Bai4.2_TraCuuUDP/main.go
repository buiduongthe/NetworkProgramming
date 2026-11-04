// Bài 4.2 – Dịch vụ tra cứu thuật ngữ trên UDP có gửi lại (Mã giả 4.3, 4.4; Mục 4.3.1 Giáo trình Lập trình mạng).
//
// Chạy trong thư mục bài (server đọc glossary.json ở thư mục hiện hành):
//
//	go run . server [host] [port] [so_datagram_bo_qua]   (mặc định 0.0.0.0 9100 0)
//	go run . client [host] [port]                        (mặc định 127.0.0.1 9100)
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 || (os.Args[1] != "server" && os.Args[1] != "client") {
		fmt.Println("Cách dùng: go run . server|client [host] [port] [so_datagram_bo_qua]")
		os.Exit(1)
	}
	rest := os.Args[2:]
	if os.Args[1] == "server" {
		os.Exit(RunUdpLookupServer(rest))
	}
	os.Exit(RunUdpLookupClient(rest))
}
