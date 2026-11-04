// Bài 8.4 – Bổ sung TLS và xác thực token cho hệ thống chat (Mã giả 8.6).
//
// Một chương trình, ba vai trò:
//
//	out/app server [host] [port]                          server chat TLS
//	out/app client <host> <port> <tệp_token> [tệp_CA]     client chat TLS
//	out/app token <sub> <tệp_token> [aud] [thời_hạn_giây] cấp token JWT thử nghiệm
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
		os.Exit(clientMain(args))
	case "token":
		tokenMain(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app server|client|token [tham số...]")
	os.Exit(2)
}

func fail(msg string) {
	fmt.Fprintln(os.Stderr, msg)
	os.Exit(1)
}

func envOr(name, def string) string {
	if v := os.Getenv(name); v != "" {
		return v
	}
	return def
}

func mustPort(s string) int {
	p, err := strconv.Atoi(s)
	if err != nil || p < 1 || p > 65535 {
		fail("Cổng không hợp lệ: " + s)
	}
	return p
}
