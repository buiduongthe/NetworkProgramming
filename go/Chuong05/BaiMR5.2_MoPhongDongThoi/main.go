// Bài mở rộng MR5.2 – So sánh các mô hình đồng thời trên cùng một khối việc (Go).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1, 5.2 – bất đồng bộ, giới hạn
// đồng thời) và Chương 6 (Bài 6.1, 6.2 – server đa luồng, nhóm luồng, bất đồng bộ).
//
// Ba khối việc:
//
//	mang  – 20 yêu cầu mạng tới máy chủ thử (mỗi yêu cầu trễ 200 ms): việc chờ I/O;
//	cpu   – 8 tác vụ tính toán (mỗi tác vụ ≈ 100 ms): việc nặng CPU;
//	tron  – 20 tác vụ = yêu cầu mạng 200 ms + xử lý CPU ≈ 30 ms: goroutine + nhóm worker CPU.
//
// Thêm: khoa – tranh chấp dữ liệu và cách sửa (Mutex, atomic), bế tắc và cách tránh.
//
//	out/app server <host> <port> [trễ_ms]
//	out/app mang <host> <port>
//	out/app cpu
//	out/app tron <host> <port>
//	out/app khoa
package main

import (
	"fmt"
	"os"
	"strconv"
	"time"
)

func main() {
	a := os.Args
	switch {
	case len(a) >= 4 && a[1] == "server":
		ms := 200
		if len(a) > 4 {
			ms, _ = strconv.Atoi(a[4])
		}
		runServer(a[2], a[3], time.Duration(ms)*time.Millisecond)
	case len(a) >= 4 && a[1] == "mang":
		runNet(a[2], a[3])
	case len(a) >= 2 && a[1] == "cpu":
		runCPU()
	case len(a) >= 4 && a[1] == "tron":
		runMixed(a[2], a[3])
	case len(a) >= 2 && a[1] == "khoa":
		raceDemo()
		deadlockDemo()
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app server <host> <port> [trễ_ms]")
		fmt.Println("  out/app mang <host> <port> | out/app cpu | out/app tron <host> <port> | out/app khoa")
		os.Exit(1)
	}
}
