// Bài 7.4 – Bộ đọc luồng Server-Sent Events (Mã giả 7.5) và server SSE nhỏ.
//
// Một chương trình, hai vai trò:
//
//	out/app server [host] [port]               server SSE phát luồng mẫu
//	out/app client [host] [port] [đường_dẫn]   bộ đọc luồng SSE
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
