// Bài mở rộng MR7.1 – Tải tệp HTTP có tải tiếp (Range, If-Range, RFC 9110).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 7.1 (client HTTP), Bài 5.6 (thử lại
// có lùi thời gian) và Bài 3.4 (kiểm tra SHA-256).
//
//	out/app server <host> <port> [kích_thước] [cắt_sau] [số_lần_cắt]   – server tệp (server.go)
//	out/app tai <url> <tệp_lưu> [số_lần_thử]                           – client (client.go)
package main

import (
	"fmt"
	"net"
	"os"
	"strconv"
)

func atoi(a []string, i, def int) int {
	if i < len(a) {
		if v, err := strconv.Atoi(a[i]); err == nil {
			return v
		}
	}
	return def
}

func main() {
	a := os.Args
	switch {
	case len(a) >= 4 && a[1] == "server":
		runServer(net.JoinHostPort(a[2], a[3]), atoi(a, 4, 300000), atoi(a, 5, 100000), atoi(a, 6, 0))
	case len(a) >= 4 && a[1] == "tai":
		runClient(a[2], a[3], atoi(a, 4, 5))
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app server <host> <port> [kích_thước] [cắt_sau] [số_lần_cắt]")
		fmt.Println("  out/app tai <url> <tệp_lưu> [số_lần_thử]")
		os.Exit(1)
	}
}
