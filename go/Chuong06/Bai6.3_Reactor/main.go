// Bài 6.3 – Server echo một luồng theo mô hình reactor (Mã giả 6.4).
//
// Một chương trình, hai vai trò:
//
//	out/app server [host] [port]            server reactor một luồng (Mã giả 6.4, Linux)
//	out/app client [host] [port] [n] [size] n client echo đồng thời
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
		reactorServer(args)
	case "client":
		echoClients(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app server|client [tham số...]")
	os.Exit(2)
}
