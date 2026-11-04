// Bài 5.3 – Ứng dụng tra cứu có giao diện bất đồng bộ (Mã giả 5.3, 5.4; Mục 5.3.2 Giáo trình Lập trình mạng).
//
// Chạy:  go run . server [host] [port] [delay_ms]          (server tra cứu, mặc định 0.0.0.0 9100 0)
//
//	go run . gui [host] [port] [cổng_giao_diện]       (giao diện web, mặc định 127.0.0.1 9100 8080)
//
// Sau khi chạy vai trò gui, mở trình duyệt tại địa chỉ chương trình in ra.
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 || (os.Args[1] != "server" && os.Args[1] != "gui") {
		fmt.Println("Cách dùng: go run . server [host] [port] [delay_ms] | gui [host] [port] [cổng_giao_diện]")
		os.Exit(1)
	}
	rest := os.Args[2:]
	if os.Args[1] == "server" {
		os.Exit(RunLookupServer(rest))
	}
	os.Exit(RunLookupGui(rest))
}
