// Bài 3.4 – Tải tệp lên server có kiểm tra SHA-256 (Mã giả 3.9, 3.10; Mục 3.5.2, 3.5.3 Giáo trình Lập trình mạng).
//
// Chạy trong thư mục bài (server đọc glossary.json và ghi tệp vào uploads/ ở thư mục hiện hành):
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
		os.Exit(RunPutServer(rest))
	case "client":
		os.Exit(RunPutClient(rest))
	case "kiemthu":
		os.Exit(RunKiemThu(rest))
	}
	usage()
}

func usage() {
	fmt.Println("Cách dùng: go run . server|client|kiemthu [host] [port]")
	os.Exit(1)
}
