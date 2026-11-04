// Bài 7.5 – Dịch vụ gRPC tra cứu thuật ngữ (Mã giả 7.6).
//
// Một chương trình, hai vai trò:
//
//	out/app server [host] [port] [độ_trễ_ms]    server gRPC
//	out/app client [host] [port] [hạn_chót_ms]  client gRPC
package main

import (
	"fmt"
	"os"
	"strconv"
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

func fail(msg string) {
	fmt.Fprintln(os.Stderr, msg)
	os.Exit(1)
}

// mustPort: kiểm tra cổng 1–65535.
func mustPort(s string) int {
	p, err := strconv.Atoi(s)
	if err != nil || p < 1 || p > 65535 {
		fail("Cổng không hợp lệ: " + s)
	}
	return p
}
