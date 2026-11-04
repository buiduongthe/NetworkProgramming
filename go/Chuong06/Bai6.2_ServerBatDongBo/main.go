// Bài 6.2 – Server tra cứu bất đồng bộ (Mã giả 6.3).
//
// Một chương trình, hai vai trò:
//
//	out/app server [host] [port]               server bất đồng bộ (Mã giả 6.3)
//	out/app client [host] [port] [n] [hold_ms] mở n client đồng thời
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
		asyncServer(args)
	case "client":
		multiClient(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app server|client [tham số...]")
	os.Exit(2)
}
