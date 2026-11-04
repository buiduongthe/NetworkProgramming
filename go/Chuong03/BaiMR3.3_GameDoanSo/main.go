// Bài mở rộng MR3.3 – Trò chơi đoán số nhiều người chơi trên TCP (khung JSON có tiền tố độ dài).
//
// Bài mở rộng, không có trong giáo trình; vận dụng Chương 3 (Bài 3.2: định khung, Bài 3.3: giao thức
// yêu cầu/phản hồi) và Chương 6 (server nhiều client). Một chương trình, hai vai trò:
//
//	out/app server [host] [port] [--so-nguoi 2] [--het-gio-ms 30000] [--seed 0] [--min 1] [--max 100] [--so-van 0]
//	out/app choi <tên> [host] [port] [--bot chia_doi|ngau_nhien|nghich|im_lang] [--seed 1] [--nghi-ms 300]
//
// Bỏ --bot để tự chơi bằng bàn phím.
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
	default:
		usage()
	}
}

func usage() {
	fmt.Println("Cách dùng:")
	fmt.Println("  out/app server [host] [port] [--so-nguoi 2] [--het-gio-ms 30000] [--seed 0] [--so-van 0]")
	fmt.Println("  out/app choi <tên> [host] [port] [--bot chia_doi|ngau_nhien|nghich|im_lang]")
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
