// Bài mở rộng MR8.3 – Phòng thủ tấn công từ chối dịch vụ mức ứng dụng kiểu Slowloris.
//
// Bài mở rộng, KHÔNG có trong giáo trình; nối tiếp Chương 6 (server nhiều client) và
// Chương 8 (vận hành an toàn). Mục đích DUY NHẤT là dạy cách *phòng thủ*: so sánh một server
// HTTP/1.1 nhỏ ở hai chế độ và đo xem một yêu cầu bình thường còn được phục vụ hay không.
//
//	app server vulnerable <host> <port>   – nhóm goroutine cố định, KHÔNG đặt hạn đọc tiêu đề
//	app server protected  <host> <port>   – có hạn đọc tiêu đề, cỡ tiêu đề tối đa, hạn chót rỗi,
//	                                        giới hạn kết nối theo IP và trần kết nối toàn cục (503)
//	app slow   <host> <port> [N] [nguồn]  – CLIENT THÍ NGHIỆM: mở N kết nối tới 127.0.0.1 và nhỏ
//	                                        giọt từng byte tiêu đề (chỉ cho loopback)
//	app probe  <host> <port>              – gửi MỘT yêu cầu HTTP bình thường, đo thành công/độ trễ
//
// CẢNH BÁO ĐẠO ĐỨC: vai "slow" chỉ dùng trong phòng thí nghiệm cô lập, chỉ nhắm 127.0.0.1 của
// chính máy bạn, để kiểm chứng biện pháp phòng thủ. Tuyệt đối không nhắm máy/dịch vụ của người khác.
package main

import (
	"fmt"
	"os"
	"strconv"
)

func main() {
	a := os.Args
	switch {
	case len(a) >= 5 && a[1] == "server":
		chayServer(a[2], a[3], a[4])
	case len(a) >= 4 && a[1] == "slow":
		n := 150
		if len(a) > 4 {
			n, _ = strconv.Atoi(a[4])
		}
		nguon := "127.0.0.2"
		if len(a) > 5 {
			nguon = a[5]
		}
		chaySlow(a[2], a[3], n, nguon)
	case len(a) >= 4 && a[1] == "probe":
		chayProbe(a[2], a[3])
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  app server vulnerable|protected <host> <port>")
		fmt.Println("  app slow  <host> <port> [N] [nguồn]   (chỉ 127.0.0.1)")
		fmt.Println("  app probe <host> <port>")
		os.Exit(1)
	}
}
