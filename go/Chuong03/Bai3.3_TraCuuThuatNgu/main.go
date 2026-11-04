// Bài 3.3 – Dịch vụ tra cứu thuật ngữ dùng khung JSON (Mã giả 3.7, 3.8; Mục 3.5.1, 3.5.3 Giáo trình Lập trình mạng).
//
// Chạy trong thư mục bài (server đọc glossary.json ở thư mục hiện hành):
//
//	go run . server  [host] [port]      (mặc định 0.0.0.0 9100)
//	go run . client  [host] [port]      (mặc định 127.0.0.1 9100)
//	go run . kiemthu [host] [port]      (client kiểm thử, mặc định 127.0.0.1 9100)
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	rest := os.Args[2:]
	switch os.Args[1] {
	case "server":
		os.Exit(RunLookupServer(rest))
	case "client":
		os.Exit(RunLookupClient(rest))
	case "kiemthu":
		os.Exit(RunKiemThu(rest))
	}
	usage()
}

func usage() {
	fmt.Println("Cách dùng: go run . server|client|kiemthu [host] [port]")
	os.Exit(1)
}
