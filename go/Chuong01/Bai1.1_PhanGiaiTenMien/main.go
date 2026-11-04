// Bài 1.1 – Phân giải tên miền thành danh sách địa chỉ.
//
// Cài đặt Mã giả 1.1 – RESOLVE_HOST (Mục 1.2.3, Giáo trình Lập trình mạng).
// Với mỗi tên nhận từ dòng lệnh, chương trình gọi bộ phân giải của Go (net.LookupIP),
// in từng địa chỉ kèm họ địa chỉ (INET/INET6); lỗi phân giải được bắt và trả danh sách rỗng.
//
// Chạy:  go run . [tên ...]      (mặc định: localhost)
// Ví dụ: go run . localhost example.com
package main

import (
	"fmt"
	"os"
)

func main() {
	names := os.Args[1:]
	if len(names) == 0 {
		names = []string{"localhost"}
	}
	for _, name := range names {
		fmt.Printf("[resolve] Phân giải tên: %s\n", name)
		addrs := ResolveHost(name)
		fmt.Printf("[resolve] %s -> %d địa chỉ\n", name, len(addrs))
	}
}
