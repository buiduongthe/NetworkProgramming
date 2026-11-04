// Bài 4.4 – Truyền tệp dừng và chờ trên UDP (Mã giả 4.6, 4.7; Mục 4.4.1–4.4.2 Giáo trình Lập trình mạng).
//
// Chạy:  go run . recv [host] [port] [tep_ra] [mat_goi]       (mặc định 0.0.0.0 9000 tep_nhan.bin)
//
//	go run . send <tep> [host] [port] [mat_goi]         (mặc định 127.0.0.1 9000)
//
// mat_goi: "0.1" (ngẫu nhiên 10%) hoặc "3,7" (mất datagram gửi đi thứ 3 và 7); bỏ trống: không mất gói.
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 || (os.Args[1] != "send" && os.Args[1] != "recv") || (os.Args[1] == "send" && len(os.Args) < 3) {
		fmt.Println("Cách dùng: go run . recv [host] [port] [tep_ra] [mat_goi] | send <tep> [host] [port] [mat_goi]")
		os.Exit(1)
	}
	rest := os.Args[2:]
	if os.Args[1] == "send" {
		os.Exit(RunSend(rest))
	}
	os.Exit(RunRecv(rest))
}
