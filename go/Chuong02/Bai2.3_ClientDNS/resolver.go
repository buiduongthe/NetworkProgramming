// Chép từ Bài 1.1 – Mã giả 1.1 (RESOLVE_HOST), chỉ đổi cách in cho gọn.

package main

import (
	"errors"
	"fmt"
	"net"
)

// Family – FAMILY(a): INET cho IPv4, INET6 cho IPv6.
func Family(a net.IP) string {
	if a.To4() != nil {
		return "INET"
	}
	return "INET6"
}

// ResolveHost – Mã giả 1.1 – RESOLVE_HOST: in và trả về danh sách địa chỉ; lỗi → danh sách rỗng.
func ResolveHost(name string) []net.IP {
	addrs, err := net.LookupIP(name) // RESOLVE(name)
	if err != nil {                  // ResolveError
		code := err.Error()
		var dnsErr *net.DNSError
		if errors.As(err, &dnsErr) {
			code = dnsErr.Err
		}
		fmt.Printf("  Không phân giải được %s (lỗi: %s)\n", name, code)
		return nil
	}
	for _, a := range addrs {
		fmt.Printf("  %s  %s\n", Family(a), a) // FAMILY, ADDRESS
	}
	return addrs
}
