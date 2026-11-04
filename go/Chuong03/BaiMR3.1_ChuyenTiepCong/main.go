// Bài mở rộng MR3.1 – Bộ chuyển tiếp cổng TCP (TCP port forwarder).
//
// Bài mở rộng, không có trong giáo trình; vận dụng Chương 3 (vòng đời socket TCP, SEND_ALL,
// SHUTDOWN một chiều) và Chương 6 (mỗi kết nối một goroutine).
// Một chương trình gồm ba vai trò, chọn bằng tham số đầu tiên:
//
//	out/app dich <host> <port>                              – server đích "chữ hoa"
//	out/app chuyentiep <host_nghe> <cổng_nghe> <host_đích> <cổng_đích>
//	out/app client <host> <port> <chuỗi> [số_lần_lặp]       – client thử
package main

import (
	"fmt"
	"os"
	"strconv"
)

func usage() {
	fmt.Println("Cách dùng:")
	fmt.Println("  out/app dich <host> <port>")
	fmt.Println("  out/app chuyentiep <host_nghe> <cổng_nghe> <host_đích> <cổng_đích>")
	fmt.Println("  out/app client <host> <port> <chuỗi> [số_lần_lặp]")
	os.Exit(1)
}

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	a := os.Args[2:]
	switch os.Args[1] {
	case "dich":
		if len(a) < 2 {
			usage()
		}
		runBackend(a[0], a[1])
	case "chuyentiep":
		if len(a) < 4 {
			usage()
		}
		runForwarder(a[0], a[1], a[2], a[3])
	case "client":
		if len(a) < 3 {
			usage()
		}
		n := 1
		if len(a) > 3 {
			n, _ = strconv.Atoi(a[3])
		}
		runClient(a[0], a[1], a[2], n)
	default:
		usage()
	}
}
