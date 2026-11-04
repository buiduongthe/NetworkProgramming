// Bài 7.1 – Client HTTP/1.1 tối giản trên socket TCP (Mã giả 7.1) và server thử.
//
// Một chương trình, hai vai trò:
//
//	out/app server [host] [port]                 server thử ba kiểu định khung thân
//	out/app client [host] [port] [đường_dẫn ...] client HTTP/1.1
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
