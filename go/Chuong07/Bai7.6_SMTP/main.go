// Bài 7.6 – Phiên gửi thư SMTP có STARTTLS và AUTH PLAIN (Mã giả 7.7), kèm server SMTP thử.
//
// Một chương trình, hai vai trò:
//
//	out/app client [host] [port] [tệp_CA]   client SMTP (Mã giả 7.7)
//	out/app server [host] [port]            server SMTP thử nghiệm cục bộ
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
	case "client":
		clientMain(args)
	case "server":
		serverMain(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app client|server [tham số...]")
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
