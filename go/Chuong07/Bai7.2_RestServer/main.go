// Bài 7.2 – Server REST tối giản trên HTTP/1.1 (Mã giả 7.2) và client thử keep-alive.
//
// Một chương trình, hai vai trò:
//
//	out/app server [host] [port]   server REST
//	out/app client [host] [port]   client gửi các yêu cầu đọc từ bàn phím trên một kết nối
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	role, args := os.Args[1], os.Args[2:]
	switch role {
	case "server":
		loadGlossary()
		serverMain(args)
	case "client":
		clientMain(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app server|client [tham số...]")
	os.Exit(2)
}
