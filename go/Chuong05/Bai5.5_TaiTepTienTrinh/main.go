// Bài 5.5 – Tải tệp bất đồng bộ có báo cáo tiến trình và hủy (Mã giả 5.6; Mục 5.3.4 Giáo trình Lập trình mạng).
//
// Chạy:  go run . server [host] [port] [rate_kib]            (mặc định 0.0.0.0 9100 0; tệp lưu trong uploads/)
//
//	go run . client [host] [port] [tệp] [hủy_sau_ms]    (mặc định 127.0.0.1 9100 du_lieu_thu.bin; không hủy)
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 || (os.Args[1] != "server" && os.Args[1] != "client") {
		fmt.Println("Cách dùng: go run . server [host] [port] [rate_kib] | client [host] [port] [tệp] [hủy_sau_ms]")
		os.Exit(1)
	}
	rest := os.Args[2:]
	if os.Args[1] == "server" {
		os.Exit(RunPutServer(rest))
	}
	os.Exit(RunPutClient(rest))
}
