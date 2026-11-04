// Bài mở rộng MR4.1 – Thu thập và phân tích NetFlow v5 trên UDP.
//
// Bài mở rộng, không có trong giáo trình; vận dụng Chương 4: server UDP nhận datagram,
// mỗi datagram là trọn một gói xuất (UDP giữ ranh giới thông điệp), giải mã nhị phân
// bằng encoding/binary như Bài 4.3 (truy vấn DNS); phát hiện mất gói nhờ flow_sequence.
//
//	out/app thuthap <host> <port> [giây_nhàn_rỗi]          – bộ thu thập (collector.go)
//	out/app xuat <host> <port> [số_luồng] [gói_bỏ]          – máy xuất giả lập (exporter.go)
package main

import (
	"fmt"
	"net"
	"os"
	"strconv"
	"time"
)

func main() {
	a := os.Args
	if len(a) < 4 || (a[1] != "thuthap" && a[1] != "xuat") {
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app thuthap <host> <port> [giây_nhàn_rỗi]")
		fmt.Println("  out/app xuat <host> <port> [số_luồng] [gói_bỏ]")
		os.Exit(1)
	}
	addr := net.JoinHostPort(a[2], a[3])
	if a[1] == "thuthap" {
		idle := 1.5
		if len(a) > 4 {
			idle, _ = strconv.ParseFloat(a[4], 64)
		}
		runCollector(addr, time.Duration(idle*float64(time.Second)))
		return
	}
	n, drop := 100, 0
	if len(a) > 4 {
		n, _ = strconv.Atoi(a[4])
	}
	if len(a) > 5 {
		drop, _ = strconv.Atoi(a[5])
	}
	runExporter(addr, n, drop)
}
