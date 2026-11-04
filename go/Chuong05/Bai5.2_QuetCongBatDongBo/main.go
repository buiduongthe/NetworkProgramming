// Bài 5.2 – Kiểm tra nhiều cổng đồng thời có giới hạn (Mã giả 5.2; Mục 5.2.5 Giáo trình Lập trình mạng).
//
// Chạy:  go run . scan [host] [cổng_đầu] [cổng_cuối] [max_parallel]   (mặc định 127.0.0.1 9000 9019 10)
//
//	go run . ports <cổng_mở,...> <cổng_lọc,...>                  (mở cổng minh họa trên 127.0.0.1)
//
// Chỉ chạy trên máy thuộc quyền quản lý hoặc mạng thí nghiệm cô lập (Mục 1.2.5).
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 || (os.Args[1] != "scan" && os.Args[1] != "ports") {
		fmt.Println("Cách dùng: go run . scan|ports ...")
		os.Exit(1)
	}
	rest := os.Args[2:]
	if os.Args[1] == "scan" {
		os.Exit(RunScan(rest))
	}
	os.Exit(RunDemoPorts(rest))
}
