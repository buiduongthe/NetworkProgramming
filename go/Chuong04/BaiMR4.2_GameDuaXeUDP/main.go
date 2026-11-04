// Bài mở rộng MR4.2 – Trò chơi đua xe thời gian thực trên UDP: vòng tick có thẩm quyền ở server,
// đầu vào có số thứ tự và dư thừa, ảnh chụp trạng thái (ảnh mới nhất thắng), dự đoán phía client,
// giả lập mất gói để so sánh 0 % với 20 %.
//
// Bài mở rộng, không có trong giáo trình; vận dụng Chương 4 (Bài 4.1, 4.2: UDP; Bài 4.4: số thứ tự,
// mất gói). Hai vai trò:
//
//	out/app server [host] [port] [--so-xe 2] [--mat 0] [--dao 0] [--seed 1] [--so-van 0]
//	out/app xe <tên> [host] [port] [--bot lieu|can_than] [--khung 10] [--so-van 1]
//
// Bỏ --bot để tự lái: gõ g (ga), n (nhả), p (phanh) rồi Enter.
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	switch os.Args[1] {
	case "server":
		runServer(os.Args[2:])
	case "xe":
		runCar(os.Args[2:])
	default:
		usage()
	}
}

func usage() {
	fmt.Println("Cách dùng:")
	fmt.Println("  out/app server [host] [port] [--so-xe 2] [--mat 0] [--dao 0] [--seed 1] [--so-van 0]")
	fmt.Println("  out/app xe <tên> [host] [port] [--bot lieu|can_than] [--khung 10] [--so-van 1]")
	os.Exit(1)
}
