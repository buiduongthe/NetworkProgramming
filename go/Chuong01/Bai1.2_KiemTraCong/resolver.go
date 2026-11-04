// Chép từ Bài 1.1 – Mã giả 1.1 (RESOLVE_HOST), chỉ đổi tiền tố thông điệp thành [port-check].

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

// ResolveHost – Mã giả 1.1 – RESOLVE_HOST(name): trả về danh sách địa chỉ; rỗng nếu lỗi.
func ResolveHost(name string) []net.IP {
	addrs, err := net.LookupIP(name) // dòng 3: RESOLVE(name)
	if err != nil {                  // CATCH ResolveError AS e
		code := err.Error()
		var dnsErr *net.DNSError
		if errors.As(err, &dnsErr) {
			code = dnsErr.Err // e.code: mô tả lỗi của bộ phân giải
		}
		fmt.Printf("[port-check] Không phân giải được %s (lỗi: %s)\n", name, code)
		return nil // RETURN EMPTY_LIST
	}
	for _, a := range addrs { // FOR EACH a IN addrs
		fmt.Printf("  %-6s %s\n", Family(a), a) // PRINT(FAMILY(a), ADDRESS(a))
	}
	return addrs // RETURN addrs
}
