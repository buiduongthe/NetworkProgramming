// Bài 3.2 – Thư viện định khung và chương trình minh họa (Mã giả 3.3–3.6; Mục 3.2.4, 3.4 Giáo trình Lập trình mạng).
//
// Chạy:  go run . server [host] [port]               (mặc định 0.0.0.0 9000)
//
//	go run . client [host] [port] [so_ky_tu]    (mặc định 127.0.0.1 9000 0)
//	go run . line   [host] [port]               (minh họa READ_LINE, mặc định 127.0.0.1 9000)
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	rest := os.Args[2:]
	switch os.Args[1] {
	case "server":
		os.Exit(RunFrameServer(rest))
	case "client":
		os.Exit(RunFrameClient(rest))
	case "line":
		os.Exit(RunLineDemo(rest))
	}
	usage()
}

func usage() {
	fmt.Println("Cách dùng: go run . server|client|line [host] [port] [so_ky_tu]")
	os.Exit(1)
}
