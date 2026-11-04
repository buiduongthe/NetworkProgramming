// Bài 6.4 – Server chat nhiều phòng (Mã giả 6.5, 6.6) và client dòng lệnh (Mã giả 5.5).
//
// Một chương trình, hai vai trò:
//
//	out/app server [host] [port]          server chat (mặc định 0.0.0.0 9200)
//	out/app client [host] [port] [tên]    client chat dòng lệnh
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
		chatServer(args)
	case "client":
		chatClientMain(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app server|client [tham số...]")
	os.Exit(2)
}
