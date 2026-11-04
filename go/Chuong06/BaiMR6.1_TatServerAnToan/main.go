// Bài mở rộng MR6.1 – Tắt server an toàn (graceful shutdown) với context và WaitGroup.
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1, 6.2 (server nhiều client) và
// Bài 5.6 (hạn chót, hủy). Khi nhận SIGTERM hoặc SIGINT (Ctrl+C), server:
//  1. đóng listener: ngừng nhận kết nối mới;
//  2. kết nối đang rỗi: gửi "BYE" rồi đóng ngay;
//  3. kết nối đang làm dở: làm xong yêu cầu hiện tại, trả kết quả, gửi "BYE", đóng;
//  4. quá hạn chờ: hủy các yêu cầu còn dở (context bị hủy), báo "HUY" cho client.
//
// PID được ghi vào server.pid để gửi tín hiệu bằng: kill -TERM $(cat server.pid)
//
//	out/app server <host> <port> [hạn_chờ_ms]    (mặc định 2000)
//	out/app khach <tên> <host> <port> [ms ...]
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
		drain := 2000
		if len(a) > 4 {
			drain, _ = strconv.Atoi(a[4])
		}
		runServer(a[2], a[3], time.Duration(drain)*time.Millisecond)
	case len(a) >= 5 && a[1] == "khach":
		runClient(a[2], a[3], a[4], a[5:])
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app server <host> <port> [hạn_chờ_ms]")
		fmt.Println("  out/app khach <tên> <host> <port> [ms ...]")
		os.Exit(1)
	}
}
