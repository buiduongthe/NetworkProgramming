// Bài 4.3 – Tự tạo và phân tích truy vấn DNS qua UDP (Mã giả 4.5; Mục 4.3.2 Giáo trình Lập trình mạng, RFC 1035).
//
// Chạy:  go run . <ip_dns_server> <ten_mien> [A|AAAA] [cong]     (mặc định A, cổng 53)
// Ví dụ: go run . 8.8.8.8 example.com AAAA
package main

import (
	"errors"
	"fmt"
	"net"
	"os"
	"strconv"
	"strings"
)

func main() {
	os.Exit(run())
}

func run() int {
	if len(os.Args) < 3 {
		fmt.Println("Cách dùng: go run . <ip_dns_server> <ten_mien> [A|AAAA] [cong]")
		return 1
	}
	serverIP := net.ParseIP(os.Args[1])
	if serverIP == nil {
		fmt.Printf("Địa chỉ DNS server không hợp lệ: %s\n", os.Args[1])
		return 1
	}
	name := os.Args[2]
	tname := "A"
	if len(os.Args) > 3 {
		tname = strings.ToUpper(os.Args[3])
	}
	port := 53
	if len(os.Args) > 4 {
		p, err := strconv.Atoi(os.Args[4])
		if err != nil || p < 1 || p > 65535 {
			fmt.Printf("Cổng không hợp lệ: %s\n", os.Args[4])
			return 1
		}
		port = p
	}
	qtypes := map[string]uint16{"A": 1, "AAAA": 28}
	qtype, ok := qtypes[tname]
	if !ok {
		fmt.Println("Kiểu bản ghi chỉ hỗ trợ A hoặc AAAA")
		return 1
	}
	_, answers, err := DnsQuery(serverIP, port, name, qtype)
	var ne net.Error
	var pe *ProtocolError
	switch {
	case errors.As(err, &ne) && ne.Timeout():
		fmt.Println("[client] Lỗi: hết thời gian chờ phản hồi (2000 ms)")
		return 1
	case errors.As(err, &pe):
		fmt.Printf("[client] Lỗi giao thức: %v\n", pe)
		return 1
	case err != nil:
		fmt.Printf("[client] Lỗi: %v\n", err)
		return 1
	}
	for _, a := range answers {
		fmt.Printf("[client] Địa chỉ: %s\n", a)
	}
	if len(answers) == 0 {
		fmt.Printf("[client] Không có bản ghi %s\n", tname)
	}
	return 0
}
