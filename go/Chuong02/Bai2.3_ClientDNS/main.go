// Bài 2.3 – Client tra cứu DNS dùng bộ phân giải của hệ điều hành.
//
// Cài đặt Mã giả 2.3 (Mục 2.5, Giáo trình Lập trình mạng), kế thừa Mã giả 1.1 RESOLVE_HOST.
// Mỗi tham số là một tên miền (tra cứu xuôi: tên → địa chỉ IPv4/IPv6)
// hoặc một địa chỉ IP (tra cứu ngược: địa chỉ → tên, bản ghi PTR).
//
// Chạy:  go run . <tên miền | địa chỉ IP> ...
// Ví dụ: go run . localhost 127.0.0.1 8.8.8.8
package main

import (
	"fmt"
	"net"
	"os"
	"strings"
	"time"
)

// IsIPAddress – IS_IP_ADDRESS: chuỗi là địa chỉ IPv4/IPv6 dạng số hay là tên miền.
func IsIPAddress(q string) bool {
	return net.ParseIP(q) != nil
}

func main() {
	args := os.Args[1:]
	if len(args) == 0 { // dòng 1–5
		fmt.Println("Cách dùng: go run . <tên miền | địa chỉ IP> ...")
		return
	}
	for _, q := range args { // dòng 6: FOR EACH q IN args
		t0 := time.Now()    // dòng 7: t0 ← NOW()
		if IsIPAddress(q) { // dòng 8
			fmt.Printf("[dns] Tra cứu ngược: %s\n", q)
			names, err := net.LookupAddr(q)    // REVERSE_RESOLVE(q): bản ghi PTR
			if err != nil || len(names) == 0 { // CATCH ResolveError
				fmt.Printf("  %s → không có bản ghi PTR\n", q)
			} else {
				fmt.Printf("  %s → %s\n", q, strings.TrimSuffix(names[0], "."))
			}
		} else {
			fmt.Printf("[dns] Tra cứu xuôi: %s\n", q)
			ResolveHost(q) // = Mã giả 1.1
		}
		ms := float64(time.Since(t0).Microseconds()) / 1000
		fmt.Printf("  Thời gian: %.2f ms\n", ms) // dòng 18
	}
}
