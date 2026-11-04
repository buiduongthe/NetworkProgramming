// Bài mở rộng MR6.4 – Trò chơi Caro (5 quân liên tiếp trên bàn 15×15) nhiều phòng qua TCP,
// khám phá server trong LAN bằng UDP broadcast.
//
// Bài mở rộng, không có trong giáo trình; vận dụng Chương 6 (Bài 6.4: server chat nhiều client,
// phát tin; Bài 6.5: khám phá bằng broadcast) và Chương 3 (Bài 3.2: định khung JSON). Ba vai trò:
//
//	out/app server [host] [port] [--kham-pha 9999] [--ten Caro] [--het-gio-ms 30000]
//	out/app choi <tên> [host] [port] [--tim CỔNG] [--tao P | --vao P | --xem P]
//	             [--bot KIỂU] [--seed 1] [--thu-loi] [--so-van 1] [--tre-ms 300]
//	out/app tim [cổng_khám_phá] [chờ_ms] [broadcast]
package main

import (
	"fmt"
	"os"
	"os/signal"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	switch os.Args[1] {
	case "server":
		runServer(os.Args[2:])
	case "choi":
		runPlayer(os.Args[2:])
	case "tim":
		runFind(os.Args[2:])
	default:
		usage()
	}
}

func usage() {
	fmt.Println("Cách dùng:")
	fmt.Println("  out/app server [host] [port] [--kham-pha 9999] [--ten Caro] [--het-gio-ms 30000]")
	fmt.Println("  out/app choi <tên> [host] [port] [--tim CỔNG] [--tao P | --vao P | --xem P] [--bot KIỂU]")
	fmt.Println("  out/app tim [cổng_khám_phá] [chờ_ms] [broadcast]")
	os.Exit(1)
}

// onInterrupt: khi nhấn Ctrl+C (SIGINT), in thông báo rồi kết thúc chương trình.
func onInterrupt(msg string) {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		fmt.Println(msg)
		os.Exit(0)
	}()
}
