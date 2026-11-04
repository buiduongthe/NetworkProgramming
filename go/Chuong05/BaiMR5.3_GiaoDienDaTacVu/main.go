// Bài mở rộng MR5.3 – Giao diện luôn phản hồi khi nhiều tác vụ mạng chạy song song (Go, giao diện web).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 5.3 (giao diện bất đồng bộ), Bài 5.5 (tải tệp
// có tiến trình và hủy), Bài 5.2 (giới hạn đồng thời), Bài 1.2 (kiểm tra cổng), Bài 4.3 (tra cứu tên
// qua UDP), Bài 7.4 (SSE). Go không có thư viện giao diện chuẩn, nên giao diện là một trang web cục bộ:
// chương trình Go phục vụ trang và API, mỗi tác vụ là một goroutine có context để hủy, tiến trình
// được đẩy tới trình duyệt bằng Server-Sent Events.
//
//	out/app server <host> <port>                            máy chủ thử (HTTP, "DNS" UDP, cổng mở)
//	out/app ui <host> <cổng_giao_diện> <host_thử> <port_thử>   chương trình giao diện
package main

import (
	"fmt"
	"net"
	"os"
	"strconv"
)

func main() {
	a := os.Args
	switch {
	case len(a) >= 4 && a[1] == "server":
		p, _ := strconv.Atoi(a[3])
		runTestServer(a[2], p)
	case len(a) >= 6 && a[1] == "ui":
		p, _ := strconv.Atoi(a[3])
		runUI(a[2], p, net.JoinHostPort(a[4], a[5]))
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app server <host> <port>")
		fmt.Println("  out/app ui <host> <cổng_giao_diện> <host_thử> <port_thử>")
		os.Exit(1)
	}
}
