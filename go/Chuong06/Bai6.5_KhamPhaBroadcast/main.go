// Bài 6.5 – Khám phá server trong LAN bằng broadcast (Mã giả 6.7).
//
// Một chương trình, hai vai trò:
//
//	out/app responder [service_port] [tên] [disc_port]   phía server (RESPONDER)
//	out/app discover [wait_ms] [disc_port] [broadcast]   phía client (DISCOVER)
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
	case "responder":
		responderMain(args)
	case "discover":
		discoverMain(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app responder|discover [tham số...]")
	os.Exit(2)
}
